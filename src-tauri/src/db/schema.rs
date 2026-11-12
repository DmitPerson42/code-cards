// Схема базы данных. SQL взят из раздела 3.1 плана разработки.
//
// Версия схемы хранится в таблице schema_meta. Миграции только вперёд:
// каждый шаг применяется один раз и переводит версию на единицу выше.

use rusqlite::Connection;

/// Текущая версия схемы, которую понимает этот код.
pub const SCHEMA_VERSION: i32 = 1;

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

/// Применяет миграции к открытому соединению.
/// Возвращает версию схемы после применения.
pub fn migrate(conn: &Connection) -> Result<i32, String> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS schema_meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);",
    )
    .map_err(|e| format!("не удалось создать schema_meta: {e}"))?;

    let current: Option<String> = conn
        .query_row(
            "SELECT value FROM schema_meta WHERE key = 'version'",
            [],
            |row| row.get(0),
        )
        .ok();
    let current_version: i32 = current
        .as_deref()
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);

    if current_version == 0 {
        conn.execute_batch(SCHEMA_V1)
            .map_err(|e| format!("не удалось применить схему версии 1: {e}"))?;
        conn.execute(
            "INSERT INTO schema_meta (key, value) VALUES ('version', ?1)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            [SCHEMA_VERSION.to_string()],
        )
        .map_err(|e| format!("не удалось записать версию схемы: {e}"))?;
        return Ok(SCHEMA_VERSION);
    }

    // Будущие миграции добавляются здесь шагами: if current_version < 2 { ... }
    Ok(current_version)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrate_creates_schema_and_is_idempotent() {
        let conn = Connection::open_in_memory().expect("память");
        assert_eq!(migrate(&conn).expect("первая миграция"), 1);
        // Повторный вызов ничего не ломает и не пересоздаёт таблицы
        assert_eq!(migrate(&conn).expect("вторая миграция"), 1);

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
