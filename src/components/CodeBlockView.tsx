import { useEffect, useMemo, useRef } from "react";
import type { FoundSlot } from "../api/tauri";
import {
  EditorState,
  RangeSet,
  StateEffect,
  StateField,
  type Extension,
  type Range,
} from "@codemirror/state";
import {
  Decoration,
  EditorView,
  GutterMarker,
  gutterLineClass,
  keymap,
  lineNumbers,
  highlightActiveLine,
  highlightActiveLineGutter,
  drawSelection,
  dropCursor,
  type DecorationSet,
  type ViewUpdate,
} from "@codemirror/view";
import {
  defaultKeymap,
  indentWithTab,
  history,
  historyKeymap,
} from "@codemirror/commands";
import { bracketMatching, indentOnInput } from "@codemirror/language";
import { python } from "@codemirror/lang-python";
import { javascript } from "@codemirror/lang-javascript";
import { sql } from "@codemirror/lang-sql";
import { html } from "@codemirror/lang-html";
import { css } from "@codemirror/lang-css";
import { markdown } from "@codemirror/lang-markdown";
import { json } from "@codemirror/lang-json";
import { oneDark } from "@codemirror/theme-one-dark";

// Выбор языка один на всё приложение: и редактор, и просмотр берут отсюда.
// Shell и обычный текст идут без подсветки: зависимости lang-legacy в каркасе нет.
export const CODE_LANGUAGES: ReadonlyArray<{ value: string; label: string }> = [
  { value: "python", label: "Python" },
  { value: "javascript", label: "JavaScript" },
  { value: "typescript", label: "TypeScript" },
  { value: "sql", label: "SQL" },
  { value: "html", label: "HTML" },
  { value: "css", label: "CSS" },
  { value: "markdown", label: "Markdown" },
  { value: "json", label: "JSON" },
  { value: "shell", label: "Shell" },
  { value: "text", label: "Текст" },
];

/** Расширение CodeMirror под выбранный язык карточки. */
export function languageExtension(language: string): Extension {
  switch (language) {
    case "python":
      return python();
    case "javascript":
      return javascript();
    case "typescript":
      return javascript({ typescript: true });
    case "sql":
      return sql();
    case "html":
      return html();
    case "css":
      return css();
    case "markdown":
      return markdown();
    case "json":
      return json();
    default:
      return [];
  }
}

/** Диапазон строк разбора: 1-based, включительно. */
export interface LineRange {
  /** Ключ записи: id разбора либо временный ключ новой записи. */
  key: string;
  lineFrom: number;
  lineTo: number;
  isStale: boolean;
}

/** Классы строк: активный диапазон (выбранный разбор) и устаревший. */
const EFFECT_LINES = StateEffect.define<{ ranges: LineRange[]; activeKey: string | null }>();

/** Слоты в коде: смещения от начала всего кода, end указывает на первый "}}". */
const EFFECT_SLOTS = StateEffect.define<FoundSlot[]>();

const LINE_ACTIVE = Decoration.line({ class: "cm-line-note-active" });
const LINE_NOTE = Decoration.line({ class: "cm-line-note-line" });
// Подсветка {{имя}} целиком: от start до end + длина "}}" (класс из styles.css).
const SLOT_MARK = Decoration.mark({ class: "cm-slot-mark" });

/** Маркер слева у строк, у которых есть разбор; у устаревшего он жёлтый. */
class NoteGutterMarker extends GutterMarker {
  private readonly stale: boolean;

  constructor(stale: boolean) {
    super();
    this.stale = stale;
  }

  eq(other: GutterMarker): boolean {
    return other instanceof NoteGutterMarker && other.stale === this.stale;
  }

  toDOM(): Node {
    const dot = document.createElement("span");
    dot.className = this.stale ? "cm-note-dot cm-note-dot-stale" : "cm-note-dot";
    return dot;
  }
}

const NOTE_MARKER = new NoteGutterMarker(false);
const STALE_MARKER = new NoteGutterMarker(true);

/** Собирает декорации строк по диапазонам разбора. */
function buildDecorations(
  state: EditorState,
  ranges: LineRange[],
  activeKey: string | null,
): { lines: DecorationSet; gutter: RangeSet<GutterMarker> } {
  const lineDecorations: Range<Decoration>[] = [];
  const gutterRanges: Range<GutterMarker>[] = [];
  const total = state.doc.lines;

  for (const range of ranges) {
    const from = Math.min(Math.max(range.lineFrom, 1), total);
    const to = Math.min(Math.max(range.lineTo, from), total);
    const isActive = range.key === activeKey;
    const marker = range.isStale ? STALE_MARKER : NOTE_MARKER;

    for (let number = from; number <= to; number += 1) {
      const line = state.doc.line(number);
      lineDecorations.push((isActive ? LINE_ACTIVE : LINE_NOTE).range(line.from));
      gutterRanges.push(marker.range(line.from));
    }
  }

  return {
    lines: Decoration.set(lineDecorations, true),
    gutter: RangeSet.of(gutterRanges, true),
  };
}

/** Декорации строк: фон диапазонов и маркеры в жёлобе. */
interface NoteDecoration {
  lines: DecorationSet;
  gutter: RangeSet<GutterMarker>;
}

/** Поле декораций: держит подсветку диапазонов и пересчитывает её по эффекту. */
const noteDecorations: StateField<NoteDecoration> = StateField.define<NoteDecoration>({
  create: (): NoteDecoration => ({ lines: Decoration.none, gutter: RangeSet.empty }),
  update(value: NoteDecoration, transaction): NoteDecoration {
    for (const effect of transaction.effects) {
      if (effect.is(EFFECT_LINES)) {
        return buildDecorations(transaction.state, effect.value.ranges, effect.value.activeKey);
      }
    }
    // Правка текста меняет номера строк: подсветку снимаем до следующего
    // пересчёта диапазонов, чтобы она не показывала чужие строки.
    if (transaction.docChanged) {
      return { lines: Decoration.none, gutter: RangeSet.empty };
    }
    return value;
  },
  provide: (field) => [
    EditorView.decorations.from(field, (value: NoteDecoration) => value.lines),
    gutterLineClass.from(field, (value: NoteDecoration) => value.gutter),
  ],
});

/** Подсветка слотов {{имя}}: отдельное поле, оно не мешает декорациям разбора. */
const slotDecorations: StateField<DecorationSet> = StateField.define<DecorationSet>({
  create: () => Decoration.none,
  update(value, transaction) {
    for (const effect of transaction.effects) {
      if (effect.is(EFFECT_SLOTS)) {
        const decorations: Range<Decoration>[] = [];
        const length = transaction.state.doc.length;
        for (const slot of effect.value) {
          // end указывает на первый символ закрывающих скобок, добавляем два символа.
          const from = Math.max(0, Math.min(slot.start, length));
          const to = Math.max(from, Math.min(slot.end + 2, length));
          if (to > from) {
            decorations.push(SLOT_MARK.range(from, to));
          }
        }
        return Decoration.set(decorations, true);
      }
    }
    // Текст изменился: старые смещения недействительны до нового сканирования.
    if (transaction.docChanged) {
      return Decoration.none;
    }
    return value;
  },
  provide: (field) => EditorView.decorations.from(field),
});

/** Выделение в редакторе: нужно кнопке "Сделать чертёж из выделенного". */
export interface SelectionInfo {
  /** Строка выделения, 1-based. */
  line: number;
  /** Смещения в символах строки, конец не включается. */
  rangeStart: number;
  rangeEnd: number;
}

/** Сообщает наружу выделение внутри одной строки; иначе приходит null. */
const selectionListener = (onSelection: (selection: SelectionInfo | null) => void): Extension =>
  EditorView.updateListener.of((update: ViewUpdate) => {
    if (!update.selectionSet && !update.docChanged) {
      return;
    }
    const range = update.state.selection.main;
    if (range.empty) {
      onSelection(null);
      return;
    }
    const doc = update.state.doc;
    const lineFrom = doc.lineAt(range.from);
    const lineTo = doc.lineAt(range.to);
    if (lineFrom.number !== lineTo.number) {
      // Чертёж делается из фрагмента одной строки: перенос на несколько строк не поддержан.
      onSelection(null);
      return;
    }
    onSelection({
      line: lineFrom.number,
      rangeStart: range.from - lineFrom.from,
      rangeEnd: range.to - lineFrom.from,
    });
  });

/** Сообщает наружу номер строки под курсором и по щелчку мышью. */
const cursorListener = (onLine: (line: number) => void): Extension =>
  EditorView.updateListener.of((update: ViewUpdate) => {
    if (!update.selectionSet && !update.docChanged) {
      return;
    }
    const head = update.state.selection.main.head;
    onLine(update.state.doc.lineAt(head).number);
  });

/** Обработчик одиночного щелчка: сообщает строку, по которой кликнули. */
const clickListener = (onLine: (line: number) => void): Extension =>
  EditorView.domEventHandlers({
    click(event, view) {
      const position = view.posAtCoords({ x: event.clientX, y: event.clientY });
      if (position == null) {
        return false;
      }
      onLine(view.state.doc.lineAt(position).number);
      return false;
    },
  });

/** Сдвигает курсор на указанную строку (переход из панели разбора в код). */
const effectFocusLine = StateEffect.define<number>();

const focusLineListener = EditorView.updateListener.of((update: ViewUpdate) => {
  for (const effect of update.transactions.flatMap((transaction) => transaction.effects)) {
    if (!effect.is(effectFocusLine)) {
      continue;
    }
    const total = update.state.doc.lines;
    const number = Math.min(Math.max(effect.value, 1), total);
    const line = update.state.doc.line(number);
    update.view.dispatch({
      selection: { anchor: line.from },
      scrollIntoView: true,
    });
  }
});

/** Номер строки, которую надо подсветить курсором. */
const FOCUS_LINE: Extension = focusLineListener;

export interface CodeBlockViewProps {
  /** Текст кода блока. */
  value: string;
  /** Язык блока (значение из CODE_LANGUAGES). */
  language: string;
  /** true - редактор с курсором и клавиатурой, false - только просмотр. */
  editable: boolean;
  /** Вызывается на каждое изменение текста в режиме редактирования. */
  onChange?: (value: string) => void;
  /** Вызывается при Ctrl+S внутри редактора. */
  onSaveShortcut?: () => void;
  /** Диапазоны разбора для подсветки строк. */
  ranges?: LineRange[];
  /** Ключ разбора, который сейчас выбран в панели. */
  activeKey?: string | null;
  /** Строка, на которую нужно перейти (клик по записи разбора). */
  focusLine?: number | null;
  /** Растёт при каждом новом запросе перехода: повторный щелчок тоже прокручивает. */
  focusToken?: number;
  /** Щелчок или перемещение курсора по строке: номер строки 1-based. */
  onLineActivate?: (line: number) => void;
  /** Слоты для подсветки {{имя}}. Пустой массив снимает подсветку. */
  slots?: FoundSlot[];
  /** Выделение внутри одной строки: для кнопки "Сделать чертёж". */
  onSelectionChange?: (selection: SelectionInfo | null) => void;
}

/**
 * Блок кода на CodeMirror 6 с нумерацией строк (раздел 3.3 плана).
 * Редактор создаётся один раз; изменения текста приходят снаружи через
 * отдельный эффект, поэтому курсор и прокрутка не сбрасываются.
 */
export default function CodeBlockView({
  value,
  language,
  editable,
  onChange,
  onSaveShortcut,
  ranges,
  activeKey = null,
  focusLine = null,
  focusToken = 0,
  onLineActivate,
  slots,
  onSelectionChange,
}: CodeBlockViewProps) {
  const hostRef = useRef<HTMLDivElement | null>(null);
  const viewRef = useRef<EditorView | null>(null);

  // Колбэки в ref: сам EditorView не должен зависеть от новых ссылок на функции.
  const onChangeRef = useRef(onChange);
  const onSaveRef = useRef(onSaveShortcut);
  const onLineRef = useRef(onLineActivate);
  const onSelectionRef = useRef(onSelectionChange);
  useEffect(() => {
    onChangeRef.current = onChange;
    onSaveRef.current = onSaveShortcut;
    onLineRef.current = onLineActivate;
    onSelectionRef.current = onSelectionChange;
  }, [onChange, onSaveShortcut, onLineActivate, onSelectionChange]);

  // Набор расширений зависит от языка и режима: при их смене редактор пересобирается.
  const extensions = useMemo<Extension[]>(() => {
    const list: Extension[] = [
      lineNumbers(),
      highlightActiveLineGutter(),
      history(),
      drawSelection(),
      dropCursor(),
      indentOnInput(),
      bracketMatching(),
      oneDark,
      EditorView.lineWrapping,
      languageExtension(language),
      EditorState.readOnly.of(!editable),
      EditorView.editable.of(editable),
      noteDecorations,
      slotDecorations,
      FOCUS_LINE,
      cursorListener((line) => onLineRef.current?.(line)),
      clickListener((line) => onLineRef.current?.(line)),
      selectionListener((selection) => onSelectionRef.current?.(selection)),
    ];
    if (editable) {
      list.push(highlightActiveLine());
      list.push(
        keymap.of([
          {
            key: "Mod-s",
            preventDefault: true,
            run: () => {
              onSaveRef.current?.();
              return true;
            },
          },
          ...defaultKeymap,
          ...historyKeymap,
          indentWithTab,
        ]),
      );
      list.push(
        EditorView.updateListener.of((update) => {
          if (update.docChanged) {
            onChangeRef.current?.(update.state.doc.toString());
          }
        }),
      );
    }
    return list;
  }, [editable, language]);

  useEffect(() => {
    const host = hostRef.current;
    if (!host) {
      return;
    }
    const view = new EditorView({
      state: EditorState.create({ doc: value, extensions }),
      parent: host,
    });
    viewRef.current = view;
    return () => {
      view.destroy();
      viewRef.current = null;
    };
    // value намеренно не в зависимостях: начальный текст берётся один раз.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [extensions]);

  // Внешняя замена текста (открыли другую карточку, откатили правку).
  useEffect(() => {
    const view = viewRef.current;
    if (!view) {
      return;
    }
    const current = view.state.doc.toString();
    if (current !== value) {
      view.dispatch({
        changes: { from: 0, to: current.length, insert: value },
      });
    }
  }, [value]);

  // Подсветка строк разбора и выбранного диапазона.
  useEffect(() => {
    const view = viewRef.current;
    if (!view) {
      return;
    }
    view.dispatch({
      effects: EFFECT_LINES.of({ ranges: ranges ?? [], activeKey }),
    });
  }, [ranges, activeKey]);

  // Текст поменялся: декорации пересчитываются от новых номеров строк.
  // ranges приходят новыми объектами при каждой правке кода, поэтому
  // достаточно повторно применить те же диапазоны.

  // Переход к строке из панели разбора.
  useEffect(() => {
    const view = viewRef.current;
    if (!view || focusLine == null) {
      return;
    }
    view.dispatch({ effects: effectFocusLine.of(focusLine) });
  }, [focusLine, focusToken]);

  // Подсветка слотов: смещения приходят от Rust (block_slots_scan).
  useEffect(() => {
    const view = viewRef.current;
    if (!view) {
      return;
    }
    view.dispatch({ effects: EFFECT_SLOTS.of(slots ?? []) });
  }, [slots]);

  return <div ref={hostRef} className="code-surface" />;
}
