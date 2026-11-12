// Резервные копии и обмен данными (этап 7, раздел 3.6 плана).
//
// Копия базы делается через VACUUM INTO, а не копированием файла:
// в режиме WAL копирование даёт неполную базу.

use std::path::{Path, PathBuf};

use rusqlite::{Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::models::{ImportReport, Stats};

/// Сколько копий базы храним.
const BACKUPS_TO_KEEP: usize = 10;

/// Создаёт резервную копию базы в каталоге backups.
/// Возвращает путь к файлу копии.
pub fn run_backup(conn: &Connection, data_dir: &Path) -> Result<PathBuf, String> {
    let backups = data_dir.join("backups");
    std::fs::create_dir_all(&backups)
        .map_err(|e| format!("не удалось создать каталог копий {}: {e}", backups.display()))?;

    let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S").to_string();
    let target = backups.join(format!("app-{stamp}.db"));
    // Путь подставляется в SQL, поэтому кавычки удваиваем
    let escaped = target.to_string_lossy().replace('\'', "''");
    conn.execute_batch(&format!("VACUUM INTO '{escaped}'"))
        .map_err(|e| format!("не удалось сделать копию базы: {e}"))?;

    rotate_backups(&backups)?;
    Ok(target)
}

/// Удаляет старые копии, оставляя BACKUPS_TO_KEEP последних.
fn rotate_backups(backups: &Path) -> Result<(), String> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(backups)
        .map_err(|e| format!("чтение каталога копий: {e}"))?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with("app-") && n.ends_with(".db"))
        })
        .collect();
    // Имена содержат дату, поэтому сортировка по имени совпадает с порядком создания
    files.sort();
    if files.len() > BACKUPS_TO_KEEP {
        let excess = files.len() - BACKUPS_TO_KEEP;
        for path in files.into_iter().take(excess) {
            std::fs::remove_file(&path)
                .map_err(|e| format!("удаление старой копии {}: {e}", path.display()))?;
        }
    }
    Ok(())
}

/// Нужна ли копия: прошло больше суток с последней.
pub fn backup_due(data_dir: &Path) -> bool {
    let backups = data_dir.join("backups");
    let newest = match std::fs::read_dir(&backups) {
        Ok(entries) => entries
            .filter_map(Result::ok)
            .filter_map(|entry| entry.metadata().ok()?.modified().ok())
            .max(),
        Err(_) => return true,
    };
    match newest {
        Some(modified) => {
            let now = std::time::SystemTime::now();
            match now.duration_since(modified) {
                Ok(age) => age.as_secs() >= 24 * 60 * 60,
                Err(_) => true,
            }
        }
        None => true,
    }
}

/// Архив для экспорта: читаемый JSON со всем содержимым.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Archive {
    /// Версия формата архива.
    pub format_version: i32,
    #[serde(default)]
    pub cards: Vec<ArchiveCard>,
    #[serde(default)]
    pub tags: Vec<ArchiveTag>,
    #[serde(default)]
    pub collections: Vec<ArchiveCollection>,
    /// Словарь терминов. В архивах первой версии поля нет: default = пусто.
    #[serde(default)]
    pub terms: Vec<ArchiveTerm>,
}

/// Термин в архиве: те же поля, что видит пользователь.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArchiveTerm {
    pub term: String,
    #[serde(default)]
    pub meaning: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArchiveTag {
    pub name: String,
    #[serde(default)]
    pub color: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArchiveCollection {
    pub name: String,
    /// Имя родительской подборки, если она вложенная.
    #[serde(default)]
    pub parent_name: Option<String>,
    /// Названия карточек в нужном порядке.
    #[serde(default)]
    pub card_titles: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArchiveCard {
    pub title: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub is_template: bool,
    #[serde(default = "default_syntax")]
    pub slot_syntax: String,
    #[serde(default)]
    pub favorite: bool,
    #[serde(default)]
    pub created_at: String,
    #[serde(default)]
    pub updated_at: String,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub blocks: Vec<ArchiveBlock>,
}

fn default_syntax() -> String {
    crate::models::SLOT_DOUBLE_BRACE.to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArchiveBlock {
    pub language: String,
    pub code: String,
    #[serde(default)]
    pub notes: Vec<ArchiveNote>,
    #[serde(default)]
    pub slots: Vec<ArchiveSlot>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArchiveNote {
    pub line_from: i64,
    pub line_to: i64,
    pub text: String,
    #[serde(default)]
    pub is_stale: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArchiveSlot {
    pub name: String,
    #[serde(default)]
    pub default_value: String,
    #[serde(default)]
    pub variants: Vec<String>,
}

/// Собирает архив из базы.
pub fn build_archive(conn: &Connection) -> Result<Archive, String> {
    let card_ids: Vec<i64> = {
        let mut stmt = conn
            .prepare("SELECT id FROM cards ORDER BY id")
            .map_err(|e| format!("карточки для архива: {e}"))?;
        let ids: Vec<i64> = stmt
            .query_map([], |row| row.get(0))
            .map_err(|e| format!("карточки для архива: {e}"))?
            .filter_map(Result::ok)
            .collect();
        ids
    };

    let mut cards = Vec::new();
    for card_id in card_ids {
        let detail = crate::commands::cards::card_get_impl(conn, card_id)?;
        cards.push(ArchiveCard {
            title: detail.title,
            description: detail.description,
            is_template: detail.is_template,
            slot_syntax: detail.slot_syntax,
            favorite: detail.favorite,
            created_at: detail.created_at,
            updated_at: detail.updated_at,
            tags: detail.tags,
            blocks: detail
                .blocks
                .into_iter()
                .map(|block| ArchiveBlock {
                    language: block.language,
                    code: block.code,
                    notes: block
                        .notes
                        .into_iter()
                        .map(|note| ArchiveNote {
                            line_from: note.line_from,
                            line_to: note.line_to,
                            text: note.text,
                            is_stale: note.is_stale,
                        })
                        .collect(),
                    slots: block
                        .slots
                        .into_iter()
                        .map(|slot| ArchiveSlot {
                            name: slot.name,
                            default_value: slot.default_value,
                            variants: slot.variants,
                        })
                        .collect(),
                })
                .collect(),
        });
    }

    let tags = crate::commands::tags::tag_list_impl(conn)?
        .into_iter()
        .map(|tag| ArchiveTag {
            name: tag.name,
            color: tag.color,
        })
        .collect();

    // Подборки: плоским списком с именами родителей и карточками
    let collections = {
        let mut stmt = conn
            .prepare(
                "SELECT c.id, c.name, p.name FROM collections c
                 LEFT JOIN collections p ON p.id = c.parent_id
                 ORDER BY c.sort_order, c.name",
            )
            .map_err(|e| format!("подборки для архива: {e}"))?;
        let rows: Vec<(i64, String, Option<String>)> = stmt
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
            .map_err(|e| format!("подборки для архива: {e}"))?
            .filter_map(Result::ok)
            .collect();
        let mut result = Vec::new();
        for (collection_id, name, parent_name) in rows {
            let mut titles_stmt = conn
                .prepare(
                    "SELECT c.title FROM collection_cards cc
                     JOIN cards c ON c.id = cc.card_id
                     WHERE cc.collection_id = ?1 ORDER BY cc.sort_order",
                )
                .map_err(|e| format!("карточки подборки для архива: {e}"))?;
            let card_titles: Vec<String> = titles_stmt
                .query_map([collection_id], |row| row.get(0))
                .map_err(|e| format!("карточки подборки для архива: {e}"))?
                .filter_map(Result::ok)
                .collect();
            result.push(ArchiveCollection {
                name,
                parent_name,
                card_titles,
            });
        }
        result
    };

    Ok(Archive {
        format_version: 1,
        cards,
        tags,
        collections,
        terms: build_archive_terms(conn)?,
    })
}

/// Термины словаря в алфавитном порядке для архива.
fn build_archive_terms(conn: &Connection) -> Result<Vec<ArchiveTerm>, String> {
    let mut stmt = conn
        .prepare("SELECT term, meaning FROM terms ORDER BY term COLLATE NOCASE")
        .map_err(|e| format!("термины для архива: {e}"))?;
    let terms: Vec<ArchiveTerm> = stmt
        .query_map([], |row| {
            Ok(ArchiveTerm {
                term: row.get(0)?,
                meaning: row.get(1)?,
            })
        })
        .map_err(|e| format!("термины для архива: {e}"))?
        .filter_map(Result::ok)
        .collect();
    Ok(terms)
}

/// Экспортирует архив в файл JSON. Возвращает статистику.
pub fn export_json(conn: &Connection, path: &Path) -> Result<Stats, String> {
    let archive = build_archive(conn)?;
    let stats = Stats {
        cards: archive.cards.len() as i64,
        blocks: archive.cards.iter().map(|c| c.blocks.len()).sum::<usize>() as i64,
        notes: archive
            .cards
            .iter()
            .flat_map(|c| &c.blocks)
            .map(|b| b.notes.len())
            .sum::<usize>() as i64,
        slots: archive
            .cards
            .iter()
            .flat_map(|c| &c.blocks)
            .map(|b| b.slots.len())
            .sum::<usize>() as i64,
        tags: archive.tags.len() as i64,
        collections: archive.collections.len() as i64,
        terms: archive.terms.len() as i64,
    };
    let text = serde_json::to_string_pretty(&archive)
        .map_err(|e| format!("не удалось собрать JSON: {e}"))?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).ok();
    }
    std::fs::write(path, text)
        .map_err(|e| format!("не удалось записать {}: {e}", path.display()))?;
    Ok(stats)
}

/// Импортирует архив. При совпадении карточек создаёт новые, а не перезаписывает.
pub fn import_json(conn: &mut Connection, path: &Path) -> Result<ImportReport, String> {
    let text = std::fs::read_to_string(path)
        .map_err(|e| format!("не удалось прочитать {}: {e}", path.display()))?;
    let archive: Archive =
        serde_json::from_str(&text).map_err(|e| format!("файл не похож на архив Code Cards: {e}"))?;

    let mut report = ImportReport::default();
    let tx = conn
        .transaction()
        .map_err(|e| format!("транзакция импорта: {e}"))?;

    for tag in &archive.tags {
        // Теги переиспользуются по имени: дубли не нужны
        let exists: i64 = tx
            .query_row(
                "SELECT count(*) FROM tags WHERE name = ?1 COLLATE NOCASE",
                [&tag.name],
                |row| row.get(0),
            )
            .unwrap_or(0);
        if exists == 0 {
            tx.execute(
                "INSERT INTO tags (name, color) VALUES (?1, ?2)",
                rusqlite::params![tag.name, tag.color],
            )
            .map_err(|e| format!("импорт тега: {e}"))?;
            report.tags += 1;
        }
    }

    for card in &archive.cards {
        let detail = crate::models::CardDetail {
            id: None,
            title: card.title.clone(),
            description: card.description.clone(),
            is_template: card.is_template,
            slot_syntax: card.slot_syntax.clone(),
            favorite: card.favorite,
            created_at: card.created_at.clone(),
            updated_at: card.updated_at.clone(),
            checked_version: String::new(),
            checked_at: String::new(),
            tags: card.tags.clone(),
            blocks: card
                .blocks
                .iter()
                .map(|block| crate::models::CodeBlock {
                    id: None,
                    language: block.language.clone(),
                    code: block.code.clone(),
                    sort_order: 0,
                    notes: block
                        .notes
                        .iter()
                        .map(|note| crate::models::LineNote {
                            id: None,
                            line_from: note.line_from,
                            line_to: note.line_to,
                            text: note.text.clone(),
                            is_stale: note.is_stale,
                            sort_order: 0,
                        })
                        .collect(),
                    slots: block
                        .slots
                        .iter()
                        .map(|slot| crate::models::Slot {
                            id: None,
                            name: slot.name.clone(),
                            default_value: slot.default_value.clone(),
                            variants: slot.variants.clone(),
                        })
                        .collect(),
                })
                .collect(),
        };
        crate::commands::cards::card_save_in_tx(&tx, &detail)?;
        // Счётчики подтягиваем из базы по названию импортированной карточки
        report.cards += 1;
        // Карточки всегда создаются заново, даже при совпадении содержимого
        report.recreated += 1;
        report.blocks += detail.blocks.len() as i64;
        report.notes += detail.blocks.iter().map(|b| b.notes.len()).sum::<usize>() as i64;
        report.slots += detail.blocks.iter().map(|b| b.slots.len()).sum::<usize>() as i64;
    }

    // Словарь терминов: термин уникален без учёта регистра. Существующее
    // написание переиспользуется, значение из архива обновляет его.
    for item in &archive.terms {
        let term = match crate::commands::terms::normalize_term(&item.term) {
            Some(term) => term,
            None => continue,
        };
        let existing: Option<i64> = tx
            .query_row(
                "SELECT id FROM terms WHERE term = ?1 COLLATE NOCASE",
                [&term],
                |row| row.get(0),
            )
            .optional()
            .map_err(|e| format!("импорт термина: {e}"))?;
        crate::commands::terms::term_save_impl(&tx, existing, &term, &item.meaning)?;
        report.terms += 1;
    }

    // Подборки: сначала родители, потом дети
    let mut pending: Vec<&ArchiveCollection> = archive.collections.iter().collect();
    let mut created: std::collections::HashMap<String, i64> = std::collections::HashMap::new();
    let mut guard = 0;
    while !pending.is_empty() && guard < 100 {
        guard += 1;
        let mut next = Vec::new();
        for collection in pending {
            let parent_id = match &collection.parent_name {
                Some(parent) => match created.get(parent) {
                    Some(id) => Some(*id),
                    // Родитель ещё не создан: вернёмся к этой подборке на следующем проходе
                    None => {
                        next.push(collection);
                        continue;
                    }
                },
                None => None,
            };
            let id = crate::commands::collections::collection_save_impl(
                &tx,
                None,
                &collection.name,
                parent_id,
            )?;
            created.insert(collection.name.clone(), id);
            report.collections += 1;

            // Карточки подборки ищем по названию среди только что импортированных
            for title in &collection.card_titles {
                let card_id: Option<i64> = tx
                    .query_row(
                        "SELECT id FROM cards WHERE title = ?1 ORDER BY id DESC LIMIT 1",
                        [title],
                        |row| row.get(0),
                    )
                    .ok();
                if let Some(card_id) = card_id {
                    crate::commands::collections::collection_add_card_impl(&tx, id, card_id)?;
                }
            }
        }
        pending = next;
    }

    tx.commit().map_err(|e| format!("фиксация импорта: {e}"))?;
    Ok(report)
}

/// Экспорт карточки в Markdown: код в блоках плюс разбор таблицей.
pub fn export_markdown(conn: &Connection, card_id: i64, path: &Path) -> Result<(), String> {
    let detail = crate::commands::cards::card_get_impl(conn, card_id)?;
    let mut out = String::new();
    out.push_str(&format!("# {}\n\n", detail.title));
    if !detail.description.is_empty() {
        out.push_str(&format!("{}\n\n", detail.description));
    }
    if !detail.tags.is_empty() {
        out.push_str(&format!(
            "Теги: {}\n\n",
            detail
                .tags
                .iter()
                .map(|t| format!("`{t}`"))
                .collect::<Vec<_>>()
                .join(" ")
        ));
    }

    for (index, block) in detail.blocks.iter().enumerate() {
        if detail.blocks.len() > 1 {
            out.push_str(&format!("## Блок {}\n\n", index + 1));
        }
        out.push_str(&format!("```{}\n{}\n```\n\n", block.language, block.code));
        if !block.notes.is_empty() {
            out.push_str("| Строки | Пояснение |\n|---|---|\n");
            for note in &block.notes {
                let range = if note.line_from == note.line_to {
                    format!("{}", note.line_from)
                } else {
                    format!("{}-{}", note.line_from, note.line_to)
                };
                let text = note.text.replace('|', "\\|").replace('\n', " ");
                let stale = if note.is_stale { " (устарело)" } else { "" };
                out.push_str(&format!("| {range} | {text}{stale} |\n"));
            }
            out.push('\n');
        }
    }

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).ok();
    }
    std::fs::write(path, out)
        .map_err(|e| format!("не удалось записать {}: {e}", path.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use crate::models::{CardDetail, CodeBlock, LineNote, Slot, SLOT_DOUBLE_BRACE};

    fn sample() -> CardDetail {
        CardDetail {
            id: None,
            title: "Шаблон".into(),
            description: "описание".into(),
            is_template: true,
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
                code: "def {{name}}(): pass".into(),
                sort_order: 0,
                notes: vec![LineNote {
                    id: None,
                    line_from: 1,
                    line_to: 1,
                    text: "объявление".into(),
                    is_stale: false,
                    sort_order: 0,
                }],
                slots: vec![Slot {
                    id: None,
                    name: "name".into(),
                    default_value: "f".into(),
                    variants: vec!["f".into(), "g".into()],
                }],
            }],
        }
    }

    /// Рабочий каталог для тестов.
    ///
    /// Системный TEMP на этой машине закрыт для записи посторонним процессам,
    /// поэтому файлы тестов держим в target, рядом с артефактами сборки.
    fn test_dir(name: &str) -> std::path::PathBuf {
        let base = std::env::current_dir()
            .unwrap()
            .join("target")
            .join("test-scratch")
            .join(format!("{name}-{}", std::process::id()));
        std::fs::create_dir_all(&base).unwrap();
        base
    }

    #[test]
    fn json_roundtrip_loses_nothing() {
        let mut conn = db::open_in_memory().unwrap();
        crate::commands::cards::card_save_impl(&mut conn, &sample()).unwrap();
        // Термин словаря тоже должен пережить экспорт и импорт
        crate::commands::terms::term_save_impl(&conn, None, "слот", "параметр шаблона").unwrap();

        let archive = build_archive(&conn).unwrap();
        assert_eq!(archive.cards.len(), 1);
        assert_eq!(archive.cards[0].blocks[0].notes.len(), 1);
        assert_eq!(archive.cards[0].blocks[0].slots.len(), 1);
        assert_eq!(archive.terms.len(), 1);

        let dir = test_dir("roundtrip");
        let path = dir.join("archive.json");
        let stats = export_json(&conn, &path).unwrap();
        assert_eq!(stats.cards, 1);
        assert_eq!(stats.notes, 1);
        assert_eq!(stats.slots, 1);
        assert_eq!(stats.terms, 1);

        // Импорт в чистую базу
        let mut fresh = db::open_in_memory().unwrap();
        let report = import_json(&mut fresh, &path).unwrap();
        assert_eq!(report.cards, 1);
        assert_eq!(report.notes, 1);
        assert_eq!(report.slots, 1);
        assert_eq!(report.terms, 1);

        let restored = build_archive(&fresh).unwrap();
        assert_eq!(restored.cards[0].title, "Шаблон");
        assert_eq!(restored.cards[0].blocks[0].slots[0].variants.len(), 2);
        assert_eq!(restored.terms[0].term, "слот");
        // Импортированный термин сразу ищется: индекс словаря собран заново
        let hits = crate::commands::terms::search_terms_impl(&fresh, "параметр").unwrap();
        assert_eq!(hits.len(), 1, "термин из архива должен попасть в поиск");

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn import_creates_new_cards_instead_of_overwriting() {
        let mut conn = db::open_in_memory().unwrap();
        crate::commands::cards::card_save_impl(&mut conn, &sample()).unwrap();
        let before = crate::commands::cards::cards_list_impl(&conn, &Default::default())
            .unwrap()
            .len();

        let dir = test_dir("import");
        let path = dir.join("archive.json");
        export_json(&conn, &path).unwrap();

        // Импорт в ту же базу: карточка должна добавиться, а не замениться
        let report = import_json(&mut conn, &path).unwrap();
        assert_eq!(report.cards, 1);
        let after = crate::commands::cards::cards_list_impl(&conn, &Default::default())
            .unwrap()
            .len();
        assert_eq!(after, before + 1, "исходная карточка на месте, добавлена новая");

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn backup_rotates_to_ten_files() {
        let dir = test_dir("rotate");
        std::fs::create_dir_all(&dir).unwrap();
        let backups = dir.join("backups");
        std::fs::create_dir_all(&backups).unwrap();
        // Двенадцать фальшивых копий: остаться должно десять
        for day in 1..=12 {
            let name = format!("app-202601{day:02}-120000.db");
            std::fs::write(backups.join(name), b"x").unwrap();
        }
        rotate_backups(&backups).unwrap();
        let left = std::fs::read_dir(&backups).unwrap().count();
        assert_eq!(left, BACKUPS_TO_KEEP);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn markdown_contains_code_and_note_table() {
        let mut conn = db::open_in_memory().unwrap();
        let id = crate::commands::cards::card_save_impl(&mut conn, &sample()).unwrap();
        let dir = test_dir("markdown");
        let path = dir.join("card.md");
        export_markdown(&conn, id, &path).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("# Шаблон"));
        assert!(text.contains("```python"));
        assert!(text.contains("| Строки | Пояснение |"));
        assert!(text.contains("объявление"));

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn vacuum_into_creates_a_readable_copy() {
        let dir = test_dir("vacuum");
        std::fs::create_dir_all(&dir).unwrap();
        let db_path = dir.join("app.db");
        {
            let mut conn = db::open(&db_path).unwrap();
            crate::commands::cards::card_save_impl(&mut conn, &sample()).unwrap();
            let backup = run_backup(&conn, &dir).unwrap();
            assert!(backup.exists(), "копия создана");

            // Копия открывается и содержит карточку
            let copy = Connection::open(&backup).unwrap();
            let count: i64 = copy
                .query_row("SELECT count(*) FROM cards", [], |row| row.get(0))
                .unwrap();
            assert_eq!(count, 1);
        }
        std::fs::remove_dir_all(&dir).ok();
    }
}
