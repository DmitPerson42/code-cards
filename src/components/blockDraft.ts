import type { CodeBlock, LineNote, Slot } from "../api/tauri";
import type { DraftNote } from "../hooks/useNotes";

// Черновик блока кода в редакторе: код, язык и разбор в виде черновика.
// Разбор хранится отдельно от CodeBlock, чтобы правки текста не влияли
// на исходные данные карточки до сохранения.

export interface BlockDraft {
  /** Ключ блока внутри редактора. */
  key: string;
  /** id блока в базе, null у нового. */
  id: number | null;
  language: string;
  code: string;
  notes: DraftNote[];
  /** Слоты пока не редактируются (этап 6), но сохраняются без потерь. */
  slots: Slot[];
}

let blockCounter = 0;

/** Новый временный ключ блока. */
export function newBlockKey(): string {
  blockCounter += 1;
  return `block-${blockCounter}`;
}

/**
 * Превращает блоки карточки в черновики редактора.
 * makeKey выдаёт новый временный ключ, toDraft готовит записи разбора.
 */
export function toBlockDrafts(
  blocks: CodeBlock[],
  makeKey: () => string,
  toDraft: (notes: LineNote[]) => DraftNote[],
): BlockDraft[] {
  return blocks.map((block) => ({
    key: makeKey(),
    id: block.id ?? null,
    language: block.language,
    code: block.code,
    notes: toDraft(block.notes),
    slots: block.slots,
  }));
}
