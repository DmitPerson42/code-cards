// Команды карточек: список, чтение, сохранение, удаление, избранное.
//
// Правило плана: SQL живёт только здесь, во фронтенд уходят готовые структуры.

use crate::db;
use crate::models::{
    CardDetail, CardSummary, CodeBlock, Filter, LineNote, MovedSummary, SaveResult, Slot,
    SLOT_DOUBLE_BRACE,
};
use rusqlite::{params, Connection, OptionalExtension};

/// Сколько строк кода показываем в превью карточки в сетке.
const PREVIEW_LINES: usize = 4;

fn is_true(v: i64) -> bool {
    v != 0
}

/// Собирает краткую карточку по её id.
///
/// Используется списком и поиском: обработка строк кода, превью и тегов
/// должна быть в одном месте, иначе сниппет и превью начнут расходиться.
pub fn summary_by_id(
    conn: &Connection,
    id: i64,
    snippet: Option<String>,
) -> Result<Option<CardSummary>, String> {
    let base: Option<(String, String, i64, i64, String, String)> = conn
        .query_row(
            "SELECT title, description, favorite, is_template, created_at, updated_at
             FROM cards WHERE id = ?1",
            [id],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                ))
            },
        )
        .optional()
        .map_err(|e| format!("чтение карточки {id}: {e}"))?;

    let Some((title, description, favorite, is_template, created_at, updated_at)) = base else {
        return Ok(None);
    };
    let (language, preview) = first_block_preview(conn, id)?;
    let tags = card_tag_names(conn, id)?;
    Ok(Some(CardSummary {
        id,
        title,
        description,
        language,
        tags,
        preview,
        favorite: is_true(favorite),
        is_template: is_true(is_template),
        created_at,
        updated_at,
        snippet,
    }))
}

/// Читает список карточек по фильтру.
pub fn cards_list_impl(conn: &Connection, filter: &Filter) -> Result<Vec<CardSummary>, String> {
    let mut sql = String::from(
        "SELECT c.id, c.title, c.description, c.favorite, c.is_template, c.created_at, c.updated_at
         FROM cards c",
    );
    let mut args: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();

    // Фильтр по подборке: соединение с collection_cards и порядок оттуда
    let mut order = String::new();
    if let Some(collection_id) = filter.collection_id {
        sql.push_str(
            " JOIN collection_cards cc ON cc.card_id = c.id AND cc.collection_id = ?",
        );
        args.push(Box::new(collection_id));
        // Внутри подборки сохраняем ручной порядок
        order = " ORDER BY cc.sort_order, c.updated_at DESC".to_string();
    }

    if filter.favorite_only {
        sql.push_str(" WHERE c.favorite = 1");
    } else {
        sql.push_str(" WHERE 1 = 1");
    }

    // Теги: карточка должна содержать все указанные теги
    for tag in &filter.tags {
        sql.push_str(
            " AND EXISTS (SELECT 1 FROM card_tags ct
                          JOIN tags t ON t.id = ct.tag_id
                          WHERE ct.card_id = c.id AND t.name = ? COLLATE NOCASE)",
        );
        args.push(Box::new(tag.clone()));
    }

    if let Some(language) = &filter.language {
        sql.push_str(
            " AND EXISTS (SELECT 1 FROM code_blocks b
                          WHERE b.card_id = c.id AND b.language = ?)",
        );
        args.push(Box::new(language.clone()));
    }

    if order.is_empty() {
        // Фронтенд присылает "recent" | "alpha" | "created". Неизвестное
        // значение трактуем как "recent", чтобы список всегда приходил.
        order = match filter.sort.as_deref() {
            Some("alpha") => " ORDER BY c.title COLLATE NOCASE ASC".to_string(),
            Some("created") => " ORDER BY c.created_at DESC".to_string(),
            Some("recent") | None => " ORDER BY c.updated_at DESC".to_string(),
            _ => " ORDER BY c.updated_at DESC".to_string(),
        };
    }
    sql.push_str(&order);

    let mut stmt = conn.prepare(&sql).map_err(|e| format!("список карточек: {e}"))?;
    let params_ref: Vec<&dyn rusqlite::ToSql> = args.iter().map(|a| a.as_ref()).collect();
    let rows: Vec<(i64, String, String, i64, i64, String, String)> = stmt
        .query_map(params_ref.as_slice(), |row| {
            Ok((
                row.get(0)?,
                row.get(1)?,
                row.get(2)?,
                row.get(3)?,
                row.get(4)?,
                row.get(5)?,
                row.get(6)?,
            ))
        })
        .map_err(|e| format!("список карточек: {e}"))?
        .filter_map(Result::ok)
        .collect();

    let mut result = Vec::with_capacity(rows.len());
    for (id, title, description, favorite, is_template, created_at, updated_at) in rows {
        let (language, preview) = first_block_preview(conn, id)?;
        let tags = card_tag_names(conn, id)?;
        result.push(CardSummary {
            id,
            title,
            description,
            language,
            tags,
            preview,
            favorite: is_true(favorite),
            is_template: is_true(is_template),
            created_at,
            updated_at,
            snippet: None,
        });
    }
    Ok(result)
}

/// Язык и первые строки первого блока: для превью в сетке.
fn first_block_preview(conn: &Connection, card_id: i64) -> Result<(String, String), String> {
    let row: Option<(String, String)> = conn
        .query_row(
            "SELECT language, code FROM code_blocks WHERE card_id = ?1 ORDER BY sort_order, id LIMIT 1",
            [card_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()
        .map_err(|e| format!("блок для превью: {e}"))?;
    Ok(match row {
        Some((language, code)) => {
            let preview: Vec<&str> = code.lines().take(PREVIEW_LINES).collect();
            (language, preview.join("\n"))
        }
        None => (String::new(), String::new()),
    })
}

/// Имена тегов карточки в алфавитном порядке.
///
/// Нужны и карточке, и поиску при сборке сниппета.
pub fn card_tag_names(conn: &Connection, card_id: i64) -> Result<Vec<String>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT t.name FROM tags t
             JOIN card_tags ct ON ct.tag_id = t.id
             WHERE ct.card_id = ?1 ORDER BY t.name",
        )
        .map_err(|e| format!("теги карточки: {e}"))?;
    let names: Vec<String> = stmt
        .query_map([card_id], |row| row.get(0))
        .map_err(|e| format!("теги карточки: {e}"))?
        .filter_map(Result::ok)
        .collect();
    Ok(names)
}

/// Пересобирает связи карточки с тегами по списку имён.
///
/// Существующий тег переиспользуется (уникальность по имени без учёта регистра,
/// `COLLATE NOCASE` в схеме), новый создаётся. Пустые имена пропускаются.
/// Расширенная нормализация и остальные команды тегов появятся на этапе 3.
fn set_card_tags_tx(
    tx: &Connection,
    card_id: i64,
    tags: &[String],
) -> Result<(), String> {
    tx.execute("DELETE FROM card_tags WHERE card_id = ?1", [card_id])
        .map_err(|e| format!("сброс тегов карточки: {e}"))?;
    for raw in tags {
        let name = raw.trim();
        if name.is_empty() {
            continue;
        }
        let tag_id: Option<i64> = tx
            .query_row(
                "SELECT id FROM tags WHERE name = ?1 COLLATE NOCASE",
                [name],
                |row| row.get(0),
            )
            .optional()
            .map_err(|e| format!("поиск тега: {e}"))?;
        let tag_id = match tag_id {
            Some(id) => id,
            None => {
                tx.execute(
                    "INSERT INTO tags (name, color) VALUES (?1, '')",
                    params![name],
                )
                .map_err(|e| format!("создание тега: {e}"))?;
                tx.last_insert_rowid()
            }
        };
        tx.execute(
            "INSERT OR IGNORE INTO card_tags (card_id, tag_id) VALUES (?1, ?2)",
            params![card_id, tag_id],
        )
        .map_err(|e| format!("связь карточки и тега: {e}"))?;
    }
    Ok(())
}

/// Читает полную карточку.
pub fn card_get_impl(conn: &Connection, id: i64) -> Result<CardDetail, String> {
    let mut detail = conn
        .query_row(
            "SELECT title, description, is_template, slot_syntax, favorite,
                    created_at, updated_at, checked_version, checked_at
             FROM cards WHERE id = ?1",
            [id],
            |row| {
                Ok(CardDetail {
                    id: Some(id),
                    title: row.get(0)?,
                    description: row.get(1)?,
                    is_template: is_true(row.get::<_, i64>(2)?),
                    slot_syntax: row.get(3)?,
                    favorite: is_true(row.get::<_, i64>(4)?),
                    created_at: row.get(5)?,
                    updated_at: row.get(6)?,
                    checked_version: row.get(7)?,
                    checked_at: row.get(8)?,
                    tags: Vec::new(),
                    blocks: Vec::new(),
                })
            },
        )
        .optional()
        .map_err(|e| format!("чтение карточки: {e}"))?
        .ok_or_else(|| format!("карточка {id} не найдена"))?;

    detail.tags = card_tag_names(conn, id)?;

    let block_rows: Vec<(i64, String, String, i64)> = {
        let mut stmt = conn
            .prepare(
                "SELECT id, language, code, sort_order FROM code_blocks
                 WHERE card_id = ?1 ORDER BY sort_order, id",
            )
            .map_err(|e| format!("блоки карточки: {e}"))?;
        let rows = stmt
            .query_map([id], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
            })
            .map_err(|e| format!("блоки карточки: {e}"))?;
        rows.filter_map(Result::ok).collect()
    };

    for (block_id, language, code, sort_order) in block_rows {
        detail.blocks.push(CodeBlock {
            id: Some(block_id),
            language,
            code,
            sort_order,
            notes: block_notes(conn, block_id)?,
            slots: block_slots(conn, block_id)?,
        });
    }
    Ok(detail)
}

fn block_notes(conn: &Connection, block_id: i64) -> Result<Vec<LineNote>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT id, line_from, line_to, text, is_stale, sort_order
             FROM line_notes WHERE block_id = ?1 ORDER BY line_from, sort_order",
        )
        .map_err(|e| format!("разбор блока: {e}"))?;
    let notes = stmt
        .query_map([block_id], |row| {
            Ok(LineNote {
                id: Some(row.get(0)?),
                line_from: row.get(1)?,
                line_to: row.get(2)?,
                text: row.get(3)?,
                is_stale: is_true(row.get::<_, i64>(4)?),
                sort_order: row.get(5)?,
            })
        })
        .map_err(|e| format!("разбор блока: {e}"))?
        .filter_map(Result::ok)
        .collect();
    Ok(notes)
}

fn block_slots(conn: &Connection, block_id: i64) -> Result<Vec<Slot>, String> {
    let mut stmt = conn
        .prepare("SELECT id, name, default_value, variants FROM slots WHERE block_id = ?1 ORDER BY id")
        .map_err(|e| format!("слоты блока: {e}"))?;
    let rows: Vec<(i64, String, String, String)> = stmt
        .query_map([block_id], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
        })
        .map_err(|e| format!("слоты блока: {e}"))?
        .filter_map(Result::ok)
        .collect();
    Ok(rows
        .into_iter()
        .map(|(id, name, default_value, variants)| Slot {
            id: Some(id),
            name,
            default_value,
            variants: serde_json::from_str(&variants).unwrap_or_default(),
        })
        .collect())
}

/// Сохраняет карточку целиком в одной транзакции.
///
/// `force = false` и правка кода, которая затрагивает разбор: возвращает
/// сводку переноса и НИЧЕГО не пишет. `force = true` применяет перенос.
///
/// Перенос считается от старого кода в базе к новому коду от фронтенда,
/// а не от того, что прислал редактор: так он работает, даже если
/// фронтенд прислал диапазоны уже пересчитанными.
pub fn card_save_with_diff(
    conn: &mut Connection,
    detail: &CardDetail,
    force: bool,
) -> Result<SaveResult, String> {
    if detail.title.trim().is_empty() {
        return Err("название карточки не может быть пустым".to_string());
    }

    // Старые коды блоков и их разбор: нужны для переноса
    let old_blocks = match detail.id {
        Some(id) => load_blocks_snapshot(conn, id)?,
        None => Vec::new(),
    };

    // Сопоставляем блоки: пока по порядку следования, как их видит редактор
    let mut per_block_notes: Vec<Vec<crate::models::LineNote>> = Vec::with_capacity(detail.blocks.len());
    let mut summary = MovedSummary::default();

    for (index, block) in detail.blocks.iter().enumerate() {
        let old = old_blocks.get(index);
        match old {
            Some((_old_id, old_code, old_notes)) if *old_code != block.code && !old_notes.is_empty() => {
                let (moved, block_summary) =
                    crate::line_diff::transfer_notes(old_code, &block.code, old_notes);
                summary.moved += block_summary.moved;
                summary.changed += block_summary.changed;
                summary.lost += block_summary.lost;
                per_block_notes.push(moved.into_iter().map(|m| m.note).collect());
            }
            _ => {
                // Новый блок или код не менялся: разбор берём как есть
                per_block_notes.push(block.notes.clone());
            }
        }
    }

    // Есть изменения разбора, а пользователь ещё не подтвердил: не пишем ничего
    if !force && crate::line_diff::summary_needs_confirmation(&summary) {
        return Ok(SaveResult::NeedsConfirmation { summary });
    }

    write_card(conn, detail, &per_block_notes)?;
    let id = saved_card_id(conn, detail);
    Ok(SaveResult::Ok { id })
}

/// Снимок блоков карточки: id, код и разбор, в порядке вывода.
type BlockSnapshot = Vec<(i64, String, Vec<crate::models::LineNote>)>;

fn load_blocks_snapshot(conn: &Connection, card_id: i64) -> Result<BlockSnapshot, String> {
    let mut result = Vec::new();
    // stmt объявлен в отдельном блоке: временное значение в цепочке
    // query_map не доживает до collect и даёт E0597.
    let ids: Vec<i64> = {
        let mut stmt = conn
            .prepare("SELECT id FROM code_blocks WHERE card_id = ?1 ORDER BY sort_order, id")
            .map_err(|e| format!("блоки карточки: {e}"))?;
        let rows = stmt
            .query_map([card_id], |row| row.get(0))
            .map_err(|e| format!("блоки карточки: {e}"))?;
        rows.filter_map(Result::ok).collect()
    };
    for block_id in ids {
        let code: String = conn
            .query_row(
                "SELECT code FROM code_blocks WHERE id = ?1",
                [block_id],
                |row| row.get(0),
            )
            .map_err(|e| format!("код блока: {e}"))?;
        result.push((block_id, code, block_notes(conn, block_id)?));
    }
    Ok(result)
}

fn last_saved_id(conn: &Connection) -> Option<i64> {
    conn.query_row("SELECT max(id) FROM cards", [], |row| row.get(0)).ok()
}

/// Id карточки после сохранения: свой, либо последней записанной при создании.
pub fn saved_card_id(conn: &Connection, detail: &CardDetail) -> i64 {
    detail.id.or_else(|| last_saved_id(conn)).unwrap_or(0)
}

/// Простое сохранение без согласования переноса: разбор берётся как есть.
pub fn card_save_impl(conn: &mut Connection, detail: &CardDetail) -> Result<i64, String> {
    if detail.title.trim().is_empty() {
        return Err("название карточки не может быть пустым".to_string());
    }
    let per_block_notes: Vec<Vec<LineNote>> =
        detail.blocks.iter().map(|b| b.notes.clone()).collect();
    write_card(conn, detail, &per_block_notes)?;
    // id вернём по номеру только что записанной карточки
    if let Some(id) = detail.id {
        Ok(id)
    } else {
        Ok(last_saved_id(conn).unwrap_or(0))
    }
}

/// Сохранение внутри уже открытой транзакции.
///
/// Нужно импорту архива: весь архив должен записаться одной транзакцией,
/// а вложенную транзакцию rusqlite на том же соединении открыть нельзя.
#[allow(dead_code)]
pub fn card_save_in_tx(tx: &Connection, detail: &CardDetail) -> Result<i64, String> {
    if detail.title.trim().is_empty() {
        return Err("название карточки не может быть пустым".to_string());
    }
    let per_block_notes: Vec<Vec<LineNote>> =
        detail.blocks.iter().map(|b| b.notes.clone()).collect();
    write_card_in_tx(tx, detail, &per_block_notes)?;
    if let Some(id) = detail.id {
        Ok(id)
    } else {
        Ok(last_saved_id(tx).unwrap_or(0))
    }
}

/// Записывает карточку, блоки, разбор и слоты одной транзакцией.
fn write_card(
    conn: &mut Connection,
    detail: &CardDetail,
    notes_per_block: &[Vec<LineNote>],
) -> Result<i64, String> {
    let tx = conn
        .transaction()
        .map_err(|e| format!("не удалось начать транзакцию: {e}"))?;
    let card_id = write_card_in_tx(&tx, detail, notes_per_block)?;
    tx.commit().map_err(|e| format!("фиксация: {e}"))?;
    Ok(card_id)
}

/// Внутренняя часть записи карточки: работает и внутри чужой транзакции.
fn write_card_in_tx(
    tx: &Connection,
    detail: &CardDetail,
    notes_per_block: &[Vec<LineNote>],
) -> Result<i64, String> {
    let now = db::now_iso();
    let slot_syntax = if detail.slot_syntax.is_empty() {
        SLOT_DOUBLE_BRACE.to_string()
    } else {
        detail.slot_syntax.clone()
    };

    let card_id = match detail.id {
        Some(id) => {
            // ВАЖНО ПО ПОРЯДКУ. У таблицы FTS5 без содержимого (content='')
            // удаление сверяет значения колонок с индексом. Снимать строку
            // нужно ДО изменений карточки: UPDATE cards сменит title и
            // description, DELETE FROM code_blocks - code и notes. После
            // этого collect_fts_row соберёт уже другие значения, и FTS5
            // ответит "database disk image is malformed" (так падала правка
            // кода и названия) либо молча оставит старый текст в индексе.
            // Сюда не годится delete_fts_row_empty: 'delete' с пустыми
            // значениями не ошибку не даёт, ни строку не снимает - проверено.
            db::delete_fts_row(tx, id)?;

            let updated = tx
                .execute(
                    "UPDATE cards SET title = ?1, description = ?2, is_template = ?3,
                            slot_syntax = ?4, favorite = ?5, updated_at = ?6,
                            checked_version = ?7, checked_at = ?8
                     WHERE id = ?9",
                    params![
                        detail.title,
                        detail.description,
                        detail.is_template as i64,
                        slot_syntax,
                        detail.favorite as i64,
                        now,
                        detail.checked_version,
                        detail.checked_at,
                        id
                    ],
                )
                .map_err(|e| format!("обновление карточки: {e}"))?;
            if updated == 0 {
                return Err(format!("карточка {id} не найдена"));
            }
            id
        }
        None => {
            tx.execute(
                "INSERT INTO cards (title, description, is_template, slot_syntax, created_at,
                                    updated_at, favorite, checked_version, checked_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?5, ?6, ?7, ?8)",
                params![
                    detail.title,
                    detail.description,
                    detail.is_template as i64,
                    slot_syntax,
                    now,
                    detail.favorite as i64,
                    detail.checked_version,
                    detail.checked_at
                ],
            )
            .map_err(|e| format!("создание карточки: {e}"))?;
            tx.last_insert_rowid()
        }
    };

    // Блоки переписываются целиком: карточка приходит из редактора вся.
    tx.execute("DELETE FROM code_blocks WHERE card_id = ?1", [card_id])
        .map_err(|e| format!("очистка блоков: {e}"))?;

    for (index, block) in detail.blocks.iter().enumerate() {
        tx.execute(
            "INSERT INTO code_blocks (card_id, language, code, sort_order)
             VALUES (?1, ?2, ?3, ?4)",
            params![card_id, block.language, block.code, index as i64],
        )
        .map_err(|e| format!("сохранение блока: {e}"))?;
        let block_id = tx.last_insert_rowid();

        // Разбор берём из перенесённого набора: он уже согласован со старым кодом
        let block_notes = notes_per_block.get(index).unwrap_or(&block.notes);
        for (note_index, note) in block_notes.iter().enumerate() {
            let line_from = note.line_from.max(1);
            let line_to = note.line_to.max(line_from);
            tx.execute(
                "INSERT INTO line_notes (block_id, line_from, line_to, text, is_stale, sort_order)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    block_id,
                    line_from,
                    line_to,
                    note.text,
                    note.is_stale as i64,
                    if note.sort_order == 0 {
                        note_index as i64
                    } else {
                        note.sort_order
                    }
                ],
            )
            .map_err(|e| format!("сохранение разбора: {e}"))?;
        }

        for slot in &block.slots {
            let variants = serde_json::to_string(&slot.variants).unwrap_or_else(|_| "[]".to_string());
            tx.execute(
                "INSERT INTO slots (block_id, name, default_value, variants)
                 VALUES (?1, ?2, ?3, ?4)",
                params![block_id, slot.name, slot.default_value, variants],
            )
            .map_err(|e| format!("сохранение слота: {e}"))?;
        }
    }

    // Строка индекса уже снята: для правки - в начале ветки Some(id),
    // для новой карточки её и не было. Осталось записать актуальную строку.
    // Теги приходят именами, поэтому пересобираем связи здесь же.
    set_card_tags_tx(&tx, card_id, &detail.tags)?;

    // Индекс пуст: пишем актуальную строку заново.
    db::insert_fts_row(&tx, card_id)?;

    Ok(card_id)
}

/// Удаляет карточку. Разбор, блоки и слоты уходят по каскаду.
pub fn card_delete_impl(conn: &Connection, id: i64) -> Result<(), String> {
    // FTS хранит строку по rowid, каскад её не трогает. Для таблицы без
    // содержимого удаление требует прежних значений колонок, поэтому строку
    // индекса снимаем до удаления самой карточки.
    db::delete_fts_row(conn, id)?;
    conn.execute("DELETE FROM cards WHERE id = ?1", [id])
        .map_err(|e| format!("удаление карточки: {e}"))?;
    Ok(())
}

/// Переключает избранное и возвращает новое значение.
pub fn card_toggle_favorite_impl(conn: &Connection, id: i64) -> Result<bool, String> {
    conn.execute(
        "UPDATE cards SET favorite = 1 - favorite WHERE id = ?1",
        [id],
    )
    .map_err(|e| format!("переключение избранного: {e}"))?;
    let value: i64 = conn
        .query_row("SELECT favorite FROM cards WHERE id = ?1", [id], |row| {
            row.get(0)
        })
        .map_err(|e| format!("чтение избранного: {e}"))?;
    Ok(is_true(value))
}

/// Тип результата сохранения для регистрации в Tauri.
#[allow(dead_code)]
pub type SaveOutcome = SaveResult;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::search::search_impl;

    fn sample_card(title: &str) -> CardDetail {
        CardDetail {
            id: None,
            title: title.into(),
            description: "описание".into(),
            is_template: false,
            slot_syntax: SLOT_DOUBLE_BRACE.into(),
            favorite: false,
            created_at: String::new(),
            updated_at: String::new(),
            checked_version: String::new(),
            checked_at: String::new(),
            tags: vec!["python".into()],
            blocks: vec![CodeBlock {
                id: None,
                language: "python".into(),
                code: "def f():\n    return 1".into(),
                sort_order: 0,
                notes: vec![LineNote {
                    id: None,
                    line_from: 1,
                    line_to: 2,
                    text: "функция".into(),
                    is_stale: false,
                    sort_order: 0,
                }],
                slots: vec![],
            }],
        }
    }

    #[test]
    fn create_read_update_delete_roundtrip() {
        let mut conn = db::open_in_memory().unwrap();
        let id = card_save_impl(&mut conn, &sample_card("Первая")).unwrap();
        assert!(id > 0);

        let loaded = card_get_impl(&conn, id).unwrap();
        assert_eq!(loaded.title, "Первая");
        assert_eq!(loaded.blocks.len(), 1);
        assert_eq!(loaded.blocks[0].notes.len(), 1);
        assert_eq!(loaded.blocks[0].notes[0].text, "функция");
        assert_eq!(loaded.tags, vec!["python".to_string()]);

        let mut edited = loaded.clone();
        edited.title = "Вторая".into();
        let id2 = card_save_impl(&mut conn, &edited).unwrap();
        assert_eq!(id, id2, "правка не должна создавать новую карточку");
        assert_eq!(card_get_impl(&conn, id).unwrap().title, "Вторая");

        // В списке одна карточка с превью кода
        let list = cards_list_impl(&conn, &Filter::default()).unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].language, "python");
        assert!(list[0].preview.contains("def f"));

        card_delete_impl(&conn, id).unwrap();
        assert!(card_get_impl(&conn, id).is_err());
        assert!(cards_list_impl(&conn, &Filter::default()).unwrap().is_empty());
    }

    #[test]
    fn favorite_toggles_both_ways() {
        let mut conn = db::open_in_memory().unwrap();
        let id = card_save_impl(&mut conn, &sample_card("Избранная")).unwrap();
        assert!(card_toggle_favorite_impl(&conn, id).unwrap());
        assert!(card_get_impl(&conn, id).unwrap().favorite);
        assert!(!card_toggle_favorite_impl(&conn, id).unwrap());
        assert!(!card_get_impl(&conn, id).unwrap().favorite);
    }

    #[test]
    fn empty_title_is_rejected() {
        let mut conn = db::open_in_memory().unwrap();
        let mut card = sample_card("   ");
        assert!(card_save_impl(&mut conn, &card).is_err());
        card.title = "Нормальное".into();
        assert!(card_save_impl(&mut conn, &card).is_ok());
    }

    #[test]
    fn list_filters_by_tag_and_favorite() {
        let mut conn = db::open_in_memory().unwrap();
        let a = card_save_impl(&mut conn, &sample_card("А")).unwrap();
        let mut b = sample_card("Б");
        b.tags = vec!["sql".into()];
        card_save_impl(&mut conn, &b).unwrap();
        card_toggle_favorite_impl(&conn, a).unwrap();

        let filter = Filter {
            favorite_only: true,
            ..Default::default()
        };
        let list = cards_list_impl(&conn, &filter).unwrap();
        assert_eq!(list.len(), 1, "фильтр избранного");
        assert_eq!(list[0].title, "А");

        let filter = Filter {
            tags: vec!["SQL".into()],
            ..Default::default()
        };
        let list = cards_list_impl(&conn, &filter).unwrap();
        assert_eq!(list.len(), 1, "фильтр по тегу без учёта регистра");
        assert_eq!(list[0].title, "Б");
    }

    /// Требование раздела 3.5: сводка обязательна, а без force в базу
    /// не пишется ничего.
    #[test]
    fn save_with_changed_code_asks_confirmation_and_writes_nothing() {
        let mut conn = db::open_in_memory().unwrap();
        let id = card_save_impl(&mut conn, &sample_card("Правка")).unwrap();
        let original = card_get_impl(&conn, id).unwrap();

        // Пользователь удалил первую строку, а разбор висел на диапазоне 1-2
        let mut edited = original.clone();
        edited.blocks[0].code = "    return 1".into();
        edited.blocks[0].notes = original.blocks[0].notes.clone();

        match card_save_with_diff(&mut conn, &edited, false).unwrap() {
            SaveResult::NeedsConfirmation { summary } => {
                assert!(
                    summary.moved + summary.changed + summary.lost >= 1,
                    "сводка не должна быть пустой"
                );
            }
            SaveResult::Ok { .. } => panic!("ожидалось подтверждение переноса"),
        }

        // Без force ничего не записано: в базе прежний код
        let after = card_get_impl(&conn, id).unwrap();
        assert_eq!(
            after.blocks[0].code, original.blocks[0].code,
            "без force писать нельзя"
        );

        // С force сохраняем: разбор переносится и остаётся живым
        match card_save_with_diff(&mut conn, &edited, true).unwrap() {
            SaveResult::Ok { id: saved } => assert_eq!(saved, id),
            SaveResult::NeedsConfirmation { .. } => panic!("с force должно сохраняться"),
        }
        let final_state = card_get_impl(&conn, id).unwrap();
        assert_eq!(final_state.blocks[0].code, "    return 1");
        assert_eq!(final_state.blocks[0].notes.len(), 1, "разбор не потерян");
    }

    #[test]
    fn save_without_code_change_never_asks_confirmation() {
        let mut conn = db::open_in_memory().unwrap();
        let id = card_save_impl(&mut conn, &sample_card("Без правок")).unwrap();
        let mut edited = card_get_impl(&conn, id).unwrap();
        edited.title = "Только название".into();
        match card_save_with_diff(&mut conn, &edited, false).unwrap() {
            SaveResult::Ok { id: saved } => assert_eq!(saved, id),
            SaveResult::NeedsConfirmation { .. } => {
                panic!("правка названия не должна требовать подтверждения")
            }
        }
    }

    /// Регрессия на реальную ошибку пользователя: добавление тега к уже
    /// сохранённой карточке падало с "database disk image is malformed".
    ///
    /// Причина была в порядке операций: теги менялись до снятия строки из
    /// индекса FTS5, а у таблицы без содержимого удаление требует ровно те
    /// значения колонок, что лежат в индексе. Чиним порядком, тест держит его.
    #[test]
    fn adding_tag_to_existing_card_does_not_break_fts() {
        let mut conn = db::open_in_memory().unwrap();
        let id = card_save_impl(&mut conn, &sample_card("Карточка с тегом")).unwrap();

        let mut edited = card_get_impl(&conn, id).unwrap();
        // Пользователь добавил русский тег
        edited.tags.push("Обучение".to_string());
        assert!(edited.tags.contains(&"python".to_string()));

        let saved = card_save_impl(&mut conn, &edited)
            .expect("сохранение с новым тегом не должно падать");
        assert_eq!(saved, id);

        let check = card_get_impl(&conn, id).unwrap();
        assert!(check.tags.contains(&"Обучение".to_string()), "тег записан");
        assert!(check.tags.contains(&"python".to_string()), "старый тег сохранён");

        // Индекс в согласованном состоянии: находит и по коду, и по новому тегу
        let by_code = search_impl(&conn, "def", &Filter::default()).unwrap();
        assert_eq!(by_code.len(), 1, "поиск по коду после сохранения работает");
        let by_tag = search_impl(&conn, "Обучение", &Filter::default()).unwrap();
        assert_eq!(by_tag.len(), 1, "поиск по новому тегу работает");

        // Файл индекса не испорчен: целостность базы проходит
        let integrity: String = conn
            .query_row("PRAGMA integrity_check", [], |row| row.get(0))
            .unwrap();
        assert_eq!(integrity, "ok", "база не должна считаться повреждённой");
    }

    /// Регрессия: правка существующей карточки, где меняются сами поисковые
    /// колонки - название, описание, код и разбор.
    ///
    /// Тесты по тегам этого не ловили: тег меняет только строку `tags`, а
    /// `title`, `description`, `code` и `notes` оставались прежними. Стоило
    /// поправить код или название - и `delete` из индекса собирал уже новые
    /// значения, FTS5 не находил такой строки и падал с "database disk image
    /// is malformed". Поэтому здесь меняется всё сразу.
    #[test]
    fn editing_title_code_and_notes_does_not_break_fts() {
        let mut conn = db::open_in_memory().unwrap();
        let id = card_save_impl(&mut conn, &sample_card("Старое название")).unwrap();
        assert_eq!(
            search_impl(&conn, "Старое", &Filter::default()).unwrap().len(),
            1,
            "до правки карточка находится по старому названию"
        );

        // Пользователь правит название, описание, код, разбор и добавляет блок
        let mut edited = card_get_impl(&conn, id).unwrap();
        edited.title = "Новое название".into();
        edited.description = "другое описание про словарь".into();
        edited.blocks[0].code = "def g():\n    return 2".into();
        edited.blocks[0].notes = vec![LineNote {
            id: None,
            line_from: 1,
            line_to: 2,
            text: "вторая функция".into(),
            is_stale: false,
            sort_order: 0,
        }];
        edited.blocks.push(CodeBlock {
            id: None,
            language: "sql".into(),
            code: "SELECT * FROM terms;".into(),
            sort_order: 1,
            notes: vec![],
            slots: vec![],
        });

        let saved = card_save_impl(&mut conn, &edited).expect("правка кода не должна падать");
        assert_eq!(saved, id, "карточка правится, а не создаётся заново");

        // Новые значения находятся, старых в индексе не осталось
        for (query, why) in [
            ("Новое", "новое название"),
            ("словарь", "новое описание"),
            ("terms", "код нового блока"),
            ("вторая", "новый текст разбора"),
        ] {
            let hits = search_impl(&conn, query, &Filter::default()).unwrap();
            assert_eq!(hits.len(), 1, "не найдено: {why} (запрос {query})");
        }
        for (query, why) in [
            ("Старое", "прежнее название"),
            ("return 1", "прежний код"),
            ("return 2", "код нового блока тоже не должен теряться"),
        ] {
            let hits = search_impl(&conn, query, &Filter::default()).unwrap();
            let expect_hit = query == "return 2";
            assert_eq!(
                hits.is_empty(),
                !expect_hit,
                "неверный результат: {why} (запрос {query})"
            );
        }

        // Индекс по-прежнему в одном экземпляре на карточку
        let rows: i64 = conn
            .query_row("SELECT count(*) FROM card_search", [], |row| row.get(0))
            .unwrap();
        assert_eq!(rows, 1, "на карточку одна строка индекса");

        let integrity: String = conn
            .query_row("PRAGMA integrity_check", [], |row| row.get(0))
            .unwrap();
        assert_eq!(integrity, "ok", "база не должна считаться повреждённой");
    }

    /// То же для нескольких сохранений подряд: индекс не должен разъезжаться.
    #[test]
    fn repeated_saves_keep_fts_consistent() {        let mut conn = db::open_in_memory().unwrap();
        let id = card_save_impl(&mut conn, &sample_card("Многократная правка")).unwrap();
        for tag in ["один", "два", "три"] {
            let mut edited = card_get_impl(&conn, id).unwrap();
            edited.tags = vec![tag.to_string()];
            card_save_impl(&mut conn, &edited).expect("сохранение");
        }
        let final_card = card_get_impl(&conn, id).unwrap();
        assert_eq!(final_card.tags, vec!["три".to_string()]);
        let by_tag = search_impl(&conn, "три", &Filter::default()).unwrap();
        assert_eq!(by_tag.len(), 1);
        // Старый тег из индекса ушёл
        let by_old = search_impl(&conn, "один", &Filter::default()).unwrap();
        assert!(by_old.is_empty(), "удалённый тег не должен находиться поиском");
    }
}
