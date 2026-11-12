import { invoke } from "@tauri-apps/api/core";

// Обёртки invoke(): единственное место, где фронтенд говорит с Rust.
// SQL во фронтенде нет и быть не должно (правило 4 плана).
// Типы ниже повторяют serde-структуры из src-tauri/src/models.rs (camelCase).

/**
 * Достаёт текст ошибки из того, что бросил invoke.
 *
 * Rust-команды возвращают ошибку строкой, а не объектом Error, поэтому
 * `error.message` у неё пустой и в интерфейсе появлялось "undefined".
 * Здесь строка превращается в читаемый текст, а для настоящего Error
 * берётся его message.
 */
export function errorText(error: unknown): string {
  if (typeof error === "string" && error.trim() !== "") {
    return error;
  }
  if (error instanceof Error && error.message.trim() !== "") {
    return error.message;
  }
  if (error && typeof error === "object" && "message" in error) {
    const message = (error as { message?: unknown }).message;
    if (typeof message === "string" && message.trim() !== "") {
      return message;
    }
  }
  if (error === null || error === undefined) {
    return "неизвестная ошибка. Подробности в логе приложения";
  }
  const text = String(error);
  return text === "[object Object]" ? "неизвестная ошибка. Подробности в логе приложения" : text;
}

export interface AppInfo {
  appVersion: string;
  sqliteVersion: string;
  fts5Available: boolean;
}

/** Версия приложения, версия SQLite и наличие FTS5. */
export function appInfo(): Promise<AppInfo> {
  return invoke<AppInfo>("app_info");
}

// ---------------------------------------------------------------------------
// Общие типы
// ---------------------------------------------------------------------------

/** Режим синтаксиса слотов у карточки. */
export type SlotSyntax = "double_brace" | "jinja_off";

/** Значение по умолчанию в Rust: двойные фигурные скобки. */
export const SLOT_DOUBLE_BRACE: SlotSyntax = "double_brace";

/** Порядок сортировки списка карточек (поле Filter.sort). */
export type CardSort = "recent" | "alpha" | "created";

/**
 * Фильтр списка и поиска. Пока передаются язык, избранное и сортировка;
 * теги и подборки подключаются на этапах 3 и 5.
 */
export interface Filter {
  tags?: string[];
  language?: string | null;
  collectionId?: number | null;
  favoriteOnly?: boolean;
  sort?: CardSort | null;
}

/** Краткая карточка для сетки списка. */
export interface CardSummary {
  id: number;
  title: string;
  description: string;
  /** Язык первого блока: в сетке один основной язык. */
  language: string;
  tags: string[];
  /** Первые строки кода первого блока для превью. */
  preview: string;
  favorite: boolean;
  isTemplate: boolean;
  createdAt: string;
  updatedAt: string;
  /** Фрагмент с совпадением из FTS, только в результатах поиска. */
  snippet?: string | null;
}

/** Пояснение к диапазону строк блока. */
export interface LineNote {
  id?: number | null;
  lineFrom: number;
  lineTo: number;
  text: string;
  isStale: boolean;
  sortOrder: number;
}

/** Слот шаблона. */
export interface Slot {
  id?: number | null;
  name: string;
  defaultValue: string;
  variants: string[];
}

/** Один блок кода карточки. */
export interface CodeBlock {
  id?: number | null;
  language: string;
  code: string;
  sortOrder: number;
  notes: LineNote[];
  slots: Slot[];
}

/** Полная карточка для экрана просмотра и редактора. */
export interface CardDetail {
  id?: number | null;
  title: string;
  description: string;
  isTemplate: boolean;
  slotSyntax: SlotSyntax;
  favorite: boolean;
  createdAt: string;
  updatedAt: string;
  checkedVersion: string;
  checkedAt: string;
  tags: string[];
  blocks: CodeBlock[];
}

/** Сводка переноса разбора: сдвинулось, изменилось, потерялось. */
export interface MovedSummary {
  moved: number;
  changed: number;
  lost: number;
}

/**
 * Результат сохранения. Первый вызов может вернуть needsConfirmation:
 * тогда фронтенд показывает сводку и повторяет вызов с force: true.
 */
export type SaveResult =
  | { status: "ok"; id: number }
  | { status: "needsConfirmation"; summary: MovedSummary };

// ---------------------------------------------------------------------------
// Карточки
// ---------------------------------------------------------------------------

/** Список карточек с учётом фильтра и сортировки. */
export function cardsList(filter: Filter = {}): Promise<CardSummary[]> {
  return invoke<CardSummary[]>("cards_list", { filter });
}

/** Одна карточка со всеми блоками, разбором и слотами. */
export function cardGet(id: number): Promise<CardDetail> {
  return invoke<CardDetail>("card_get", { id });
}

/**
 * Сохранение карточки. force: true подтверждает перенос разбора
 * и снимает необходимость второго раунда.
 */
export function cardSave(detail: CardDetail, force = false): Promise<SaveResult> {
  return invoke<SaveResult>("card_save", { detail, force });
}

/** Удаление карточки вместе с блоками, разбором и слотами. */
export function cardDelete(id: number): Promise<void> {
  return invoke<void>("card_delete", { id });
}

/**
 * Переключение избранного. Возвращает новое значение избранного,
 * поэтому повторно читать карточку не нужно.
 */
export function cardToggleFavorite(id: number): Promise<boolean> {
  return invoke<boolean>("card_toggle_favorite", { id });
}

// ---------------------------------------------------------------------------
// Поиск
// ---------------------------------------------------------------------------

/**
 * Поиск по названию, описанию, тегам, коду и разбору (раздел 3.2 плана).
 * Поиск точный: слова ищутся как префиксы, пунктуация отдельным проходом.
 * В результатах у карточки заполнен snippet: строка вокруг первого совпадения
 * без разметки, подсветка совпадений строится на клиенте.
 */
export function search(query: string, filter: Filter = {}): Promise<CardSummary[]> {
  return invoke<CardSummary[]>("search", { query, filter });
}

// ---------------------------------------------------------------------------
// Теги
// ---------------------------------------------------------------------------

/**
 * Список всех тегов с числом карточек. Теги без карточек тоже приходят:
 * пользователь мог снять тег и захочет вернуть его.
 */
export function tagList(): Promise<TagWithCount[]> {
  return invoke<TagWithCount[]>("tag_list");
}

/**
 * Полностью заменяет набор тегов карточки (этап 3).
 * Используется только для уже сохранённой карточки: у новой id ещё нет,
 * её теги уходят вместе с card_save.
 */
export function cardSetTags(cardId: number, tags: string[]): Promise<void> {
  return invoke<void>("card_set_tags", { cardId, tags });
}

/** Тег с числом карточек. */
export interface TagWithCount {
  id: number;
  name: string;
  color: string;
  count: number;
}

// ---------------------------------------------------------------------------
// Подборки (этап 5)
// ---------------------------------------------------------------------------

/**
 * Узел дерева подборок. Дерево приходит целиком одним вызовом:
 * порядок задаёт Rust (sortOrder, затем имя без учёта регистра).
 */
export interface CollectionNode {
  id: number;
  name: string;
  parentId: number | null;
  sortOrder: number;
  /** Число карточек именно в этой подборке, без суммирования по детям. */
  cardCount: number;
  children: CollectionNode[];
}

/** Дерево подборок целиком. */
export function collectionList(): Promise<CollectionNode[]> {
  return invoke<CollectionNode[]>("collection_list");
}

/**
 * Создание или переименование подборки. id = null создаёт новую.
 * Возвращает id подборки. Циклы и пустое имя отклоняются в Rust.
 */
export function collectionSave(
  id: number | null,
  name: string,
  parentId: number | null,
): Promise<number> {
  return invoke<number>("collection_save", { id, name, parentId });
}

/** Удаление подборки вместе с вложенными. Карточки остаются в библиотеке. */
export function collectionDelete(id: number): Promise<void> {
  return invoke<void>("collection_delete", { id });
}

/**
 * Задаёт состав и порядок карточек подборки целиком (порядок 0..n).
 * Используется для переноса карточки и для смены порядка перетаскиванием.
 */
export function collectionSetCards(collectionId: number, cardIds: number[]): Promise<void> {
  return invoke<void>("collection_set_cards", { collectionId, cardIds });
}

/** Добавляет карточку в конец подборки. Повторное добавление безвредно. */
export function collectionAddCard(collectionId: number, cardId: number): Promise<void> {
  return invoke<void>("collection_add_card", { collectionId, cardId });
}

// ---------------------------------------------------------------------------
// Слоты и чертежи (этап 6)
// ---------------------------------------------------------------------------

/** Найденный в коде слот. */
export interface FoundSlot {
  name: string;
  /** Смещение в символах от начала всего кода. */
  start: number;
  /** Смещение первого символа "}}" (в подстановку не входит). */
  end: number;
  /** Номер строки, 1-based. */
  line: number;
}

/**
 * Ищет {{имя}} в коде блока. При slotSyntax = "jinja_off" всегда пустой массив:
 * карточка в режиме "не слоты" не подсвечивает шаблоны.
 */
export function blockSlotsScan(code: string, slotSyntax: SlotSyntax): Promise<FoundSlot[]> {
  return invoke<FoundSlot[]>("block_slots_scan", { code, slotSyntax });
}

/**
 * Подставляет значения в слоты и возвращает код по строке на блок.
 * Для jinja_off код приходит без изменений.
 */
export function renderCard(
  cardId: number,
  values: Record<string, string>,
): Promise<string[]> {
  return invoke<string[]>("render_card", { cardId, values });
}

/**
 * Превращает выделение в строке в слот {{имя}}.
 * line 1-based, rangeStart/rangeEnd в символах строки, rangeEnd не включается.
 * Возвращает [новый код, значение по умолчанию для слота].
 */
export function makeSlotCommand(
  code: string,
  line: number,
  rangeStart: number,
  rangeEnd: number,
  name: string,
): Promise<[string, string]> {
  return invoke<[string, string]>("make_slot_command", {
    code,
    line,
    rangeStart,
    rangeEnd,
    name,
  });
}

/** Точечное сохранение слотов уже сохранённого блока. */
export function slotsSet(blockId: number, slots: Slot[]): Promise<void> {
  return invoke<void>("slots_set", { blockId, slots });
}

// ---------------------------------------------------------------------------
// Данные и копии (этап 7)
// ---------------------------------------------------------------------------

/** Статистика архива: сколько чего содержит база или файл. */
export interface Stats {
  cards: number;
  blocks: number;
  notes: number;
  slots: number;
  tags: number;
  collections: number;
}

/** Отчёт об импорте. recreated - карточки, созданные заново из-за совпадения id. */
export interface ImportReport {
  cards: number;
  blocks: number;
  notes: number;
  slots: number;
  tags: number;
  collections: number;
  recreated: number;
}

/** Ручная копия базы. Возвращает путь к созданному файлу копии. */
export function backupRun(): Promise<string> {
  return invoke<string>("backup_run");
}

/** Экспорт всего архива в JSON по указанному пути. */
export function backupExportJson(path: string): Promise<Stats> {
  return invoke<Stats>("backup_export_json", { path });
}

/** Импорт архива из JSON. Ничего не перезаписывает, при совпадении id создаёт новое. */
export function backupImportJson(path: string): Promise<ImportReport> {
  return invoke<ImportReport>("backup_import_json", { path });
}

/** Экспорт одной карточки в Markdown по указанному пути. */
export function backupExportMarkdown(cardId: number, path: string): Promise<void> {
  return invoke<void>("backup_export_markdown", { cardId, path });
}
