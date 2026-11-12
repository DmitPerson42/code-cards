import type { ReactNode } from "react";

// Подсветка совпадений в сниппете поиска.
// Сниппет от Rust приходит чистым текстом, без маркеров: FTS5 над таблицей
// без содержимого не умеет highlight(). Поэтому совпадения находим сами и
// отдаём разбиение на части, которые React рисует как <mark>.
// Сырой HTML не вставляется нигде: dangerouslySetInnerHTML не используется.

export interface TextPart {
  text: string;
  /** true - часть совпала с запросом и подсвечивается. */
  hit: boolean;
}

/** Экранирование для регулярного выражения: запрос не должен быть шаблоном. */
function escapeRegExp(value: string): string {
  return value.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}

/**
 * Слова запроса, которые имеет смысл искать в сниппете:
 * пробелы отбрасываются, длинные идут первыми, чтобы `lru_cache`
 * подсвечивался целиком, а не как `lru`.
 */
export function queryPieces(query: string): string[] {
  const pieces = query
    .split(/\s+/)
    .map((piece) => piece.trim())
    .filter((piece) => piece !== "");
  // Дубли без учёта регистра не нужны, порядок сохраняем.
  const unique: string[] = [];
  for (const piece of pieces) {
    if (!unique.some((known) => known.toLowerCase() === piece.toLowerCase())) {
      unique.push(piece);
    }
  }
  return unique.sort((first, second) => second.length - first.length);
}

/**
 * Разбивает текст на части с пометкой совпадений.
 * Пустой запрос возвращает одну часть без подсветки.
 */
export function highlightParts(text: string, query: string): TextPart[] {
  const pieces = queryPieces(query);
  if (text === "" || pieces.length === 0) {
    return [{ text, hit: false }];
  }

  const pattern = pieces.map(escapeRegExp).join("|");
  let regex: RegExp;
  try {
    regex = new RegExp(`(${pattern})`, "giu");
  } catch {
    // Некорректный шаблон не должен ломать список: показываем текст как есть.
    return [{ text, hit: false }];
  }

  const parts: TextPart[] = [];
  let lastIndex = 0;
  for (const match of text.matchAll(regex)) {
    const index = match.index ?? 0;
    if (index > lastIndex) {
      parts.push({ text: text.slice(lastIndex, index), hit: false });
    }
    parts.push({ text: match[0], hit: true });
    lastIndex = index + match[0].length;
  }
  if (lastIndex < text.length) {
    parts.push({ text: text.slice(lastIndex), hit: false });
  }
  return parts.length > 0 ? parts : [{ text, hit: false }];
}

export interface HighlightedTextProps {
  /** Текст для вывода. */
  text: string;
  /** Запрос пользователя. */
  query: string;
}

/** Текст с подсвеченными совпадениями. Собирается из узлов React, не из HTML. */
export function HighlightedText({ text, query }: HighlightedTextProps): ReactNode {
  const parts = highlightParts(text, query);
  return (
    <>
      {parts.map((part, index) =>
        part.hit ? (
          <mark key={index} className="search-hit">
            {part.text}
          </mark>
        ) : (
          <span key={index}>{part.text}</span>
        ),
      )}
    </>
  );
}
