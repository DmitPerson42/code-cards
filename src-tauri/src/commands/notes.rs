// Разбор по строкам: точечные команды поверх данных карточки.
//
// В основном разбор сохраняется вместе с карточкой через card_save,
// но отдельные команды нужны для быстрого наведения порядка в редакторе.

use crate::models::LineNote;
use rusqlite::{params, Connection};

/// Добавляет диапазон разбора к блоку, возвращает id новой записи.
#[allow(dead_code)]
pub fn note_create_impl(
    conn: &Connection,
    block_id: i64,
    line_from: i64,
    line_to: i64,
    text: &str,
) -> Result<i64, String> {
    let from = line_from.max(1);
    let to = line_to.max(from);
    conn.execute(
        "INSERT INTO line_notes (block_id, line_from, line_to, text, is_stale, sort_order)
         VALUES (?1, ?2, ?3, ?4, 0,
                 (SELECT coalesce(max(sort_order), -1) + 1 FROM line_notes WHERE block_id = ?1))",
        params![block_id, from, to, text],
    )
    .map_err(|e| format!("добавление разбора: {e}"))?;
    Ok(conn.last_insert_rowid())
}

/// Обновляет диапазон и текст разбора.
#[allow(dead_code)]
pub fn note_update_impl(conn: &Connection, note: &LineNote) -> Result<(), String> {
    let id = note
        .id
        .ok_or_else(|| "для обновления разбора нужен id".to_string())?;
    let from = note.line_from.max(1);
    let to = note.line_to.max(from);
    let changed = conn
        .execute(
            "UPDATE line_notes SET line_from = ?1, line_to = ?2, text = ?3,
                                   is_stale = ?4, sort_order = ?5
             WHERE id = ?6",
            params![from, to, note.text, note.is_stale as i64, note.sort_order, id],
        )
        .map_err(|e| format!("обновление разбора: {e}"))?;
    if changed == 0 {
        return Err(format!("разбор {id} не найден"));
    }
    Ok(())
}

/// Удаляет разбор.
#[allow(dead_code)]
pub fn note_delete_impl(conn: &Connection, note_id: i64) -> Result<(), String> {
    conn.execute("DELETE FROM line_notes WHERE id = ?1", [note_id])
        .map_err(|e| format!("удаление разбора: {e}"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;

    fn setup() -> Connection {
        let conn = db::open_in_memory().unwrap();
        let now = db::now_iso();
        conn.execute(
            "INSERT INTO cards (id, title, created_at, updated_at) VALUES (1, 'К', ?1, ?1)",
            [&now],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO code_blocks (id, card_id, code) VALUES (1, 1, 'a\nb\nc')",
            [],
        )
        .unwrap();
        conn
    }

    #[test]
    fn note_crud() {
        let conn = setup();
        let id = note_create_impl(&conn, 1, 2, 2, "вторая строка").unwrap();
        let note = LineNote {
            id: Some(id),
            line_from: 1,
            line_to: 3,
            text: "весь блок".into(),
            is_stale: true,
            sort_order: 0,
        };
        note_update_impl(&conn, &note).unwrap();
        let (from, to, text, stale): (i64, i64, String, i64) = conn
            .query_row(
                "SELECT line_from, line_to, text, is_stale FROM line_notes WHERE id = ?1",
                [id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .unwrap();
        assert_eq!((from, to), (1, 3));
        assert_eq!(text, "весь блок");
        assert_eq!(stale, 1);

        note_delete_impl(&conn, id).unwrap();
        let count: i64 = conn
            .query_row("SELECT count(*) FROM line_notes", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn line_to_is_clamped_to_line_from() {
        let conn = setup();
        let id = note_create_impl(&conn, 1, 5, 2, "путаный диапазон").unwrap();
        let (from, to): (i64, i64) = conn
            .query_row(
                "SELECT line_from, line_to FROM line_notes WHERE id = ?1",
                [id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(from, 5);
        assert_eq!(to, 5, "конец диапазона не может быть раньше начала");
    }
}
