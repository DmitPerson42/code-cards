// Слоты шаблона (этап 6): разбор {{имя}}, подстановка значений и
// превращение выделенного фрагмента в слот ("сделать чертёж из выделенного").
//
// Режим карточки управляет всем: при slot_syntax = jinja_off слоты не
// распознаются вообще, даже если в коде есть {{ }}.

use rusqlite::{params, Connection};

use crate::models::SLOT_NONE;

/// Найденный в коде слот.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FoundSlot {
    pub name: String,
    /// Смещение в символах от начала кода.
    pub start: i64,
    /// Смещение конца (не включая закрывающие скобки).
    pub end: i64,
    /// Номер строки, 1-based: для подсветки в редакторе.
    pub line: i64,
}

/// Находит все слоты {{имя}} в коде.
///
/// Имя слота: буквы, цифры, подчёркивание и дефис. Пустые {{}} и скобки
/// с пробелами внутри игнорируются, чтобы не ломать обычный код.
pub fn find_slots(code: &str, slot_syntax: &str) -> Vec<FoundSlot> {
    if slot_syntax == SLOT_NONE {
        return Vec::new();
    }

    let chars: Vec<char> = code.chars().collect();
    let mut result = Vec::new();
    let mut index = 0;
    let mut line = 1i64;

    while index < chars.len() {
        if chars[index] == '\n' {
            line += 1;
            index += 1;
            continue;
        }
        // Ищем открывающие {{
        if chars[index] == '{' && index + 1 < chars.len() && chars[index + 1] == '{' {
            let name_start = index + 2;
            if let Some(close) = find_close(&chars, name_start) {
                let name: String = chars[name_start..close].iter().collect();
                let name = name.trim();
                if is_valid_slot_name(name) {
                    result.push(FoundSlot {
                        name: name.to_string(),
                        start: index as i64,
                        end: close as i64,
                        line,
                    });
                    index = close + 2;
                    continue;
                }
            }
        }
        index += 1;
    }
    result
}

/// Позиция закрывающих }} после имени слота.
fn find_close(chars: &[char], from: usize) -> Option<usize> {
    let mut index = from;
    while index < chars.len() {
        // Внутри одного плейсхолдера перевод строки недопустим
        if chars[index] == '\n' {
            return None;
        }
        if chars[index] == '}' && index + 1 < chars.len() && chars[index + 1] == '}' {
            return Some(index);
        }
        index += 1;
    }
    None
}

fn is_valid_slot_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_alphanumeric() || c == '_' || c == '-')
}

/// Уникальные имена слотов в порядке появления.
pub fn slot_names(code: &str, slot_syntax: &str) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    for slot in find_slots(code, slot_syntax) {
        if !names.contains(&slot.name) {
            names.push(slot.name);
        }
    }
    names
}

/// Подставляет значения вместо {{имя}}.
///
/// Незаполненные слоты берут значение по умолчанию; если его нет,
/// подставляется пустая строка. Неизвестные имена в values игнорируются.
pub fn render_code(
    code: &str,
    slot_syntax: &str,
    defaults: &std::collections::HashMap<String, String>,
    values: &std::collections::HashMap<String, String>,
) -> String {
    let slots = find_slots(code, slot_syntax);
    if slots.is_empty() {
        return code.to_string();
    }
    let chars: Vec<char> = code.chars().collect();
    let mut out = String::new();
    let mut cursor = 0usize;
    for slot in slots {
        let start = slot.start as usize;
        let end_inclusive = slot.end as usize + 1; // вторая закрывающая скобка
        out.extend(chars[cursor..start].iter());
        let value = values
            .get(&slot.name)
            .filter(|v| !v.is_empty())
            .or_else(|| defaults.get(&slot.name))
            .cloned()
            .unwrap_or_default();
        out.push_str(&value);
        cursor = end_inclusive + 1;
    }
    if cursor < chars.len() {
        out.extend(chars[cursor..].iter());
    }
    out
}

/// Превращает выделенный текст в слот: заменяет его на {{имя}}.
///
/// Возвращает новый код и значение по умолчанию (сам выделенный текст).
/// Смещения заданы в символах строки.
pub fn make_slot(
    code: &str,
    line: usize,
    range_start: usize,
    range_end: usize,
    name: &str,
) -> Result<(String, String), String> {
    let name = name.trim();
    if !is_valid_slot_name(name) {
        return Err(
            "имя слота может содержать только буквы, цифры, подчёркивание и дефис".to_string(),
        );
    }

    let lines: Vec<&str> = code.split('\n').collect();
    if line == 0 || line > lines.len() {
        return Err(format!("строка {line} вне кода"));
    }
    // Собираем смещение начала строки в символах
    let mut line_start = 0usize;
    for (index, text) in lines.iter().enumerate() {
        if index + 1 == line {
            break;
        }
        line_start += text.chars().count() + 1;
    }
    let line_len = lines[line - 1].chars().count();
    let start = range_start.min(line_len);
    let end = range_end.min(line_len);
    if end <= start {
        return Err("выделенный фрагмент пуст".to_string());
    }

    let chars: Vec<char> = code.chars().collect();
    let selected: String = chars[line_start + start..line_start + end].iter().collect();

    let mut out = String::new();
    out.extend(chars[..line_start + start].iter());
    out.push_str("{{");
    out.push_str(name);
    out.push_str("}}");
    out.extend(chars[line_start + end..].iter());

    Ok((out, selected))
}

/// Записывает список слотов блока: имена, значения по умолчанию, варианты.
pub fn slots_set_impl(
    conn: &Connection,
    block_id: i64,
    slots: &[crate::models::Slot],
) -> Result<(), String> {
    conn.execute("DELETE FROM slots WHERE block_id = ?1", [block_id])
        .map_err(|e| format!("очистка слотов: {e}"))?;
    for slot in slots {
        let variants = serde_json::to_string(&slot.variants).unwrap_or_else(|_| "[]".to_string());
        conn.execute(
            "INSERT INTO slots (block_id, name, default_value, variants)
             VALUES (?1, ?2, ?3, ?4)",
            params![block_id, slot.name, slot.default_value, variants],
        )
        .map_err(|e| format!("сохранение слота: {e}"))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::SLOT_DOUBLE_BRACE;
    use std::collections::HashMap;

    #[test]
    fn finds_two_slots_with_positions() {
        let code = "def {{name}}(x):\n    return {{body}}";
        let slots = find_slots(code, SLOT_DOUBLE_BRACE);
        assert_eq!(slots.len(), 2);
        assert_eq!(slots[0].name, "name");
        assert_eq!(slots[0].line, 1);
        assert_eq!(slots[1].name, "body");
        assert_eq!(slots[1].line, 2);
    }

    #[test]
    fn jinja_off_disables_slots_completely() {
        let code = "{{a}} and {{b}}";
        assert!(!find_slots(code, SLOT_DOUBLE_BRACE).is_empty());
        assert!(find_slots(code, SLOT_NONE).is_empty(), "режим не-слоты отключает всё");
        let defaults = HashMap::new();
        let values = HashMap::new();
        assert_eq!(render_code(code, SLOT_NONE, &defaults, &values), code);
    }

    #[test]
    fn repeated_slot_name_is_replaced_everywhere() {
        let code = "a = {{v}}\nb = {{v}}\nc = {{w}}";
        let mut defaults = HashMap::new();
        defaults.insert("v".to_string(), "1".to_string());
        defaults.insert("w".to_string(), "2".to_string());
        let values = HashMap::new();
        assert_eq!(
            render_code(code, SLOT_DOUBLE_BRACE, &defaults, &values),
            "a = 1\nb = 1\nc = 2"
        );
    }

    #[test]
    fn values_override_defaults_and_empty_falls_back() {
        let code = "{{a}}-{{b}}";
        let mut defaults = HashMap::new();
        defaults.insert("a".to_string(), "по умолчанию".to_string());
        defaults.insert("b".to_string(), "тоже".to_string());
        let mut values = HashMap::new();
        values.insert("a".to_string(), "задано".to_string());
        values.insert("b".to_string(), String::new());
        assert_eq!(
            render_code(code, SLOT_DOUBLE_BRACE, &defaults, &values),
            "задано-тоже",
            "пустое значение откатывается к умолчанию"
        );
    }

    #[test]
    fn make_slot_replaces_selection_and_returns_default() {
        let code = "x = 42\nprint(x)";
        let (new_code, default) = make_slot(code, 1, 4, 6, "value").unwrap();
        assert_eq!(new_code, "x = {{value}}\nprint(x)");
        assert_eq!(default, "42", "исходный текст становится значением по умолчанию");
    }

    #[test]
    fn invalid_inputs_are_rejected() {
        let code = "abc";
        assert!(make_slot(code, 1, 2, 2, "name").is_err(), "пустое выделение");
        assert!(make_slot(code, 5, 0, 1, "name").is_err(), "строки нет");
        assert!(make_slot(code, 1, 0, 1, "плохое имя!").is_err(), "недопустимое имя");
    }

    #[test]
    fn malformed_placeholders_are_ignored() {
        let code = "{{}} {{ }} {single} {{ok-name}}";
        let names = slot_names(code, SLOT_DOUBLE_BRACE);
        assert_eq!(names, vec!["ok-name".to_string()]);
    }
}
