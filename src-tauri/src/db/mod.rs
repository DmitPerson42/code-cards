// Работа с базой данных: открытие файла, настройки соединения, миграции.

pub mod schema;

use rusqlite::Connection;
use std::path::Path;

/// Открывает базу по указанному пути и готовит её к работе.
///
/// Режим WAL нужен, чтобы чтение не блокировалось записью, а резервная копия
/// делалась через VACUUM INTO (см. раздел 3.6 плана).
pub fn open(path: &Path) -> Result<Connection, String> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)
            .map_err(|e| format!("не удалось создать каталог данных {}: {e}", dir.display()))?;
    }
    let conn = Connection::open(path)
        .map_err(|e| format!("не удалось открыть базу {}: {e}", path.display()))?;
    configure(&conn)?;
    schema::migrate(&conn)?;
    Ok(conn)
}

/// Открывает базу в памяти: для тестов и проверок.
#[allow(dead_code)]
pub fn open_in_memory() -> Result<Connection, String> {
    let conn = Connection::open_in_memory().map_err(|e| format!("память: {e}"))?;
    configure(&conn)?;
    schema::migrate(&conn)?;
    Ok(conn)
}

fn configure(conn: &Connection) -> Result<(), String> {
    conn.execute_batch(
        "PRAGMA journal_mode = WAL;
         PRAGMA foreign_keys = ON;
         PRAGMA busy_timeout = 5000;
         PRAGMA synchronous = NORMAL;",
    )
    .map_err(|e| format!("не удалось настроить соединение: {e}"))
}

/// Пять поисковых колонок карточки в порядке схемы FTS.
pub struct FtsRow {
    pub title: String,
    pub description: String,
    pub tags: String,
    pub code: String,
    pub notes: String,
}

/// Собирает поисковую строку карточки из таблиц.
///
/// Одна функция на две операции: вставку и удаление. Для таблицы FTS5
/// без содержимого (`content=''`) удаление требует ровно те значения,
/// что были вставлены, поэтому обе операции берут одни и те же данные.
pub fn collect_fts_row(conn: &Connection, card_id: i64) -> Result<FtsRow, String> {
    let title: String = conn
        .query_row("SELECT title FROM cards WHERE id = ?1", [card_id], |row| {
            row.get(0)
        })
        .unwrap_or_default();
    let description: String = conn
        .query_row(
            "SELECT description FROM cards WHERE id = ?1",
            [card_id],
            |row| row.get(0),
        )
        .unwrap_or_default();

    // Теги через пробел
    let tags: String = {
        let mut stmt = conn
            .prepare(
                "SELECT t.name FROM tags t
                 JOIN card_tags ct ON ct.tag_id = t.id
                 WHERE ct.card_id = ?1
                 ORDER BY t.name",
            )
            .map_err(|e| format!("теги для FTS: {e}"))?;
        let names: Vec<String> = stmt
            .query_map([card_id], |row| row.get(0))
            .map_err(|e| format!("теги для FTS: {e}"))?
            .filter_map(Result::ok)
            .collect();
        names.join(" ")
    };

    // Код всех блоков и тексты разбора
    let mut code = String::new();
    let mut notes = String::new();
    {
        let mut stmt = conn
            .prepare("SELECT id, code FROM code_blocks WHERE card_id = ?1 ORDER BY sort_order, id")
            .map_err(|e| format!("блоки для FTS: {e}"))?;
        let blocks: Vec<(i64, String)> = stmt
            .query_map([card_id], |row| Ok((row.get(0)?, row.get(1)?)))
            .map_err(|e| format!("блоки для FTS: {e}"))?
            .filter_map(Result::ok)
            .collect();
        for (_, block_code) in &blocks {
            code.push_str(block_code);
            code.push('\n');
        }
        for (block_id, _) in &blocks {
            let mut note_stmt = conn
                .prepare("SELECT text FROM line_notes WHERE block_id = ?1 ORDER BY line_from")
                .map_err(|e| format!("разбор для FTS: {e}"))?;
            let texts: Vec<String> = note_stmt
                .query_map([block_id], |row| row.get(0))
                .map_err(|e| format!("разбор для FTS: {e}"))?
                .filter_map(Result::ok)
                .collect();
            for text in texts {
                notes.push_str(&text);
                notes.push('\n');
            }
        }
    }

    Ok(FtsRow {
        title,
        description,
        tags,
        code,
        notes,
    })
}

/// Пересобирает поисковую строку карточки в FTS.
///
/// FTS наполняется вручную при каждом сохранении (раздел 3.1 плана):
/// так проще, чем триггеры на составные поля.
pub fn rebuild_fts(conn: &Connection, card_id: i64) -> Result<(), String> {
    let row = collect_fts_row(conn, card_id)?;
    // Таблица без содержимого (content=''): обычный DELETE из неё недопустим,
    // FTS5 требует команду 'delete' со значениями колонок прежней строки.
    delete_fts_row(conn, card_id)?;
    conn.execute(
        "INSERT INTO card_search (rowid, title, description, tags, code, notes)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        rusqlite::params![
            card_id,
            row.title,
            row.description,
            row.tags,
            row.code,
            row.notes
        ],
    )
    .map_err(|e| format!("вставка в FTS: {e}"))?;
    Ok(())
}

/// Удаляет строку карточки из таблицы FTS5 без содержимого.
///
/// Значения колонок FTS5 вернуть не может (`content=''`), поэтому они берутся
/// заново из таблиц карточки. Команда 'delete' по несуществующей строке
/// возвращает ошибку "database disk image is malformed", поэтому наличие строки
/// в индексе проверяется заранее запросом по rowid без подсчёта токенов.
pub fn delete_fts_row(conn: &Connection, card_id: i64) -> Result<(), String> {
    let indexed: bool = conn
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM card_search WHERE rowid = ?1)",
            [card_id],
            |row| row.get(0),
        )
        .map_err(|e| format!("проверка строки FTS: {e}"))?;
    if !indexed {
        return Ok(());
    }
    let row = collect_fts_row(conn, card_id)?;
    conn.execute(
        "INSERT INTO card_search (card_search, rowid, title, description, tags, code, notes)
         VALUES ('delete', ?1, ?2, ?3, ?4, ?5, ?6)",
        rusqlite::params![
            card_id,
            row.title,
            row.description,
            row.tags,
            row.code,
            row.notes
        ],
    )
    .map_err(|e| format!("удаление из FTS: {e}"))?;
    Ok(())
}

/// Пишет строку карточки в индекс FTS по её текущему состоянию в таблицах.
///
/// Вызывать только когда строки этого card_id в индексе ещё нет: повторная
/// вставка по занятому rowid даст дубль токенов. Для полной пересборки есть
/// `rebuild_fts`, она сама снимает старую строку.
pub fn insert_fts_row(conn: &Connection, card_id: i64) -> Result<(), String> {
    let row = collect_fts_row(conn, card_id)?;
    conn.execute(
        "INSERT INTO card_search (rowid, title, description, tags, code, notes)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        rusqlite::params![
            card_id,
            row.title,
            row.description,
            row.tags,
            row.code,
            row.notes
        ],
    )
    .map_err(|e| format!("вставка в FTS: {e}"))?;
    Ok(())
}

/// Текущее время в ISO-формате: единый вид для created_at и updated_at.
pub fn now_iso() -> String {
    chrono::Local::now().format("%Y-%m-%dT%H:%M:%S%.3f").to_string()
}

// ---------------------------------------------------------------------------
// Индекс словаря терминов
// ---------------------------------------------------------------------------
//
// Устройство такое же, как у card_search: таблица FTS5 без содержимого
// (content=''), строку снимаем командой 'delete' с прежними значениями
// колонок. Поэтому и вставка, и удаление берут значения из таблицы terms.

/// Две поисковые колонки термина в порядке схемы FTS.
pub struct TermFtsRow {
    pub term: String,
    pub meaning: String,
}

/// Собирает поисковую строку термина из таблицы terms.
pub fn collect_term_fts_row(conn: &Connection, term_id: i64) -> Result<TermFtsRow, String> {
    let (term, meaning): (String, String) = conn
        .query_row(
            "SELECT term, meaning FROM terms WHERE id = ?1",
            [term_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .map_err(|e| format!("термин {term_id} для индекса: {e}"))?;
    Ok(TermFtsRow { term, meaning })
}

/// Удаляет строку термина из индекса словаря.
pub fn delete_term_fts_row(conn: &Connection, term_id: i64) -> Result<(), String> {
    let indexed: bool = conn
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM term_search WHERE rowid = ?1)",
            [term_id],
            |row| row.get(0),
        )
        .map_err(|e| format!("проверка строки словаря: {e}"))?;
    if !indexed {
        return Ok(());
    }
    let row = collect_term_fts_row(conn, term_id)?;
    conn.execute(
        "INSERT INTO term_search (term_search, rowid, term, meaning)
         VALUES ('delete', ?1, ?2, ?3)",
        rusqlite::params![term_id, row.term, row.meaning],
    )
    .map_err(|e| format!("удаление из словаря: {e}"))?;
    Ok(())
}

/// Пишет строку термина в индекс. Вызывать только когда строки ещё нет.
pub fn insert_term_fts_row(conn: &Connection, term_id: i64) -> Result<(), String> {
    let row = collect_term_fts_row(conn, term_id)?;
    conn.execute(
        "INSERT INTO term_search (rowid, term, meaning) VALUES (?1, ?2, ?3)",
        rusqlite::params![term_id, row.term, row.meaning],
    )
    .map_err(|e| format!("вставка в словарь: {e}"))?;
    Ok(())
}

/// Пересобирает строку термина в индексе словаря.
///
/// Внимание: 'rebuild' у contentless-таблицы запрещён, а 'delete' после
/// UPDATE сверяет уже другой текст и молча ничего не снимает. Правку записи
/// словаря делайте в порядке: `delete_term_fts_row`, затем UPDATE, затем
/// `insert_term_fts_row` (так и сделано в `commands::terms`).
pub fn rebuild_term_fts(conn: &Connection, term_id: i64) -> Result<(), String> {
    delete_term_fts_row(conn, term_id)?;
    insert_term_fts_row(conn, term_id)?;
    Ok(())
}

/// Полная пересборка индекса словаря: нужна после миграции и импорта.
pub fn rebuild_all_term_fts(conn: &Connection) -> Result<(), String> {
    let ids: Vec<i64> = {
        let mut stmt = conn
            .prepare("SELECT id FROM terms ORDER BY id")
            .map_err(|e| format!("термины для пересборки: {e}"))?;
        let rows: Vec<i64> = stmt
            .query_map([], |row| row.get(0))
            .map_err(|e| format!("термины для пересборки: {e}"))?
            .filter_map(Result::ok)
            .collect();
        rows
    };
    for term_id in ids {
        rebuild_term_fts(conn, term_id)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_in_memory_sets_wal_and_foreign_keys() {
        let conn = open_in_memory().expect("память");
        let fk: i64 = conn
            .query_row("PRAGMA foreign_keys", [], |row| row.get(0))
            .unwrap();
        assert_eq!(fk, 1, "внешние ключи должны быть включены");
    }

    #[test]
    fn rebuild_fts_collects_all_sources() {
        let conn = open_in_memory().expect("память");
        let now = now_iso();
        conn.execute(
            "INSERT INTO cards (id, title, description, created_at, updated_at)
             VALUES (1, 'Кэш функций', 'про lru_cache', ?1, ?1)",
            [&now],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO tags (id, name) VALUES (1, 'python')",
            [],
        )
        .unwrap();
        conn.execute("INSERT INTO card_tags (card_id, tag_id) VALUES (1, 1)", [])
            .unwrap();
        conn.execute(
            "INSERT INTO code_blocks (id, card_id, code) VALUES (1, 1, '@lru_cache(maxsize=None)')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO line_notes (block_id, line_from, line_to, text)
             VALUES (1, 1, 1, 'запоминает результат')",
            [],
        )
        .unwrap();

        rebuild_fts(&conn, 1).expect("пересборка");

        for (query, what) in [
            ("python", "тег"),
            ("lru*", "код"),
            ("запоминает", "разбор"),
            ("Кэш", "название"),
        ] {
            let hits: i64 = conn
                .query_row(
                    "SELECT count(*) FROM card_search WHERE card_search MATCH ?1",
                    [query],
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(hits, 1, "не найдено по полю: {what} (запрос {query})");
        }

        // Повторная пересборка не плодит дубли
        rebuild_fts(&conn, 1).expect("пересборка повторно");
        let total: i64 = conn
            .query_row("SELECT count(*) FROM card_search", [], |row| row.get(0))
            .unwrap();
        assert_eq!(total, 1);
    }
}
