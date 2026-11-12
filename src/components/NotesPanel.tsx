import { useEffect, useMemo, useRef } from "react";
import { App, Button, Empty, Input, InputNumber, Space, Tag, Tooltip, Typography } from "antd";
import {
  DeleteOutlined,
  PlusOutlined,
  SplitCellsOutlined,
} from "@ant-design/icons";
import type { DraftNote, NotesState } from "../hooks/useNotes";
import { notesStats } from "../hooks/useNotes";

// Панель разбора по строкам (раздел 3.3 плана): список диапазонов и текстов.
// Двусторонняя подсветка: строка под курсором в коде подсвечивает запись,
// щелчок по записи переводит код на её первую строку.

export interface NotesPanelProps {
  /** Состояние разбора из useNotes. */
  notes: NotesState;
  /** Код блока: нужен для кнопки "Разбить на строки". */
  code: string;
  /** true - можно править диапазоны и тексты, false - только смотреть. */
  editable: boolean;
  /** Заголовок панели: разбор какого блока показан. */
  title?: string;
}

/** Строки кода, которые стоит разбирать: без пустых и без одних отступов. */
export function splittableLines(code: string): number[] {
  const result: number[] = [];
  const lines = code.split("\n");
  lines.forEach((line, index) => {
    if (line.trim() !== "") {
      result.push(index + 1);
    }
  });
  return result;
}

export default function NotesPanel({ notes, code, editable, title }: NotesPanelProps) {
  const { message } = App.useApp();
  const stats = useMemo(() => notesStats(notes.notes), [notes.notes]);
  const lineCount = useMemo(() => code.split("\n").length, [code]);
  // Первое поле пустой записи, куда надо поставить фокус.
  const pendingFocus = useRef<string | null>(null);
  const textRefs = useRef(new Map<string, HTMLTextAreaElement | null>());

  // Фокус в первую пустую запись после "Разбить на строки".
  useEffect(() => {
    const key = pendingFocus.current;
    if (key == null) {
      return;
    }
    const element = textRefs.current.get(key);
    if (element) {
      element.focus();
      pendingFocus.current = null;
    }
  }, [notes.notes]);

  function findFirstEmpty(notesList: DraftNote[]): DraftNote | undefined {
    return notesList.find((note) => note.text.trim() === "");
  }

  function handleSplit() {
    const lines = splittableLines(code);
    if (lines.length === 0) {
      return;
    }
    // Уже разобранные строки не дублируем.
    const covered = new Set<number>();
    notes.notes.forEach((note) => {
      for (let line = note.lineFrom; line <= note.lineTo; line += 1) {
        covered.add(line);
      }
    });
    const missing = lines.filter((line) => !covered.has(line));
    if (missing.length === 0) {
      message.info("Все непустые строки уже разобраны");
      return;
    }

    const created = notes.addManyNotes(missing.map((line) => [line, line] as [number, number]));
    // Фокус ставится в первую новую запись, как требует раздел 3.3 плана.
    pendingFocus.current = created[0]?.key ?? null;
    notes.setSelectedKey(created[0]?.key ?? null);
  }

  const firstEmpty = findFirstEmpty(notes.notes);

  return (
    <div style={{ display: "flex", flexDirection: "column", gap: 8, height: "100%", minHeight: 0 }}>
      <Space wrap style={{ justifyContent: "space-between", width: "100%" }}>
        <Space size={6}>
          <Typography.Text strong>{title ?? "Разбор по строкам"}</Typography.Text>
          <Tag>{`записей: ${stats.total}`}</Tag>
          {stats.stale > 0 && (
            <Tag color="warning">{`устарело: ${stats.stale}`}</Tag>
          )}
          {stats.empty > 0 && (
            <Tag color="default">{`пустых: ${stats.empty}`}</Tag>
          )}
          <Typography.Text type="secondary" className="count-chip">
            {`строк: ${lineCount}`}
          </Typography.Text>
        </Space>
        {editable && (
          <Space size={4}>
            <Tooltip title="Пустая запись под каждую непустую строку кода">
              <Button size="small" icon={<SplitCellsOutlined />} onClick={handleSplit}>
                Разбить на строки
              </Button>
            </Tooltip>
            <Tooltip title={firstEmpty ? "Перейти к первой пустой записи" : "Пустых записей нет"}>
              <Button
                size="small"
                disabled={firstEmpty === undefined}
                onClick={() => {
                  if (firstEmpty !== undefined) {
                    notes.selectNote(firstEmpty.key);
                  }
                }}
              >
                К пустой
              </Button>
            </Tooltip>
            <Tooltip title="Добавить диапазон">
              <Button
                size="small"
                icon={<PlusOutlined />}
                onClick={() => {
                  const from = notes.hoverLine ?? 1;
                  const note = notes.addNote(from, from);
                  notes.setSelectedKey(note.key);
                  pendingFocus.current = note.key;
                }}
              />
            </Tooltip>
          </Space>
        )}
      </Space>

      {notes.notes.length === 0 && (
        <Empty
          image={Empty.PRESENTED_IMAGE_SIMPLE}
          description="Разбора пока нет"
        />
      )}

      <div className="notes-panel">
        {notes.notes.map((note) => (
          <NoteItem
            key={note.key}
            note={note}
            editable={editable}
            selected={note.key === notes.selectedKey}
            covered={notes.hitKeys.includes(note.key)}
            maxLine={lineCount}
            onSelect={() => notes.selectNote(note.key)}
            onHover={(line) => notes.setHoverLine(line)}
            onChange={(patch) => notes.updateNote(note.key, patch)}
            onRemove={() => notes.removeNote(note.key)}
            registerRef={(element) => {
              if (element) {
                textRefs.current.set(note.key, element);
              } else {
                textRefs.current.delete(note.key);
              }
            }}
          />
        ))}
      </div>
    </div>
  );
}

interface NoteItemProps {
  note: DraftNote;
  editable: boolean;
  selected: boolean;
  /** Строка под курсором в коде внутри диапазона этой записи. */
  covered: boolean;
  maxLine: number;
  onSelect: () => void;
  onHover: (line: number | null) => void;
  onChange: (patch: Partial<DraftNote>) => void;
  onRemove: () => void;
  registerRef: (element: HTMLTextAreaElement | null) => void;
}

function NoteItem({
  note,
  editable,
  selected,
  covered,
  maxLine,
  onSelect,
  onHover,
  onChange,
  onRemove,
  registerRef,
}: NoteItemProps) {
  const classes = ["note-item"];
  if (selected) {
    classes.push("note-item-active");
  }
  if (note.isStale) {
    classes.push("note-stale");
  }

  return (
    <div
      className={classes.join(" ")}
      // Подсветка строки в коде при наведении на запись.
      onMouseEnter={() => onHover(note.lineFrom)}
      onMouseLeave={() => onHover(null)}
      onClick={onSelect}
      style={{
        outline: covered && !selected ? "1px solid rgba(90,170,255,0.35)" : "none",
      }}
    >
      <div className="note-head">
        <Space size={4}>
          <Typography.Text strong style={{ fontVariantNumeric: "tabular-nums" }}>
            {note.lineFrom === note.lineTo
              ? `стр. ${note.lineFrom}`
              : `стр. ${note.lineFrom}-${note.lineTo}`}
          </Typography.Text>
          {note.isStale && (
            <Tooltip title="Строки изменились после правки кода: проверьте текст">
              <Tag color="warning">устарело</Tag>
            </Tooltip>
          )}
          {note.text.trim() === "" && <Tag color="default">пусто</Tag>}
        </Space>
        {editable && (
          <Space size={2}>
            <Tooltip title="Удалить разбор">
              <Button
                size="small"
                type="text"
                danger
                icon={<DeleteOutlined />}
                onClick={(event) => {
                  event.stopPropagation();
                  onRemove();
                }}
              />
            </Tooltip>
          </Space>
        )}
      </div>

      {editable ? (
        <Space direction="vertical" size={4} style={{ width: "100%" }} onClick={(event) => event.stopPropagation()}>
          <div className="note-range">
            <InputNumber
              size="small"
              min={1}
              max={maxLine}
              value={note.lineFrom}
              onChange={(value) => {
                if (typeof value !== "number") {
                  return;
                }
                onChange({
                  lineFrom: value,
                  lineTo: Math.max(value, note.lineTo),
                });
              }}
              style={{ width: "100%" }}
              aria-label="Строка от"
            />
            <InputNumber
              size="small"
              min={note.lineFrom}
              max={maxLine}
              value={note.lineTo}
              onChange={(value) => {
                if (typeof value === "number") {
                  onChange({ lineTo: value });
                }
              }}
              style={{ width: "100%" }}
              aria-label="Строка до"
            />
          </div>
          <Input.TextArea
            ref={registerRef}
            size="small"
            placeholder="Что делает этот фрагмент"
            value={note.text}
            onChange={(event) => onChange({ text: event.target.value })}
            autoSize={{ minRows: 1, maxRows: 4 }}
          />
        </Space>
      ) : (
        <Typography.Paragraph
          type={note.text.trim() === "" ? "secondary" : undefined}
          style={{ marginBottom: 0, whiteSpace: "pre-wrap" }}
        >
          {note.text.trim() === "" ? "Без пояснения" : note.text}
        </Typography.Paragraph>
      )}
    </div>
  );
}
