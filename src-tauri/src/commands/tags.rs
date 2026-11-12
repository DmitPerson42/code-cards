// Теги: список со счётчиками и привязка набора тегов к карточке.
//
// Теги свободные, отдельной сущности "категория" нет. Дедупликация
// регистронезависимая благодаря COLLATE NOCASE на tags.name.

use crate::models::TagWithCount;
use rusqlite::{params, Connection, Transaction};

/// Приводит ввод пользователя к нормальному виду:
/// обрезает пробелы, схлопывает внутренние, выбрасывает пустые.
pub fn normalize_tag(raw: &str) -> Option<String> {
    let collapsed = raw.split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.is_empty() {
        None
    } else {
        Some(collapsed)
    }
}

/// Список тегов с числом карточек. Теги без карточек тоже показываем:
/// пользователь мог снять тег и захочет вернуть его.
pub fn tag_list_impl(conn: &Connection) -> Result<Vec<TagWithCount>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT t.id, t.name, t.color,
                    (SELECT count(*) FROM card_tags ct WHERE ct.tag_id = t.id) AS cnt
             FROM tags t
             ORDER BY t.name COLLATE NOCASE",
        )
        .map_err(|e| format!("список тегов: {e}"))?;
    let tags = stmt
        .query_map([], |row| {
            Ok(TagWithCount {
                id: row.get(0)?,
                name: row.get(1)?,
                color: row.get(2)?,
                count: row.get(3)?,
            })
        })
        .map_err(|e| format!("список тегов: {e}"))?
        .filter_map(Result::ok)
        .collect();
    Ok(tags)
}

/// Возвращает id тега по имени, создавая его при необходимости.
fn ensure_tag(tx: &Transaction, name: &str) -> Result<i64, String> {
    if let Some(id) = tx
        .query_row(
            "SELECT id FROM tags WHERE name = ?1 COLLATE NOCASE",
            [name],
            |row| row.get::<_, i64>(0),
        )
        .ok()
    {
        return Ok(id);
    }
    tx.execute("INSERT INTO tags (name) VALUES (?1)", [name])
        .map_err(|e| format!("создание тега: {e}"))?;
    Ok(tx.last_insert_rowid())
}

/// Полностью заменяет набор тегов карточки.
pub fn set_card_tags_tx(tx: &Transaction, card_id: i64, tags: &[String]) -> Result<(), String> {
    // Нормализуем и убираем дубли без учёта регистра, сохраняя порядок ввода
    let mut cleaned: Vec<String> = Vec::new();
    for raw in tags {
        if let Some(name) = normalize_tag(raw) {
            if !cleaned
                .iter()
                .any(|existing| existing.eq_ignore_ascii_case(&name))
            {
                cleaned.push(name);
            }
        }
    }

    tx.execute("DELETE FROM card_tags WHERE card_id = ?1", [card_id])
        .map_err(|e| format!("очистка тегов карточки: {e}"))?;

    for name in &cleaned {
        let tag_id = ensure_tag(tx, name)?;
        tx.execute(
            "INSERT OR IGNORE INTO card_tags (card_id, tag_id) VALUES (?1, ?2)",
            params![card_id, tag_id],
        )
        .map_err(|e| format!("привязка тега: {e}"))?;
    }

    // Теги, на которых больше нет карточек, удаляем: не даём копиться мусору
    tx.execute(
        "DELETE FROM tags WHERE id NOT IN (SELECT tag_id FROM card_tags)",
        [],
    )
    .map_err(|e| format!("очистка осиротевших тегов: {e}"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;

    #[test]
    fn normalize_tag_collapses_spaces() {
        assert_eq!(normalize_tag("  python  "), Some("python".into()));
        assert_eq!(normalize_tag("data   science"), Some("data science".into()));
        assert_eq!(normalize_tag("   "), None);
    }

    fn insert_card(tx: &Transaction, title: &str) -> i64 {
        let now = db::now_iso();
        tx.execute(
            "INSERT INTO cards (title, created_at, updated_at) VALUES (?1, ?2, ?2)",
            params![title, now],
        )
        .unwrap();
        tx.last_insert_rowid()
    }

    #[test]
    fn same_tag_in_any_case_is_one_row() {
        let mut conn = db::open_in_memory().unwrap();
        let tx = conn.transaction().unwrap();
        let a = insert_card(&tx, "А");
        set_card_tags_tx(&tx, a, &["js".into()]).unwrap();
        let b = insert_card(&tx, "Б");
        set_card_tags_tx(&tx, b, &["JS".into()]).unwrap();
        tx.commit().unwrap();

        let tags = tag_list_impl(&conn).unwrap();
        assert_eq!(tags.len(), 1, "js и JS должны быть одним тегом");
        assert_eq!(tags[0].count, 2, "тег используется двумя карточками");
    }

    #[test]
    fn dropping_tag_from_all_cards_removes_it() {
        let mut conn = db::open_in_memory().unwrap();
        let tx = conn.transaction().unwrap();
        let a = insert_card(&tx, "А");
        set_card_tags_tx(&tx, a, &["temp".into()]).unwrap();
        tx.commit().unwrap();
        assert_eq!(tag_list_impl(&conn).unwrap().len(), 1);

        let tx = conn.transaction().unwrap();
        set_card_tags_tx(&tx, a, &[]).unwrap();
        tx.commit().unwrap();
        assert!(tag_list_impl(&conn).unwrap().is_empty());
    }
}
