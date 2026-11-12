// Словарь терминов: записи и поиск по ним.
//
// Термин живёт отдельно от карточек: у него нет блоков кода и разбора.
// Поиск по словарю идёт тем же запросом, что и по карточкам: разбор
// строки запроса общий (`search::parse_query`), отличается только хранилище.
//
// Индекс term_search устроен так же, как card_search: таблица FTS5 без
// содержимого, строку снимаем командой 'delete' с прежними значениями.
//
// ВАЖНО ПРО РЕГИСТР. SQLite в этой сборке сравнивает LIKE и COLLATE NOCASE
// только для латиницы: 'СЛОТ' и 'слот' для него разные строки. Поэтому
// все сравнения "без учёта регистра" делаются в Rust (to_lowercase),
// а не в SQL. Сравнение идёт по всему списку: словарь - это сотни строк,
// а не миллионы.

use rusqlite::{params, Connection};
use serde::Serialize;

use crate::commands::search::{fts_quote, parse_query};
use crate::db;
use crate::models::Term;

/// Сколько символов значения попадает в сниппет результата поиска.
const SNIPPET_LIMIT: usize = 160;

/// Результат поиска по словарю: термин, значение и строка для подсветки.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TermHit {
    pub id: i64,
    pub term: String,
    pub meaning: String,
    /// Строка значения вокруг совпадения: её показывает список результатов.
    pub snippet: String,
    /// Совпало только значение, а не сам термин: такую находку можно
    /// показать в поиске второй строкой, чтобы термин не заслонял карточки.
    pub crumb: bool,
}

/// Чистит термин: обрезает пробелы и схлопывает внутренние.
/// Возвращает None, если после чистки ничего не осталось.
pub fn normalize_term(raw: &str) -> Option<String> {
    let collapsed = raw.split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.is_empty() {
        None
    } else {
        Some(collapsed)
    }
}

/// Все термины словаря по алфавиту.
fn all_terms(conn: &Connection) -> Result<Vec<Term>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT id, term, meaning, created_at, updated_at FROM terms
             ORDER BY term COLLATE NOCASE",
        )
        .map_err(|e| format!("чтение словаря: {e}"))?;
    let terms = stmt
        .query_map([], |row| {
            Ok(Term {
                id: Some(row.get(0)?),
                term: row.get(1)?,
                meaning: row.get(2)?,
                created_at: row.get(3)?,
                updated_at: row.get(4)?,
            })
        })
        .map_err(|e| format!("чтение словаря: {e}"))?
        .filter_map(Result::ok)
        .collect();
    Ok(terms)
}

/// Термин с таким же написанием без учёта регистра (сравнение в Rust).
fn find_by_term(conn: &Connection, term: &str) -> Result<Option<i64>, String> {
    let needle = term.to_lowercase();
    Ok(all_terms(conn)?
        .into_iter()
        .find(|known| known.term.to_lowercase() == needle)
        .and_then(|known| known.id))
}

/// Список терминов. query = None отдаёт весь словарь по алфавиту,
/// иначе оставляет термины, у которых введённый текст есть в термине
/// или в значении (подстрока, без учёта регистра).
pub fn terms_list_impl(conn: &Connection, query: Option<String>) -> Result<Vec<Term>, String> {
    let terms = all_terms(conn)?;
    let needle = match query.as_deref().and_then(normalize_term) {
        Some(text) => text.to_lowercase(),
        None => return Ok(terms),
    };
    Ok(terms
        .into_iter()
        .filter(|term| {
            term.term.to_lowercase().contains(&needle)
                || term.meaning.to_lowercase().contains(&needle)
        })
        .collect())
}

/// Создаёт или обновляет термин.
///
/// Новый термин: term_id = None. Если такой термин уже есть (без учёта
/// регистра) и это не он сам, возвращается ошибка: в словаре на один
/// термин приходится одно значение.
pub fn term_save_impl(
    conn: &Connection,
    term_id: Option<i64>,
    term: &str,
    meaning: &str,
) -> Result<i64, String> {
    let term = normalize_term(term).ok_or_else(|| "термин не может быть пустым".to_string())?;
    let now = db::now_iso();

    if let Some(duplicate) = find_by_term(conn, &term)? {
        if Some(duplicate) != term_id {
            return Err(format!("термин «{term}» уже есть в словаре"));
        }
    }

    let saved_id = match term_id {
        Some(id) => {
            // ВАЖНО ПО ПОРЯДКУ, как и у карточек (см. write_card_in_tx).
            // У таблицы FTS5 без содержимого удаление сверяет значения колонок
            // с индексом. После UPDATE из таблицы уже не прочесть прежний текст,
            // и 'delete' со свежими значениями молча ничего не снимает: старое
            // значение остаётся в поиске навсегда. Поэтому строку индекса
            // снимаем ДО правки, а новую пишем после.
            db::delete_term_fts_row(conn, id)?;
            let updated = conn
                .execute(
                    "UPDATE terms SET term = ?1, meaning = ?2, updated_at = ?3 WHERE id = ?4",
                    params![term, meaning, now, id],
                )
                .map_err(|e| format!("обновление термина: {e}"))?;
            if updated == 0 {
                return Err(format!("термин {id} не найден"));
            }
            id
        }
        None => {
            conn.execute(
                "INSERT INTO terms (term, meaning, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?3)",
                params![term, meaning, now],
            )
            .map_err(|e| format!("создание термина: {e}"))?;
            conn.last_insert_rowid()
        }
    };

    // Для нового термина строки ещё нет, для правленого она снята выше:
    // дописываем актуальную в обоих случаях.
    db::insert_term_fts_row(conn, saved_id)?;
    Ok(saved_id)
}

/// Удаляет термин вместе со строкой индекса.
pub fn term_delete_impl(conn: &Connection, term_id: i64) -> Result<(), String> {
    db::delete_term_fts_row(conn, term_id)?;
    conn.execute("DELETE FROM terms WHERE id = ?1", [term_id])
        .map_err(|e| format!("удаление термина: {e}"))?;
    Ok(())
}

/// Ищет термины по запросу.
///
/// Устройство то же, что у поиска карточек: слова идут в FTS как префиксы,
/// символьные куски (arr[0], =>) ищутся отдельно, потому что FTS их не
/// индексирует. Совпадение по термину и по значению различается флагом
/// crumb: термин, найденный только по значению, в интерфейсе уходит
/// второй строкой под карточками, чтобы не спорить с ними за внимание.
pub fn search_terms_impl(conn: &Connection, query: &str) -> Result<Vec<TermHit>, String> {
    let parsed = parse_query(query);
    if parsed.words.is_empty() && parsed.symbols.is_empty() {
        return Ok(Vec::new());
    }

    let mut ids: Vec<i64> = Vec::new();
    if !parsed.words.is_empty() {
        let match_expr = parsed
            .words
            .iter()
            .map(|word| fts_quote(word))
            .collect::<Vec<_>>()
            .join(" AND ");
        // snippet() над таблицей без содержимого возвращает NULL,
        // поэтому из FTS берём только rowid.
        let mut stmt = conn
            .prepare(
                "SELECT rowid FROM term_search
                 WHERE term_search MATCH ?1 ORDER BY rank",
            )
            .map_err(|e| format!("поиск по словарю: {e}"))?;
        let rows = stmt
            .query_map([&match_expr], |row| row.get::<_, i64>(0))
            .map_err(|e| format!("поиск по словарю: {e}"))?;
        for row in rows.flatten() {
            ids.push(row);
        }
    }

    // Пунктуация: сравнение по подстроке в Rust (LIKE не подходит, см. выше)
    let lowered = query.trim().to_lowercase();
    if !parsed.symbols.is_empty() && !lowered.is_empty() {
        for term in all_terms(conn)? {
            let matches = term.term.to_lowercase().contains(&lowered)
                || term.meaning.to_lowercase().contains(&lowered);
            if let (true, Some(id)) = (matches, term.id) {
                ids.push(id);
            }
        }
    }

    // Один термин - один результат, порядок: сначала совпадения по термину
    let mut seen: std::collections::HashSet<i64> = std::collections::HashSet::new();
    let mut strict: Vec<TermHit> = Vec::new();
    let mut crumbs: Vec<TermHit> = Vec::new();
    for id in ids {
        if !seen.insert(id) {
            continue;
        }
        let Some(hit) = term_hit(conn, id, &parsed.words)? else {
            continue;
        };
        if hit.crumb {
            crumbs.push(hit);
        } else {
            strict.push(hit);
        }
    }
    strict.sort_by(|first, second| {
        first.term.to_lowercase().cmp(&second.term.to_lowercase())
    });
    crumbs.sort_by(|first, second| first.term.to_lowercase().cmp(&second.term.to_lowercase()));
    strict.extend(crumbs);
    Ok(strict)
}

/// Собирает результат по термину: строка значения вокруг совпадения.
fn term_hit(conn: &Connection, term_id: i64, words: &[String]) -> Result<Option<TermHit>, String> {
    let row: Option<(String, String)> = conn
        .query_row(
            "SELECT term, meaning FROM terms WHERE id = ?1",
            [term_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .ok();
    let Some((term, meaning)) = row else {
        // Термин успели удалить: не падаем, просто пропускаем.
        return Ok(None);
    };
    let (snippet, matched_term) = snippet_around(&term, &meaning, words);
    Ok(Some(TermHit {
        id: term_id,
        term,
        meaning,
        snippet,
        crumb: !matched_term,
    }))
}

/// Строка значения вокруг первого совпадения.
///
/// Значения бывают длинными, а в списке результатов нужна одна строка,
/// по которой видно, что именно нашлось. Второе значение пары - совпал ли
/// сам термин: от него зависит, показывать находку в основной группе или
/// второй строкой.
fn snippet_around(term: &str, meaning: &str, words: &[String]) -> (String, bool) {
    let lowered_term = term.to_lowercase();
    let matched_term = words
        .iter()
        .any(|word| lowered_term.contains(&word.to_lowercase()));

    let lowered_meaning = meaning.to_lowercase();
    let needle = words
        .iter()
        .map(|word| word.to_lowercase())
        .find(|word| lowered_meaning.contains(word));
    let line = match &needle {
        Some(word) => meaning
            .lines()
            .find(|line| line.to_lowercase().contains(word))
            .unwrap_or(meaning)
            .trim()
            .to_string(),
        None => meaning.lines().next().unwrap_or("").trim().to_string(),
    };
    let snippet = if line.chars().count() > SNIPPET_LIMIT {
        let head: String = line.chars().take(SNIPPET_LIMIT).collect();
        format!("{head}...")
    } else {
        line
    };
    (snippet, matched_term)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seed() -> Connection {
        let conn = db::open_in_memory().unwrap();
        // Значения подобраны так, чтобы слова не пересекались между записями:
        // FTS ищет по словам, а не по всему тексту целиком.
        for (term, meaning) in [
            ("слот", "Параметр шаблона в двойных фигурных скобках"),
            ("FTS5", "Полнотекстовый поиск в SQLite"),
            ("индекс", "Доступ к элементу по номеру: arr[0] в Python"),
        ] {
            term_save_impl(&conn, None, term, meaning).unwrap();
        }
        conn
    }

    #[test]
    fn save_and_list_terms() {
        let conn = seed();
        let all = terms_list_impl(&conn, None).unwrap();
        assert_eq!(all.len(), 3, "три термина в словаре");
        let names: Vec<&str> = all.iter().map(|t| t.term.as_str()).collect();
        assert_eq!(names, vec!["FTS5", "индекс", "слот"]);
    }

    #[test]
    fn update_keeps_index_in_sync() {
        let conn = seed();
        let id = terms_list_impl(&conn, Some("слот".into())).unwrap()[0]
            .id
            .unwrap();
        term_save_impl(&conn, Some(id), "слот", "Новое значение про штуку").unwrap();
        // Что лежит в индексе после правки: строка на термин, старых слов нет
        let indexed: Vec<i64> = {
            let mut stmt = conn
                .prepare("SELECT rowid FROM term_search")
                .unwrap();
            let rows: Vec<i64> = stmt
                .query_map([], |row| row.get(0))
                .unwrap()
                .filter_map(Result::ok)
                .collect();
            rows
        };
        assert_eq!(
            indexed,
            vec![1, 2, 3],
            "по строке на термин: правка не должна плодить дубли"
        );
        let old: i64 = conn
            .query_row(
                "SELECT count(*) FROM term_search WHERE term_search MATCH '\"фигурных\"'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(old, 0, "старое значение осталось в индексе");
        let fresh: i64 = conn
            .query_row(
                "SELECT count(*) FROM term_search WHERE term_search MATCH '\"штуку\"'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(fresh, 1, "новое значение проиндексировано");
        // WHERE в MATCH-запросе отсекает строки по rowid, а порядок выдачи
        // считает целым индексом: без rowid = ? поиск по слову не работает.
        let via_rowid: i64 = conn
            .query_row(
                "SELECT count(*) FROM term_search WHERE term_search MATCH '\"фигурных\"' AND rowid = ?1",
                [id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(via_rowid, 0, "строка этого термина старое слово не содержит");

        // Поиск находит по новому значению...
        let hits = search_terms_impl(&conn, "штуку").unwrap();
        assert_eq!(hits.len(), 1);
        // ...и больше не находит по старому
        let hits = search_terms_impl(&conn, "фигурных").unwrap();
        assert!(
            hits.is_empty(),
            "старое значение не должно остаться в индексе: {hits:?}"
        );
    }

    /// Кириллица: дубль в другом регистре не создаётся.
    #[test]
    fn duplicate_term_is_rejected() {
        let conn = seed();
        let error = term_save_impl(&conn, None, "СЛОТ", "другое").unwrap_err();
        assert!(error.contains("уже есть"), "получено: {error}");
        // Свой же термин при правке дублем не считается
        let id = terms_list_impl(&conn, Some("слот".into())).unwrap()[0]
            .id
            .unwrap();
        assert!(term_save_impl(&conn, Some(id), "Слот", "правка").is_ok());
        let count: i64 = conn
            .query_row("SELECT count(*) FROM terms", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 3, "новых терминов не появилось");
    }

    #[test]
    fn search_finds_by_term_and_meaning() {
        let conn = seed();
        // По началу термина
        let hits = search_terms_impl(&conn, "сло").unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].term, "слот");
        assert!(!hits[0].crumb, "совпадение по термину - не крошка");
        // По слову из значения, в любом месте текста
        let hits = search_terms_impl(&conn, "полнотекстовый").unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].term, "FTS5");
        assert!(hits[0].crumb, "совпадение только по значению помечено");
        assert!(
            hits[0].snippet.contains("Полнотекстовый"),
            "сниппет: {}",
            hits[0].snippet
        );
    }

    /// Пунктуация не индексируется FTS, но словарь обязан её находить.
    #[test]
    fn punctuation_query_finds_term() {
        let conn = seed();
        let hits = search_terms_impl(&conn, "arr[0]").unwrap();
        assert_eq!(hits.len(), 1, "arr[0] должно найти термин индекса");
        assert_eq!(hits[0].term, "индекс");
        // Без совпадения в тексте запрос ничего не находит
        assert!(search_terms_impl(&conn, "zzz[1]").unwrap().is_empty());
    }

    #[test]
    fn delete_removes_from_index_and_no_duplicates() {
        let conn = seed();
        let id = terms_list_impl(&conn, Some("FTS5".into())).unwrap()[0]
            .id
            .unwrap();
        term_delete_impl(&conn, id).unwrap();
        assert!(search_terms_impl(&conn, "FTS5").unwrap().is_empty());
        // Повторное удаление не роняет базу и не портит индекс
        assert!(term_delete_impl(&conn, id).is_ok());
        // Слово и символ в одном запросе не дают дубля
        let hits = search_terms_impl(&conn, "слот[0]").unwrap();
        assert!(hits.len() <= 1, "термин не должен дублироваться");
    }

    #[test]
    fn empty_query_returns_nothing() {
        let conn = seed();
        assert!(search_terms_impl(&conn, "   ").unwrap().is_empty());
    }

    /// Фильтр списка: подстрока без учёта регистра, кириллица тоже.
    #[test]
    fn filter_by_input_is_case_insensitive_substring() {
        let conn = seed();
        assert_eq!(terms_list_impl(&conn, Some("fts".into())).unwrap().len(), 1);
        assert_eq!(terms_list_impl(&conn, Some("FTS".into())).unwrap().len(), 1);
        // Подстрока из середины слова тоже работает
        let found = terms_list_impl(&conn, Some("декс".into())).unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].term, "индекс");
        // Кириллица в верхнем регистре
        assert_eq!(terms_list_impl(&conn, Some("СЛО".into())).unwrap().len(), 1);
        // Значение термина тоже участвует в фильтре
        assert_eq!(terms_list_impl(&conn, Some("фигурных".into())).unwrap().len(), 1);
    }
}
