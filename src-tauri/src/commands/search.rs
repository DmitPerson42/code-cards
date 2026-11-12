// Поиск по карточкам (раздел 3.2 плана).
//
// Два прохода, потому что FTS и обычный LIKE умеют разное:
// 1. Слова (буквы и цифры) уходят в FTS как префиксный MATCH.
// 2. Всё, что не буква и не цифра (arr[0], ===, ->), FTS не индексирует,
//    поэтому такие куски ищутся как LIKE по коду блоков.
// Результаты объединяются по card_id без дублей.
//
// Поиск точный, без нечёткости: опечатки не исправляются намеренно.

use rusqlite::Connection;

use crate::commands::cards;
use crate::models::{CardSummary, Filter};

/// Разобранный пользовательский запрос.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ParsedQuery {
    /// Слова и числа: для FTS.
    pub words: Vec<String>,
    /// Символьные куски: для LIKE по коду.
    pub symbols: Vec<String>,
}

/// Считается ли символ частью слова.
///
/// Подчёркивание относим к словам: `lru_cache` это один идентификатор,
/// пользователь ждёт поиск по нему, а не отдельно по `lru` и `cache`.
fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// Разбивает запрос на слова и символьные куски.
///
/// Примеры:
/// `lru_cache` -> слова [lru_cache]
/// `arr[0]`    -> слова [arr, 0], символы [[]
/// `x === y`   -> слова [x, y], символы [===]
pub fn parse_query(raw: &str) -> ParsedQuery {
    let mut words = Vec::new();
    let mut symbols = Vec::new();
    let mut current = String::new();
    let mut current_is_word: Option<bool> = None;

    // Замыкание меняет words и symbols, поэтому само должно быть mut
    let mut flush = |buffer: &mut String, kind: Option<bool>| {
        if buffer.is_empty() {
            return;
        }
        let piece = std::mem::take(buffer);
        match kind {
            Some(true) => words.push(piece),
            // Символьные куски склеиваются: `arr[0]` даёт не `[` и `]`,
            // а один кусок `[]`. Пробелы в куски не попадают: это разделитель,
            // а не пунктуация, которую пользователь ищет.
            Some(false) => {
                let meaningful: String = piece.chars().filter(|c| !c.is_whitespace()).collect();
                if meaningful.is_empty() {
                    return;
                }
                if let Some(last) = symbols.last_mut() {
                    let last: &mut String = last;
                    last.push_str(&meaningful);
                } else {
                    symbols.push(meaningful);
                }
            }
            None => {}
        }
    };

    for c in raw.chars() {
        let word = is_word_char(c);
        match current_is_word {
            Some(kind) if kind == word => current.push(c),
            Some(_) => {
                flush(&mut current, current_is_word);
                current.push(c);
                current_is_word = Some(word);
            }
            None => {
                current.push(c);
                current_is_word = Some(word);
            }
        }
    }
    flush(&mut current, current_is_word);

    // Символьные куски из одних пробелов бессмысленны
    symbols.retain(|s| !s.trim().is_empty());
    ParsedQuery { words, symbols }
}

/// Экранирует слово для FTS: кавычки внутри слова удваиваются.
fn fts_quote(word: &str) -> String {
    format!("\"{}\"*", word.replace('"', "\"\""))
}

/// Ищет карточки по запросу с учётом фильтра.
pub fn search_impl(
    conn: &Connection,
    query: &str,
    filter: &Filter,
) -> Result<Vec<CardSummary>, String> {
    let parsed = parse_query(query);
    // Исходный текст запроса без краевых пробелов: по нему ищем строку
    // для подсветки, чтобы пользователь видел `arr[0]`, а не отдельную скобку
    let full_words = query;

    // Пустой запрос: обычный список по фильтру
    if parsed.words.is_empty() && parsed.symbols.is_empty() {
        return cards::cards_list_impl(conn, filter);
    }

    // Первый проход: FTS по словам. Собираем id и сниппет.
    let mut found: Vec<(i64, String)> = Vec::new();
    let mut seen: std::collections::HashSet<i64> = std::collections::HashSet::new();

    if !parsed.words.is_empty() {
        let match_expr = parsed
            .words
            .iter()
            .map(|w| fts_quote(w))
            .collect::<Vec<_>>()
            .join(" AND ");

        // snippet() и highlight() над таблицей без содержимого (content='')
        // возвращают NULL: хранимых текстов в индексе нет. Поэтому сниппет
        // строится отдельно по коду блока, а из FTS берём только rowid.
        let sql = "SELECT rowid
                   FROM card_search
                   WHERE card_search MATCH ?1
                   ORDER BY rank";
        let mut collected: Vec<i64> = Vec::new();
        let mut stmt = conn
            .prepare(sql)
            .map_err(|e| format!("запрос поиска: {e}"))?;
        let rows = stmt
            .query_map([&match_expr], |row| row.get::<_, i64>(0))
            .map_err(|e| format!("поиск: {e}"))?;
        for row in rows.flatten() {
            collected.push(row);
        }
        for card_id in collected {
            if seen.insert(card_id) {
                found.push((card_id, snippet_for_card(conn, card_id, &parsed.words)?));
            }
        }
    }

    // Второй проход: символьные куски ищутся LIKE по коду.
    // Пунктуация должна совпасть целиком, поэтому LIKE без подстановок.
    for symbol in &parsed.symbols {
        let pattern = format!("%{}%", symbol);
        let mut stmt = conn
            .prepare(
                "SELECT DISTINCT cb.card_id, cb.code
                 FROM code_blocks cb
                 WHERE cb.code LIKE ?1 COLLATE NOCASE
                 ORDER BY cb.card_id",
            )
            .map_err(|e| format!("запрос поиска по символам: {e}"))?;
        let rows = stmt
            .query_map([&pattern], |row| {
                Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
            })
            .map_err(|e| format!("поиск по символам: {e}"))?;
        for row in rows.flatten() {
            let (card_id, code) = row;
            // Сниппет ищем по всему введённому запросу, а не по куску пунктуации:
            // пользователь хочет увидеть строку с `arr[0]`, а не с одной скобкой
            let needle = full_words.trim();
            let snippet = if needle.is_empty() || snippet_around(&code, needle).is_empty() {
                snippet_around(&code, symbol)
            } else {
                snippet_around(&code, needle)
            };
            if seen.insert(card_id) {
                found.push((card_id, snippet));
            }
        }
    }

    // Отбираем полные карточки по фильтру, сохраняя порядок находок
    let raw_query = query.trim();
    let mut result = Vec::new();
    for (card_id, snippet) in found {
        match card_by_id(conn, card_id) {
            Ok(summary) => {
                if matches_filter(conn, card_id, filter)? {
                    let mut summary = summary;
                    // Сниппет по точной подстроке запроса информативнее:
                    // для `arr[0]` это строка с индексацией, а не со словом `arr`.
                    // Если точной подстроки нет, остаётся сниппет от прохода по словам.
                    let precise = if raw_query.is_empty() {
                        String::new()
                    } else {
                        snippet_for_code_contains(conn, card_id, raw_query)?
                    };
                    summary.snippet = Some(if precise.is_empty() { snippet } else { precise });
                    result.push(summary);
                }
            }
            // Карточку успели удалить: не падаем, просто пропускаем
            Err(_) => continue,
        }
    }
    Ok(result)
}

/// Ищет строку кода, содержащую фрагмент запроса целиком.
///
/// Нужна для запросов с пунктуацией: `arr[0]` должен подсвечиваться
/// вместе со скобками, иначе сниппет не показывает, что именно совпало.
fn snippet_for_code_contains(
    conn: &Connection,
    card_id: i64,
    needle: &str,
) -> Result<String, String> {
    let mut stmt = conn
        .prepare("SELECT code FROM code_blocks WHERE card_id = ?1 ORDER BY sort_order, id")
        .map_err(|e| format!("код для точного сниппета: {e}"))?;
    let codes: Vec<String> = stmt
        .query_map([card_id], |row| row.get(0))
        .map_err(|e| format!("код для точного сниппета: {e}"))?
        .filter_map(Result::ok)
        .collect();
    let lowered = needle.to_lowercase();
    for code in codes {
        if let Some(line) = code.lines().find(|line| line.to_lowercase().contains(&lowered)) {
            let trimmed = line.trim();
            return Ok(if trimmed.chars().count() > 120 {
                let head: String = trimmed.chars().take(120).collect();
                format!("{head}...")
            } else {
                trimmed.to_string()
            });
        }
    }
    Ok(String::new())
}

/// Строит сниппет для найденной карточки по первому совпавшему тексту.
///
/// Порядок поиска как у индекса: название, описание, теги, код блоков,
/// разбор. Ищется первое слово запроса, чтобы подсветка попадала в цель.
fn snippet_for_card(
    conn: &Connection,
    card_id: i64,
    words: &[String],
) -> Result<String, String> {
    let needle = match words.first() {
        Some(word) => word.to_lowercase(),
        None => return Ok(String::new()),
    };

    let mut candidates: Vec<String> = Vec::new();
    let (title, description): (String, String) = conn
        .query_row(
            "SELECT title, description FROM cards WHERE id = ?1",
            [card_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .map_err(|e| format!("название и описание для сниппета: {e}"))?;
    candidates.push(title);
    candidates.push(description);
    candidates.push(cards::card_tag_names(conn, card_id)?.join(" "));

    let mut stmt = conn
        .prepare("SELECT code FROM code_blocks WHERE card_id = ?1 ORDER BY sort_order, id")
        .map_err(|e| format!("код для сниппета: {e}"))?;
    let codes: Vec<String> = stmt
        .query_map([card_id], |row| row.get(0))
        .map_err(|e| format!("код для сниппета: {e}"))?
        .filter_map(Result::ok)
        .collect();
    candidates.extend(codes);

    let mut note_stmt = conn
        .prepare(
            "SELECT n.text FROM line_notes n
             JOIN code_blocks b ON b.id = n.block_id
             WHERE b.card_id = ?1 ORDER BY b.sort_order, n.line_from",
        )
        .map_err(|e| format!("разбор для сниппета: {e}"))?;
    let notes: Vec<String> = note_stmt
        .query_map([card_id], |row| row.get(0))
        .map_err(|e| format!("разбор для сниппета: {e}"))?
        .filter_map(Result::ok)
        .collect();
    candidates.extend(notes);

    for text in candidates {
        if text.to_lowercase().contains(&needle) {
            let line = text
                .lines()
                .find(|line| line.to_lowercase().contains(&needle))
                .unwrap_or(&text);
            let trimmed = line.trim();
            return Ok(if trimmed.chars().count() > 120 {
                let head: String = trimmed.chars().take(120).collect();
                format!("{head}...")
            } else {
                trimmed.to_string()
            });
        }
    }
    Ok(String::new())
}

/// Строка кода вокруг совпадения: для подсветки в списке.
fn snippet_around(code: &str, needle: &str) -> String {
    for line in code.lines() {
        if line.to_lowercase().contains(&needle.to_lowercase()) {
            let trimmed = line.trim();
            return if trimmed.len() > 120 {
                let head: String = trimmed.chars().take(120).collect();
                format!("{head}...")
            } else {
                trimmed.to_string()
            };
        }
    }
    String::new()
}

/// Короткая карточка по id, без сниппета.
fn card_by_id(conn: &Connection, card_id: i64) -> Result<CardSummary, String> {
    let filter = Filter::default();
    let all = cards::cards_list_impl(conn, &filter)?;
    all.into_iter()
        .find(|c| c.id == card_id)
        .ok_or_else(|| format!("карточка {card_id} не найдена"))
}

/// Проверяет, проходит ли карточка фильтр (теги, язык, подборка, избранное).
fn matches_filter(conn: &Connection, card_id: i64, filter: &Filter) -> Result<bool, String> {
    if filter.favorite_only {
        let favorite: i64 = conn
            .query_row("SELECT favorite FROM cards WHERE id = ?1", [card_id], |row| {
                row.get(0)
            })
            .map_err(|e| format!("избранное карточки: {e}"))?;
        if favorite == 0 {
            return Ok(false);
        }
    }
    for tag in &filter.tags {
        let exists: i64 = conn
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM card_tags ct JOIN tags t ON t.id = ct.tag_id
                               WHERE ct.card_id = ?1 AND t.name = ?2 COLLATE NOCASE)",
                rusqlite::params![card_id, tag],
                |row| row.get(0),
            )
            .map_err(|e| format!("тег карточки: {e}"))?;
        if exists == 0 {
            return Ok(false);
        }
    }
    if let Some(language) = &filter.language {
        let exists: i64 = conn
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM code_blocks WHERE card_id = ?1 AND language = ?2)",
                rusqlite::params![card_id, language],
                |row| row.get(0),
            )
            .map_err(|e| format!("язык карточки: {e}"))?;
        if exists == 0 {
            return Ok(false);
        }
    }
    if let Some(collection_id) = filter.collection_id {
        let exists: i64 = conn
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM collection_cards WHERE card_id = ?1 AND collection_id = ?2)",
                rusqlite::params![card_id, collection_id],
                |row| row.get(0),
            )
            .map_err(|e| format!("подборка карточки: {e}"))?;
        if exists == 0 {
            return Ok(false);
        }
    }
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use crate::models::{CardDetail, CodeBlock, LineNote, SLOT_DOUBLE_BRACE};

    #[test]
    fn parse_splits_words_and_symbols() {
        let parsed = parse_query("lru_cache");
        assert_eq!(parsed.words, vec!["lru_cache"]);
        assert!(parsed.symbols.is_empty());

        let parsed = parse_query("arr[0]");
        assert_eq!(parsed.words, vec!["arr", "0"]);
        assert_eq!(parsed.symbols, vec!["[]"], "символы склеиваются в один кусок");

        let parsed = parse_query("x === y");
        assert_eq!(parsed.words, vec!["x", "y"]);
        assert_eq!(parsed.symbols, vec!["==="]);

        let empty = parse_query("   ");
        assert!(empty.words.is_empty() && empty.symbols.is_empty());
    }

    fn save(conn: &mut Connection, title: &str, tags: &[&str], code: &str, note: &str) -> i64 {
        // По умолчанию python: язык задаётся отдельно там, где он важен
        save_with_language(conn, title, tags, "python", code, note)
    }

    fn save_with_language(
        conn: &mut Connection,
        title: &str,
        tags: &[&str],
        language: &str,
        code: &str,
        note: &str,
    ) -> i64 {
        let detail = CardDetail {
            id: None,
            title: title.into(),
            description: String::new(),
            is_template: false,
            slot_syntax: SLOT_DOUBLE_BRACE.into(),
            favorite: false,
            created_at: String::new(),
            updated_at: String::new(),
            checked_version: String::new(),
            checked_at: String::new(),
            tags: tags.iter().map(|t| t.to_string()).collect(),
            blocks: vec![CodeBlock {
                id: None,
                language: language.into(),
                code: code.into(),
                sort_order: 0,
                notes: vec![LineNote {
                    id: None,
                    line_from: 1,
                    line_to: 1,
                    text: note.into(),
                    is_stale: false,
                    sort_order: 0,
                }],
                slots: vec![],
            }],
        };
        cards::card_save_impl(conn, &detail).unwrap()
    }

    fn seed() -> (Connection, i64) {
        let mut conn = db::open_in_memory().unwrap();
        let cache = save(
            &mut conn,
            "Кэш функций",
            &["python"],
            "@lru_cache(maxsize=None)\ndef f(x):\n    return x",
            "запоминает результат",
        );
        save(
            &mut conn,
            "Первый элемент списка",
            &["python", "array"],
            "first = arr[0]",
            "берём нулевой элемент",
        );
        save_with_language(
            &mut conn,
            "Строки SQL",
            &["sql"],
            "sql",
            "SELECT * FROM users;",
            "выборка",
        );
        (conn, cache)
    }

    /// Проверяет, что в индексе лежат все пять источников раздела 3.2.
    #[test]
    fn index_contains_all_sources() {
        let (conn, cache_id) = seed();
        let row = crate::db::collect_fts_row(&conn, cache_id).unwrap();
        assert!(row.title.contains("Кэш"), "название в индексе");
        assert!(row.tags.contains("python"), "теги в индексе");
        assert!(row.code.contains("lru_cache"), "код в индексе");
        assert!(row.notes.contains("запоминает"), "разбор в индексе");
        let n: i64 = conn
            .query_row("SELECT count(*) FROM card_search", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 3, "по строке на карточку");
    }

    #[test]
    fn finds_by_title_tag_code_and_note() {
        let (conn, _) = seed();
        // по названию
        let hits = search_impl(&conn, "Кэш", &Filter::default()).unwrap();
        assert_eq!(hits.len(), 1, "поиск по названию");
        // по тегу
        let hits = search_impl(&conn, "sql", &Filter::default()).unwrap();
        assert_eq!(hits.len(), 1, "поиск по тегу");
        assert_eq!(hits[0].title, "Строки SQL");
        // по коду
        let hits = search_impl(&conn, "lru_cache", &Filter::default()).unwrap();
        assert_eq!(hits.len(), 1, "поиск по коду");
        // по тексту разбора
        let hits = search_impl(&conn, "запоминает", &Filter::default()).unwrap();
        assert_eq!(hits.len(), 1, "поиск по разбору");
        assert!(hits[0].snippet.is_some(), "сниппет должен прийти");
    }

    /// Обязательная проверка этапа 4: запрос arr[0] находит карточку
    /// с такой строкой. Без нормализации пунктуации это не работает.
    #[test]
    fn arr_zero_bracket_query_is_found() {
        let (conn, _) = seed();
        let hits = search_impl(&conn, "arr[0]", &Filter::default()).unwrap();
        assert_eq!(hits.len(), 1, "запрос arr[0] обязан находить карточку");
        assert_eq!(hits[0].title, "Первый элемент списка");
        assert!(hits[0]
            .snippet
            .as_deref()
            .unwrap_or_default()
            .contains("arr[0]"));
    }

    #[test]
    fn symbol_only_query_searches_code() {
        let (conn, _) = seed();
        // Один символ: слова нет, идёт только проход по символам
        let hits = search_impl(&conn, "[", &Filter::default()).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].title, "Первый элемент списка");
    }

    #[test]
    fn results_have_no_duplicates() {
        let (conn, _) = seed();
        // Слово и символ встречаются в одной карточке
        let hits = search_impl(&conn, "arr[", &Filter::default()).unwrap();
        assert_eq!(hits.len(), 1, "одна карточка не должна дублироваться");
    }

    #[test]
    fn empty_query_returns_filtered_list() {
        let (conn, _) = seed();
        let hits = search_impl(&conn, "   ", &Filter::default()).unwrap();
        assert_eq!(hits.len(), 3, "пустой запрос даёт весь список");
    }

    #[test]
    fn no_matches_returns_empty_and_filters_apply() {
        let (conn, _) = seed();
        let hits = search_impl(&conn, "совершенно другое", &Filter::default()).unwrap();
        assert!(hits.is_empty());

        // Тег python стоит у двух карточек, третья его не содержит
        let filter = Filter::default();
        let hits = search_impl(&conn, "python", &filter).unwrap();
        assert_eq!(hits.len(), 2, "тег python есть у двух карточек");

        // Фильтр по избранному отсекает всё: избранных карточек в наборе нет
        let filter = Filter {
            favorite_only: true,
            ..Default::default()
        };
        let hits = search_impl(&conn, "python", &filter).unwrap();
        assert!(hits.is_empty(), "фильтр по избранному должен отсечь карточки");
    }

    #[test]
    fn fts_metacharacters_do_not_break_query() {
        let (conn, _) = seed();
        // Кавычка и звёздочка не должны ронять запрос
        assert!(search_impl(&conn, "\"unbalanced", &Filter::default()).is_ok());
        assert!(search_impl(&conn, "a*", &Filter::default()).is_ok());
    }
}
