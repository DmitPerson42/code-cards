import { useCallback, useMemo, useRef, useState } from "react";
import type { LineNote } from "../api/tauri";
import type { LineRange } from "../components/CodeBlockView";

// Состояние разбора по строкам: черновик записей, выбранная запись,
// строка под курсором и счётчик временных ключей.
// Логика одна для редактора и для просмотра, поэтому вынесена в хук.

/** Запись разбора в черновике: id есть только у сохранённых. */
export interface DraftNote extends LineNote {
  /** Ключ для React и подсветки: постоянный или временный. */
  key: string;
}

let temporaryCounter = 0;

/** Новый временный ключ для записи, которой ещё нет в базе. */
export function newNoteKey(): string {
  temporaryCounter += 1;
  return `new-${temporaryCounter}`;
}

/** Превращает записи карточки в черновик с ключами. */
export function toDraft(notes: LineNote[]): DraftNote[] {
  return notes.map((note, index) => ({
    ...note,
    key: note.id != null ? `id-${note.id}` : newNoteKey(),
    sortOrder: note.sortOrder ?? index,
  }));
}

export interface NotesState {
  notes: DraftNote[];
  setNotes: (notes: DraftNote[]) => void;
  selectedKey: string | null;
  setSelectedKey: (key: string | null) => void;
  /** Строка под курсором в коде, 1-based. */
  hoverLine: number | null;
  setHoverLine: (line: number | null) => void;
  /** Строка, на которую просили перейти из панели. */
  focusLine: number | null;
  /** Номер попытки перехода: меняется при каждом щелчке по записи. */
  focusToken: number;
  /** Диапазоны для декораций CodeMirror. */
  ranges: LineRange[];
  /** Записи, которые показываются активными: диапазон под курсором. */
  hitKeys: string[];
  /** Выбрать запись и перейти к её первой строке в коде. */
  selectNote: (key: string) => void;
  /** Щелчок или перемещение по строке кода: выбрать покрывающую запись. */
  activateLine: (line: number) => void;
  /** Добавить запись под строку. */
  addNote: (lineFrom: number, lineTo: number) => DraftNote;
  /** Добавить сразу несколько записей и вернуть их в порядке добавления. */
  addManyNotes: (ranges: Array<[number, number]>) => DraftNote[];
  /** Обновить поля записи. */
  updateNote: (key: string, patch: Partial<LineNote>) => void;
  /** Удалить запись. */
  removeNote: (key: string) => void;
}

/**
 * Хук разбора. initialNotes берутся один раз при создании: перезагрузка
 * после сохранения пересоздаёт компонент через key.
 */
export function useNotes(initialNotes: LineNote[]): NotesState {
  const [notes, setNotesState] = useState<DraftNote[]>(() => toDraft(initialNotes));
  // Тот же список для обработчиков, которые вызываются уже после рендера.
  const notesRef = useRef<DraftNote[]>(notes);
  const [selectedKey, setSelectedKey] = useState<string | null>(null);
  const [hoverLine, setHoverLine] = useState<number | null>(null);
  const [focus, setFocus] = useState<{ line: number; token: number } | null>(null);
  const focusCounter = useRef(0);

  const commit = useCallback((next: DraftNote[]) => {
    notesRef.current = next;
    setNotesState(next);
  }, []);

  const ranges = useMemo<LineRange[]>(
    () =>
      notes.map((note) => ({
        key: note.key,
        lineFrom: note.lineFrom,
        lineTo: note.lineTo,
        isStale: note.isStale,
      })),
    [notes],
  );

  const hitKeys = useMemo(() => {
    if (hoverLine == null) {
      return [];
    }
    return notes
      .filter((note) => hoverLine >= note.lineFrom && hoverLine <= note.lineTo)
      .map((note) => note.key);
  }, [notes, hoverLine]);

  const selectNote = useCallback(
    (key: string) => {
      setSelectedKey(key);
      const note = notes.find((item) => item.key === key);
      if (note !== undefined) {
        // Повторный щелчок по той же записи должен снова прокрутить код,
        // поэтому рядом со строкой храним номер попытки.
        focusCounter.current += 1;
        setFocus({ line: note.lineFrom, token: focusCounter.current });
      }
    },
    [notes],
  );

  const activateLine = useCallback(
    (line: number) => {
      setHoverLine(line);
      const hit = notes.find((note) => line >= note.lineFrom && line <= note.lineTo);
      if (hit !== undefined) {
        setSelectedKey(hit.key);
      }
    },
    [notes],
  );

  /** Создаёт пачку записей одним обновлением состояния. */
  const addManyNotes = useCallback(
    (list: Array<[number, number]>) => {
      const base = notesRef.current;
      const created: DraftNote[] = list.map(([lineFrom, lineTo], index) => {
        const from = Math.max(1, Math.min(lineFrom, lineTo));
        const to = Math.max(from, Math.max(lineFrom, lineTo));
        return {
          key: newNoteKey(),
          id: null,
          lineFrom: from,
          lineTo: to,
          text: "",
          isStale: false,
          sortOrder: base.length + index,
        };
      });
      commit([...base, ...created]);
      return created;
    },
    [commit],
  );

  const addNote = useCallback(
    (lineFrom: number, lineTo: number) => {
      const [note] = addManyNotes([[lineFrom, lineTo]]);
      return note;
    },
    [addManyNotes],
  );

  const updateNote = useCallback(
    (key: string, patch: Partial<LineNote>) => {
      commit(
        notes.map((note) => {
          if (note.key !== key) {
            return note;
          }
          const merged = { ...note, ...patch };
          // Конец диапазона не может быть раньше начала.
          merged.lineFrom = Math.max(1, merged.lineFrom);
          merged.lineTo = Math.max(merged.lineFrom, merged.lineTo);
          return merged;
        }),
      );
    },
    [notes, commit],
  );

  const removeNote = useCallback(
    (key: string) => {
      commit(
        notes
          .filter((note) => note.key !== key)
          .map((note, index) => ({ ...note, sortOrder: index })),
      );
      setSelectedKey((current) => (current === key ? null : current));
    },
    [notes, commit],
  );

  return {
    notes,
    setNotes: commit,
    selectedKey,
    setSelectedKey,
    hoverLine,
    setHoverLine,
    focusLine: focus === null ? null : focus.line,
    focusToken: focus === null ? 0 : focus.token,
    ranges,
    hitKeys,
    selectNote,
    activateLine,
    addNote,
    addManyNotes,
    updateNote,
    removeNote,
  };
}

/** Сводная статистика разбора для заголовков и сводки. */
export function notesStats(notes: DraftNote[]): {
  total: number;
  stale: number;
  empty: number;
} {
  return {
    total: notes.length,
    stale: notes.filter((note) => note.isStale).length,
    empty: notes.filter((note) => note.text.trim() === "").length,
  };
}
