// Схема базы данных. SQL взят из раздела 3.1 плана разработки.
//
// Версия схемы хранится в таблице schema_meta. Миграции только вперёд:
// каждый шаг применяется один раз и переводит версию на единицу выше.
//
// Версия 2 добавила словарь терминов: по просьбе пользователя поиск ищет
// не только по карточкам, но и по словарю. Остальные таблицы версии 1
// не менялись.

use rusqlite::Connection;

/// Текущая версия схемы, которую понимает этот код.
pub const SCHEMA_VERSION: i32 = 2;

/// Полный SQL первой версии схемы.
const SCHEMA_V1: &str = r#"
-- Карточка
CREATE TABLE IF NOT EXISTS cards (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    title         TEXT NOT NULL,
    description   TEXT NOT NULL DEFAULT '',
    is_template   INTEGER NOT NULL DEFAULT 0,
    slot_syntax   TEXT NOT NULL DEFAULT 'double_brace',
    created_at    TEXT NOT NULL,
    updated_at    TEXT NOT NULL,
    favorite      INTEGER NOT NULL DEFAULT 0,
    checked_version TEXT NOT NULL DEFAULT '',
    checked_at    TEXT NOT NULL DEFAULT ''
);

-- Блок кода внутри карточки
CREATE TABLE IF NOT EXISTS code_blocks (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    card_id     INTEGER NOT NULL REFERENCES cards(id) ON DELETE CASCADE,
    language    TEXT NOT NULL DEFAULT 'python',
    code        TEXT NOT NULL DEFAULT '',
    sort_order  INTEGER NOT NULL DEFAULT 0
);

-- Разбор: свободный текст на диапазон строк блока
CREATE TABLE IF NOT EXISTS line_notes (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    block_id    INTEGER NOT NULL REFERENCES code_blocks(id) ON DELETE CASCADE,
    line_from   INTEGER NOT NULL,
    line_to     INTEGER NOT NULL,
    text        TEXT NOT NULL,
    is_stale    INTEGER NOT NULL DEFAULT 0,
    sort_order  INTEGER NOT NULL DEFAULT 0
);

-- Слоты шаблона
CREATE TABLE IF NOT EXISTS slots (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    block_id    INTEGER NOT NULL REFERENCES code_blocks(id) ON DELETE CASCADE,
    name        TEXT NOT NULL,
    default_value TEXT NOT NULL DEFAULT '',
    variants    TEXT NOT NULL DEFAULT '[]'
);

-- Теги
CREATE TABLE IF NOT EXISTS tags (
    id    INTEGER PRIMARY KEY AUTOINCREMENT,
    name  TEXT NOT NULL UNIQUE COLLATE NOCASE,
    color TEXT NOT NULL DEFAULT ''
);

CREATE TABLE IF NOT EXISTS card_tags (
    card_id INTEGER NOT NULL REFERENCES cards(id) ON DELETE CASCADE,
    tag_id  INTEGER NOT NULL REFERENCES tags(id) ON DELETE CASCADE,
    PRIMARY KEY (card_id, tag_id)
);

-- Подборки (дерево)
CREATE TABLE IF NOT EXISTS collections (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    name       TEXT NOT NULL,
    parent_id  INTEGER REFERENCES collections(id) ON DELETE CASCADE,
    sort_order INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS collection_cards (
    collection_id INTEGER NOT NULL REFERENCES collections(id) ON DELETE CASCADE,
    card_id       INTEGER NOT NULL REFERENCES cards(id) ON DELETE CASCADE,
    sort_order    INTEGER NOT NULL DEFAULT 0,
    added_at      TEXT NOT NULL,
    PRIMARY KEY (collection_id, card_id)
);

-- Поиск (FTS5): наполняется вручную при сохранении карточки
CREATE VIRTUAL TABLE IF NOT EXISTS card_search USING fts5(
    title, description, tags, code, notes,
    content='', tokenize='unicode61 remove_diacritics 2'
);

-- Метаданные схемы
CREATE TABLE IF NOT EXISTS schema_meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);

-- Индексы
CREATE INDEX IF NOT EXISTS idx_card_tags_tag ON card_tags(tag_id);
CREATE INDEX IF NOT EXISTS idx_line_notes_block ON line_notes(block_id, line_from);
CREATE INDEX IF NOT EXISTS idx_collection_cards_card ON collection_cards(card_id);
"#;

/// Вторая версия схемы: словарь терминов.
///
/// Термин - короткая запись (как правило, одно слово), значение - свободный текст.
/// Отдельная таблица, а не карточка: у термина нет блоков кода и разбора,
/// а поиск по словарю показывается в интерфейсе отдельной группой.
const SCHEMA_V2: &str = r#"
CREATE TABLE IF NOT EXISTS terms (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    term        TEXT NOT NULL,
    meaning     TEXT NOT NULL DEFAULT '',
    created_at  TEXT NOT NULL,
    updated_at  TEXT NOT NULL
);

-- Регистронезависимый поиск точного термина: подсказка о совпадении
CREATE UNIQUE INDEX IF NOT EXISTS idx_terms_term ON terms(term COLLATE NOCASE);

-- Поиск по словарю (FTS5). Таблица без содержимого, как и card_search:
-- строку индекса снимаем командой 'delete' с прежними значениями колонок.
CREATE VIRTUAL TABLE IF NOT EXISTS term_search USING fts5(
    term, meaning,
    content='', tokenize='unicode61 remove_diacritics 2'
);
"#;

/// Применяет миграции к открытому соединению.
/// Возвращает версию схемы после применения.
pub fn migrate(conn: &Connection) -> Result<i32, String> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS schema_meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);",
    )
    .map_err(|e| format!("не удалось создать schema_meta: {e}"))?;

    let current_version: i32 = read_version(conn)?;

    // Шаг 1: таблицы карточек, блоков, разбора, слотов, тегов и подборок.
    if current_version < 1 {
        conn.execute_batch(SCHEMA_V1)
            .map_err(|e| format!("не удалось применить схему версии 1: {e}"))?;
        set_version(conn, 1)?;
    }

    // Шаг 2: словарь терминов. Применяется и к новой базе (она уже на
    // версии 1 после шага выше), и к базе пользователя, созданной раньше.
    let version_after_v1: i32 = read_version(conn)?;
    if version_after_v1 < 2 {
        conn.execute_batch(SCHEMA_V2)
            .map_err(|e| format!("не удалось применить схему версии 2: {e}"))?;
        // Если термины уже есть (база пришла из импорта), они попадают в индекс.
        crate::db::rebuild_all_term_fts(conn)?;
        set_version(conn, 2)?;
    }

    // Будущие миграции добавляются здесь шагами: if current_version < 3 { ... }
    read_version(conn)
}

/// Читает версию схемы из schema_meta. Нет записи - версия 0.
fn read_version(conn: &Connection) -> Result<i32, String> {
    let value: Option<String> = conn
        .query_row(
            "SELECT value FROM schema_meta WHERE key = 'version'",
            [],
            |row| row.get(0),
        )
        .ok();
    Ok(value.as_deref().and_then(|v| v.parse().ok()).unwrap_or(0))
}

/// Записывает версию схемы.
fn set_version(conn: &Connection, version: i32) -> Result<(), String> {
    conn.execute(
        "INSERT INTO schema_meta (key, value) VALUES ('version', ?1)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        [version.to_string()],
    )
    .map_err(|e| format!("не удалось записать версию схемы: {e}"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrate_creates_schema_and_is_idempotent() {
        let conn = Connection::open_in_memory().expect("память");
        // Свежая база сразу получает последнюю версию: 2 (словарь терминов)
        assert_eq!(migrate(&conn).expect("первая миграция"), SCHEMA_VERSION);
        // Повторный вызов ничего не ломает и не пересоздаёт таблицы
        assert_eq!(migrate(&conn).expect("вторая миграция"), SCHEMA_VERSION);
        assert_eq!(read_version(&conn).unwrap(), SCHEMA_VERSION);

        let tables: Vec<String> = conn
            .prepare("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")
            .unwrap()
            .query_map([], |row| row.get(0))
            .unwrap()
            .filter_map(Result::ok)
            .collect();
        for expected in [
            "cards",
            "code_blocks",
            "line_notes",
            "slots",
            "tags",
            "card_tags",
            "collections",
            "collection_cards",
        ] {
            assert!(tables.contains(&expected.to_string()), "нет таблицы {expected}");
        }
        assert!(tables.contains(&"card_search".to_string()), "нет FTS-таблицы");
        // Версия 2: словарь терминов
        assert!(tables.contains(&"terms".to_string()), "нет таблицы терминов");
        assert!(tables.contains(&"term_search".to_string()), "нет FTS словаря");
    }

    /// База пользователя, созданная на версии 1, получает словарь без потерь.
    #[test]
    fn upgrade_from_version_one_adds_dictionary() {
        let conn = Connection::open_in_memory().expect("память");
        // Состояние базы до словаря: только первая версия схемы
        conn.execute_batch("CREATE TABLE schema_meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);")
            .unwrap();
        conn.execute_batch(SCHEMA_V1).unwrap();
        set_version(&conn, 1).unwrap();
        let now = "2026-11-12T10:00:00.000";
        conn.execute(
            "INSERT INTO cards (id, title, created_at, updated_at) VALUES (1, 'Старая', ?1, ?1)",
            [now],
        )
        .unwrap();

        assert_eq!(migrate(&conn).expect("обновление"), 2);
        // Карточка на месте
        let title: String = conn
            .query_row("SELECT title FROM cards WHERE id = 1", [], |row| row.get(0))
            .unwrap();
        assert_eq!(title, "Старая");
        // Словарь готов к работе
        conn.execute(
            "INSERT INTO terms (term, meaning, created_at, updated_at) VALUES ('FTS', 'поиск', ?1, ?1)",
            [now],
        )
        .expect("вставка термина");
    }

    /// Повторный запуск не пересоздаёт таблицы и не меняет версию.
    #[test]
    fn migrate_twice_keeps_version_two() {
        let conn = Connection::open_in_memory().expect("память");
        assert_eq!(migrate(&conn).unwrap(), 2);
        let now = "2026-11-12T10:00:00.000";
        conn.execute(
            "INSERT INTO terms (term, meaning, created_at, updated_at) VALUES ('слот', 'параметр', ?1, ?1)",
            [now],
        )
        .unwrap();
        assert_eq!(migrate(&conn).unwrap(), 2);
        let count: i64 = conn
            .query_row("SELECT count(*) FROM terms", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 1, "термин не должен пропасть при повторной миграции");
    }

    #[test]
    fn fts5_table_accepts_rows() {
        let conn = Connection::open_in_memory().expect("память");
        migrate(&conn).expect("схема");
        conn.execute(
            "INSERT INTO card_search (rowid, title, description, tags, code, notes)
             VALUES (1, 'Кэш', '', 'python', 'lru_cache', '')",
            [],
        )
        .expect("вставка в FTS");
        let hits: i64 = conn
            .query_row(
                "SELECT count(*) FROM card_search WHERE card_search MATCH 'lru*'",
                [],
                |row| row.get(0),
            )
            .expect("поиск");
        assert_eq!(hits, 1);
    }
}
