// Подборки: дерево с вложенностью и ручным порядком карточек (этап 5).

use rusqlite::{params, Connection};

use crate::db;
use crate::models::CollectionNode;

/// Список подборок в виде дерева.
pub fn collection_list_impl(conn: &Connection) -> Result<Vec<CollectionNode>, String> {
    // Запрос в отдельном блоке: заимствование stmt кончается вместе с блоком,
    // иначе временное значение в цепочке query_map даёт E0597.
    let rows: Vec<(i64, String, Option<i64>, i64, i64)> = {
        let mut stmt = conn
            .prepare(
                "SELECT c.id, c.name, c.parent_id, c.sort_order,
                        (SELECT count(*) FROM collection_cards cc WHERE cc.collection_id = c.id)
                 FROM collections c
                 ORDER BY c.sort_order, c.name COLLATE NOCASE",
            )
            .map_err(|e| format!("список подборок: {e}"))?;
        let mapped = stmt
            .query_map([], |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                ))
            })
            .map_err(|e| format!("список подборок: {e}"))?;
        mapped.filter_map(Result::ok).collect()
    };

    // Строим дерево: сначала все узлы, затем раскладываем по родителям
    let mut nodes: Vec<CollectionNode> = rows
        .into_iter()
        .map(|(id, name, parent_id, sort_order, card_count)| CollectionNode {
            id,
            name,
            parent_id,
            sort_order,
            card_count,
            children: Vec::new(),
        })
        .collect();

    let mut children_by_parent: std::collections::HashMap<Option<i64>, Vec<usize>> =
        std::collections::HashMap::new();
    for (index, node) in nodes.iter().enumerate() {
        children_by_parent
            .entry(node.parent_id)
            .or_default()
            .push(index);
    }

    // Собираем снизу вверх: сначала листья, потом родители
    fn build(
        nodes: &mut Vec<CollectionNode>,
        children_by_parent: &std::collections::HashMap<Option<i64>, Vec<usize>>,
        parent: Option<i64>,
    ) -> Vec<CollectionNode> {
        let mut result = Vec::new();
        if let Some(indices) = children_by_parent.get(&parent) {
            for &index in indices {
                let mut node = nodes[index].clone();
                node.children = build(nodes, children_by_parent, Some(node.id));
                result.push(node);
            }
        }
        result
    }

    Ok(build(&mut nodes, &children_by_parent, None))
}

/// Создаёт или переименовывает подборку, возвращает id.
pub fn collection_save_impl(
    conn: &Connection,
    id: Option<i64>,
    name: &str,
    parent_id: Option<i64>,
) -> Result<i64, String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("название подборки не может быть пустым".to_string());
    }

    if let (Some(self_id), Some(parent)) = (id, parent_id) {
        if self_id == parent {
            return Err("подборку нельзя вложить в саму себя".to_string());
        }
        if is_descendant(conn, parent, self_id)? {
            return Err("подборку нельзя вложить в её собственную вложенную подборку".to_string());
        }
    }

    match id {
        Some(existing) => {
            let changed = conn
                .execute(
                    "UPDATE collections SET name = ?1, parent_id = ?2 WHERE id = ?3",
                    params![name, parent_id, existing],
                )
                .map_err(|e| format!("сохранение подборки: {e}"))?;
            if changed == 0 {
                return Err(format!("подборка {existing} не найдена"));
            }
            Ok(existing)
        }
        None => {
            let order: i64 = conn
                .query_row(
                    "SELECT coalesce(max(sort_order), -1) + 1 FROM collections
                     WHERE parent_id IS ?1",
                    [parent_id],
                    |row| row.get(0),
                )
                .unwrap_or(0);
            conn.execute(
                "INSERT INTO collections (name, parent_id, sort_order, created_at)
                 VALUES (?1, ?2, ?3, ?4)",
                params![name, parent_id, order, db::now_iso()],
            )
            .map_err(|e| format!("создание подборки: {e}"))?;
            Ok(conn.last_insert_rowid())
        }
    }
}

/// Проверяет, является ли `candidate` потомком `ancestor`.
fn is_descendant(conn: &Connection, candidate: i64, ancestor: i64) -> Result<bool, String> {
    let found: i64 = conn
        .query_row(
            "WITH RECURSIVE sub(id) AS (
                 SELECT id FROM collections WHERE parent_id = ?1
                 UNION ALL
                 SELECT c.id FROM collections c JOIN sub ON c.parent_id = sub.id
             )
             SELECT count(*) FROM sub WHERE id = ?2",
            params![ancestor, candidate],
            |row| row.get(0),
        )
        .map_err(|e| format!("проверка вложенности: {e}"))?;
    Ok(found > 0)
}

/// Удаляет подборку вместе с вложенными. Карточки остаются в библиотеке.
pub fn collection_delete_impl(conn: &Connection, id: i64) -> Result<(), String> {
    conn.execute("DELETE FROM collections WHERE id = ?1", [id])
        .map_err(|e| format!("удаление подборки: {e}"))?;
    Ok(())
}

/// Задаёт состав подборки в нужном порядке. Порядок фиксируется числами 0..n.
pub fn collection_set_cards_impl(
    conn: &Connection,
    collection_id: i64,
    card_ids: &[i64],
) -> Result<(), String> {
    let tx = conn
        .unchecked_transaction()
        .map_err(|e| format!("транзакция подборки: {e}"))?;
    tx.execute(
        "DELETE FROM collection_cards WHERE collection_id = ?1",
        [collection_id],
    )
    .map_err(|e| format!("очистка подборки: {e}"))?;
    let now = db::now_iso();
    for (index, card_id) in card_ids.iter().enumerate() {
        tx.execute(
            "INSERT OR IGNORE INTO collection_cards (collection_id, card_id, sort_order, added_at)
             VALUES (?1, ?2, ?3, ?4)",
            params![collection_id, card_id, index as i64, now],
        )
        .map_err(|e| format!("наполнение подборки: {e}"))?;
    }
    tx.commit().map_err(|e| format!("фиксация подборки: {e}"))?;
    Ok(())
}

/// Добавляет одну карточку в конец подборки.
pub fn collection_add_card_impl(
    conn: &Connection,
    collection_id: i64,
    card_id: i64,
) -> Result<(), String> {
    let order: i64 = conn
        .query_row(
            "SELECT coalesce(max(sort_order), -1) + 1 FROM collection_cards WHERE collection_id = ?1",
            [collection_id],
            |row| row.get(0),
        )
        .unwrap_or(0);
    conn.execute(
        "INSERT OR IGNORE INTO collection_cards (collection_id, card_id, sort_order, added_at)
         VALUES (?1, ?2, ?3, ?4)",
        params![collection_id, card_id, order, db::now_iso()],
    )
    .map_err(|e| format!("добавление карточки в подборку: {e}"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::Filter;

    fn card(conn: &Connection, title: &str) -> i64 {
        let now = db::now_iso();
        conn.execute(
            "INSERT INTO cards (title, created_at, updated_at) VALUES (?1, ?2, ?2)",
            params![title, now],
        )
        .unwrap();
        conn.last_insert_rowid()
    }

    #[test]
    fn nested_collections_form_a_tree() {
        let conn = db::open_in_memory().unwrap();
        let root = collection_save_impl(&conn, None, "Учёба", None).unwrap();
        let child = collection_save_impl(&conn, None, "Алгоритмы", Some(root)).unwrap();
        collection_save_impl(&conn, None, "Деревья", Some(child)).unwrap();

        let tree = collection_list_impl(&conn).unwrap();
        assert_eq!(tree.len(), 1, "корень один");
        assert_eq!(tree[0].name, "Учёба");
        assert_eq!(tree[0].children.len(), 1);
        assert_eq!(tree[0].children[0].name, "Алгоритмы");
        assert_eq!(tree[0].children[0].children[0].name, "Деревья");
    }

    #[test]
    fn order_is_kept_and_card_lives_in_two_collections() {
        let conn = db::open_in_memory().unwrap();
        let a = collection_save_impl(&conn, None, "Первая", None).unwrap();
        let b = collection_save_impl(&conn, None, "Вторая", None).unwrap();
        let c1 = card(&conn, "Один");
        let c2 = card(&conn, "Два");
        let c3 = card(&conn, "Три");

        collection_set_cards_impl(&conn, a, &[c3, c1, c2]).unwrap();
        collection_add_card_impl(&conn, b, c2).unwrap();

        let filter = Filter {
            collection_id: Some(a),
            ..Default::default()
        };
        let list = crate::commands::cards::cards_list_impl(&conn, &filter).unwrap();
        let titles: Vec<String> = list.iter().map(|c| c.title.clone()).collect();
        assert_eq!(titles, vec!["Три", "Один", "Два"], "порядок ручной");

        let filter = Filter {
            collection_id: Some(b),
            ..Default::default()
        };
        let list = crate::commands::cards::cards_list_impl(&conn, &filter).unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].id, c2, "карточка лежит в двух подборках сразу");
    }

    #[test]
    fn cycle_is_rejected() {
        let conn = db::open_in_memory().unwrap();
        let root = collection_save_impl(&conn, None, "Корень", None).unwrap();
        let child = collection_save_impl(&conn, None, "Ребёнок", Some(root)).unwrap();

        assert!(collection_save_impl(&conn, Some(root), "Корень", Some(root)).is_err());
        assert!(
            collection_save_impl(&conn, Some(root), "Корень", Some(child)).is_err(),
            "корень нельзя переложить внутрь своего ребёнка"
        );
    }

    #[test]
    fn empty_name_is_rejected() {
        let conn = db::open_in_memory().unwrap();
        assert!(collection_save_impl(&conn, None, "   ", None).is_err());
    }

    #[test]
    fn deleting_collection_keeps_cards() {
        let conn = db::open_in_memory().unwrap();
        let id = collection_save_impl(&conn, None, "Временная", None).unwrap();
        let c = card(&conn, "Живая");
        collection_add_card_impl(&conn, id, c).unwrap();
        collection_delete_impl(&conn, id).unwrap();

        let list = crate::commands::cards::cards_list_impl(&conn, &Filter::default()).unwrap();
        assert_eq!(list.len(), 1, "удаление подборки не трогает карточки");
    }
}
