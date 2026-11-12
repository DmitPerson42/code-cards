// Корень библиотеки: регистрация модулей, команд и запуск приложения.

pub mod commands;
pub mod db;
pub mod line_diff;
pub mod models;
pub mod slots;

use std::path::PathBuf;
use std::sync::Mutex;

use tauri::{Manager, State};

use crate::commands::cards;
use crate::models::{CardDetail, CardSummary, Filter, SaveResult};

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    pub app_version: String,
    pub sqlite_version: String,
    pub fts5_available: bool,
}

/// Общее состояние приложения: одно соединение с базой под мьютексом.
pub struct AppState {
    pub conn: Mutex<rusqlite::Connection>,
    /// Каталог данных: в нём лежат app.db и backups/.
    pub data_dir: PathBuf,
}

/// Ручная резервная копия базы (VACUUM INTO).
#[tauri::command]
fn backup_run(state: State<'_, AppState>) -> Result<String, String> {
    let conn = state.conn.lock().map_err(|e| e.to_string())?;
    let path = commands::backup::run_backup(&conn, &state.data_dir)?;
    Ok(path.to_string_lossy().to_string())
}

/// Экспорт всего архива в JSON.
#[tauri::command]
fn backup_export_json(
    state: State<'_, AppState>,
    path: String,
) -> Result<crate::models::Stats, String> {
    let conn = state.conn.lock().map_err(|e| e.to_string())?;
    commands::backup::export_json(&conn, std::path::Path::new(&path))
}

/// Импорт архива из JSON. Существующие карточки не перезаписываются.
#[tauri::command]
fn backup_import_json(
    state: State<'_, AppState>,
    path: String,
) -> Result<crate::models::ImportReport, String> {
    let mut conn = state.conn.lock().map_err(|e| e.to_string())?;
    commands::backup::import_json(&mut conn, std::path::Path::new(&path))
}

/// Экспорт одной карточки в Markdown.
#[tauri::command]
fn backup_export_markdown(
    state: State<'_, AppState>,
    card_id: i64,
    path: String,
) -> Result<(), String> {
    let conn = state.conn.lock().map_err(|e| e.to_string())?;
    commands::backup::export_markdown(&conn, card_id, std::path::Path::new(&path))
}

/// Сведения об окружении: версия приложения и версия SQLite.
/// Показывает, что сборка rusqlite с bundled и FTS5 работает.
#[tauri::command]
fn app_info() -> Result<AppInfo, String> {
    let conn = rusqlite::Connection::open_in_memory().map_err(|e| e.to_string())?;
    let sqlite_version: String = conn
        .query_row("select sqlite_version()", [], |row| row.get(0))
        .map_err(|e| e.to_string())?;
    // FTS5 может быть собран, но не зарегистрирован: проверяем созданием таблицы.
    let fts5_available = conn
        .execute_batch("create virtual table probe_fts using fts5(a);")
        .is_ok();
    Ok(AppInfo {
        app_version: env!("CARGO_PKG_VERSION").to_string(),
        sqlite_version,
        fts5_available,
    })
}

/// Каталог данных приложения: data/ рядом с исполняемым файлом приложения.
fn data_dir() -> Result<PathBuf, String> {
    let exe = std::env::current_exe().map_err(|e| format!("путь к приложению: {e}"))?;
    let base = exe
        .parent()
        .ok_or_else(|| "каталог приложения не найден".to_string())?
        .to_path_buf();
    Ok(base.join("data"))
}

#[tauri::command]
fn cards_list(state: State<'_, AppState>, filter: Filter) -> Result<Vec<CardSummary>, String> {
    let conn = state.conn.lock().map_err(|e| e.to_string())?;
    cards::cards_list_impl(&conn, &filter)
}

#[tauri::command]
fn card_get(state: State<'_, AppState>, id: i64) -> Result<CardDetail, String> {
    let conn = state.conn.lock().map_err(|e| e.to_string())?;
    cards::card_get_impl(&conn, id)
}

#[tauri::command]
fn card_save(
    state: State<'_, AppState>,
    detail: CardDetail,
    force: bool,
) -> Result<SaveResult, String> {
    let mut conn = state.conn.lock().map_err(|e| e.to_string())?;
    // Первый вызов возвращает needs_confirmation без записи в базу,
    // повторный с force = true сохраняет (раздел 3.5 плана).
    let result = cards::card_save_with_diff(&mut conn, &detail, force)?;
    Ok(match result {
        SaveResult::Ok { .. } => SaveResult::Ok {
            id: cards::saved_card_id(&conn, &detail),
        },
        other => other,
    })
}

#[tauri::command]
fn card_delete(state: State<'_, AppState>, id: i64) -> Result<(), String> {
    let conn = state.conn.lock().map_err(|e| e.to_string())?;
    cards::card_delete_impl(&conn, id)
}

/// Переключает избранное и возвращает новое значение.
#[tauri::command]
fn card_toggle_favorite(state: State<'_, AppState>, id: i64) -> Result<bool, String> {
    let conn = state.conn.lock().map_err(|e| e.to_string())?;
    cards::card_toggle_favorite_impl(&conn, id)
}

/// Поиск по названию, описанию, тегам, коду и текстам разбора.
#[tauri::command]
fn search(
    state: State<'_, AppState>,
    query: String,
    filter: Filter,
) -> Result<Vec<CardSummary>, String> {
    let conn = state.conn.lock().map_err(|e| e.to_string())?;
    commands::search::search_impl(&conn, &query, &filter)
}

/// Список тегов с числом карточек.
#[tauri::command]
fn tag_list(state: State<'_, AppState>) -> Result<Vec<crate::models::TagWithCount>, String> {
    let conn = state.conn.lock().map_err(|e| e.to_string())?;
    commands::tags::tag_list_impl(&conn)
}

/// Полностью заменяет набор тегов карточки.
#[tauri::command]
fn card_set_tags(
    state: State<'_, AppState>,
    card_id: i64,
    tags: Vec<String>,
) -> Result<(), String> {
    let mut conn = state.conn.lock().map_err(|e| e.to_string())?;
    let tx = conn
        .transaction()
        .map_err(|e| format!("не удалось начать транзакцию: {e}"))?;
    commands::tags::set_card_tags_tx(&tx, card_id, &tags)?;
    db::rebuild_fts(&tx, card_id)?;
    tx.commit().map_err(|e| format!("фиксация тегов: {e}"))?;
    Ok(())
}

/// Создаёт, обновляет или удаляет диапазон разбора.
#[tauri::command]
fn note_save(
    state: State<'_, AppState>,
    block_id: i64,
    note: crate::models::LineNote,
) -> Result<Option<i64>, String> {
    let conn = state.conn.lock().map_err(|e| e.to_string())?;
    match note.id {
        Some(_) => {
            commands::notes::note_update_impl(&conn, &note)?;
            Ok(note.id)
        }
        None => Ok(Some(commands::notes::note_create_impl(
            &conn,
            block_id,
            note.line_from,
            note.line_to,
            &note.text,
        )?)),
    }
}

#[tauri::command]
fn note_delete(state: State<'_, AppState>, note_id: i64) -> Result<(), String> {
    let conn = state.conn.lock().map_err(|e| e.to_string())?;
    commands::notes::note_delete_impl(&conn, note_id)
}

/// Дерево подборок.
#[tauri::command]
fn collection_list(state: State<'_, AppState>) -> Result<Vec<crate::models::CollectionNode>, String> {
    let conn = state.conn.lock().map_err(|e| e.to_string())?;
    commands::collections::collection_list_impl(&conn)
}

/// Создаёт или переименовывает подборку.
#[tauri::command]
fn collection_save(
    state: State<'_, AppState>,
    id: Option<i64>,
    name: String,
    parent_id: Option<i64>,
) -> Result<i64, String> {
    let conn = state.conn.lock().map_err(|e| e.to_string())?;
    commands::collections::collection_save_impl(&conn, id, &name, parent_id)
}

#[tauri::command]
fn collection_delete(state: State<'_, AppState>, id: i64) -> Result<(), String> {
    let conn = state.conn.lock().map_err(|e| e.to_string())?;
    commands::collections::collection_delete_impl(&conn, id)
}

/// Задаёт состав и порядок карточек в подборке.
#[tauri::command]
fn collection_set_cards(
    state: State<'_, AppState>,
    collection_id: i64,
    card_ids: Vec<i64>,
) -> Result<(), String> {
    let conn = state.conn.lock().map_err(|e| e.to_string())?;
    commands::collections::collection_set_cards_impl(&conn, collection_id, &card_ids)
}

#[tauri::command]
fn collection_add_card(
    state: State<'_, AppState>,
    collection_id: i64,
    card_id: i64,
) -> Result<(), String> {
    let conn = state.conn.lock().map_err(|e| e.to_string())?;
    commands::collections::collection_add_card_impl(&conn, collection_id, card_id)
}

/// Подстановка значений в карточку, возвращает блоки с готовым кодом.
/// slot_syntax = jinja_off отдаёт код без изменений.
#[tauri::command]
fn render_card(
    state: State<'_, AppState>,
    card_id: i64,
    values: std::collections::HashMap<String, String>,
) -> Result<Vec<String>, String> {
    let conn = state.conn.lock().map_err(|e| e.to_string())?;
    let detail = commands::cards::card_get_impl(&conn, card_id)?;
    let mut out = Vec::new();
    for block in &detail.blocks {
        let mut defaults = std::collections::HashMap::new();
        for slot in &block.slots {
            defaults.insert(slot.name.clone(), slot.default_value.clone());
        }
        out.push(crate::slots::render_code(
            &block.code,
            &detail.slot_syntax,
            &defaults,
            &values,
        ));
    }
    Ok(out)
}

/// Находит слоты в коде блока: для подсветки в редакторе.
#[tauri::command]
fn block_slots_scan(
    code: String,
    slot_syntax: String,
) -> Result<Vec<crate::slots::FoundSlot>, String> {
    Ok(crate::slots::find_slots(&code, &slot_syntax))
}

/// Превращает выделенный фрагмент строки в слот.
#[tauri::command]
fn make_slot_command(
    code: String,
    line: usize,
    range_start: usize,
    range_end: usize,
    name: String,
) -> Result<(String, String), String> {
    crate::slots::make_slot(&code, line, range_start, range_end, &name)
}

/// Записывает список слотов блока.
#[tauri::command]
fn slots_set(
    state: State<'_, AppState>,
    block_id: i64,
    slots: Vec<crate::models::Slot>,
) -> Result<(), String> {
    let conn = state.conn.lock().map_err(|e| e.to_string())?;
    crate::slots::slots_set_impl(&conn, block_id, &slots)
}

// ---------------------------------------------------------------------------
// Словарь терминов
// ---------------------------------------------------------------------------

/// Список терминов. query = null отдаёт весь словарь по алфавиту,
/// иначе фильтр по введённому тексту (подстрока в термине или значении).
#[tauri::command]
fn terms_list(
    state: State<'_, AppState>,
    query: Option<String>,
) -> Result<Vec<crate::models::Term>, String> {
    let conn = state.conn.lock().map_err(|e| e.to_string())?;
    commands::terms::terms_list_impl(&conn, query)
}

/// Создаёт или обновляет термин. Повторяющийся термин отклоняется.
#[tauri::command]
fn term_save(
    state: State<'_, AppState>,
    id: Option<i64>,
    term: String,
    meaning: String,
) -> Result<i64, String> {
    let conn = state.conn.lock().map_err(|e| e.to_string())?;
    commands::terms::term_save_impl(&conn, id, &term, &meaning)
}

#[tauri::command]
fn term_delete(state: State<'_, AppState>, id: i64) -> Result<(), String> {
    let conn = state.conn.lock().map_err(|e| e.to_string())?;
    commands::terms::term_delete_impl(&conn, id)
}

/// Поиск по словарю тем же запросом, что и по карточкам.
#[tauri::command]
fn search_terms(
    state: State<'_, AppState>,
    query: String,
) -> Result<Vec<commands::terms::TermHit>, String> {
    let conn = state.conn.lock().map_err(|e| e.to_string())?;
    commands::terms::search_terms_impl(&conn, &query)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let data_dir_path = match data_dir() {
        Ok(dir) => dir,
        Err(e) => {
            eprintln!("[Code Cards] {e}");
            std::process::exit(1);
        }
    };
    let db_path = data_dir_path.join("app.db");
    let conn = match db::open(&db_path) {
        Ok(conn) => conn,
        Err(e) => {
            eprintln!("[Code Cards] не удалось открыть базу: {e}");
            std::process::exit(1);
        }
    };
    // Версия SQLite видна в логе Rust при запуске (требование этапа 0).
    let version: String = conn
        .query_row("select sqlite_version()", [], |row| row.get(0))
        .unwrap_or_else(|_| "неизвестно".to_string());
    println!("[Code Cards] база: {}", db_path.display());
    println!("[Code Cards] SQLite {version}");

    // Копия базы при запуске: если прошло больше суток с последней (раздел 3.6).
    if commands::backup::backup_due(&data_dir_path) {
        match commands::backup::run_backup(&conn, &data_dir_path) {
            Ok(path) => println!("[Code Cards] копия базы: {}", path.display()),
            Err(e) => eprintln!("[Code Cards] копия базы не сделана: {e}"),
        }
    }

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(AppState {
            conn: Mutex::new(conn),
            data_dir: data_dir_path,
        })
        .invoke_handler(tauri::generate_handler![
            app_info,
            cards_list,
            card_get,
            card_save,
            card_delete,
            card_toggle_favorite,
            card_set_tags,
            tag_list,
            search,
            note_save,
            note_delete,
            collection_list,
            collection_save,
            collection_delete,
            collection_set_cards,
            collection_add_card,
            render_card,
            block_slots_scan,
            make_slot_command,
            slots_set,
            backup_run,
            backup_export_json,
            backup_import_json,
            backup_export_markdown,
            terms_list,
            term_save,
            term_delete,
            search_terms
        ])
        .setup(|app| {
            // Окно уже создано из конфигурации, здесь только проверка доступности главного окна.
            if app.get_webview_window("main").is_none() {
                eprintln!("[Code Cards] главное окно не создано");
            }
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("не удалось запустить приложение Code Cards");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sqlite_builds_with_fts5() {
        let info = app_info().expect("app_info должен отработать");
        assert!(!info.sqlite_version.is_empty());
        assert!(info.fts5_available, "FTS5 должен быть доступен");
    }
}
