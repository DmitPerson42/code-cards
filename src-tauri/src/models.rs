// Модели данных, которые ходят между Rust и фронтендом.
// Все структуры сериализуются в camelCase (раздел 4 плана).

use serde::{Deserialize, Serialize};

/// Режим синтаксиса слотов у карточки.
pub const SLOT_DOUBLE_BRACE: &str = "double_brace";
pub const SLOT_NONE: &str = "jinja_off";

/// Краткая карточка для сетки списка и результатов поиска.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CardSummary {
    pub id: i64,
    pub title: String,
    pub description: String,
    /// Язык первого блока: в сетке показываем один основной язык.
    pub language: String,
    pub tags: Vec<String>,
    /// Первые строки кода первого блока для превью.
    pub preview: String,
    pub favorite: bool,
    pub is_template: bool,
    pub created_at: String,
    pub updated_at: String,
    /// Фрагмент текста с совпадением из FTS, только в результатах поиска.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub snippet: Option<String>,
}

/// Один блок кода карточки.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CodeBlock {
    #[serde(default)]
    pub id: Option<i64>,
    pub language: String,
    pub code: String,
    #[serde(default)]
    pub sort_order: i64,
    #[serde(default)]
    pub notes: Vec<LineNote>,
    #[serde(default)]
    pub slots: Vec<Slot>,
}

/// Пояснение к диапазону строк блока.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LineNote {
    #[serde(default)]
    pub id: Option<i64>,
    pub line_from: i64,
    pub line_to: i64,
    pub text: String,
    #[serde(default)]
    pub is_stale: bool,
    #[serde(default)]
    pub sort_order: i64,
}

/// Слот шаблона.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Slot {
    #[serde(default)]
    pub id: Option<i64>,
    pub name: String,
    #[serde(default)]
    pub default_value: String,
    /// Варианты значений для выпадающего списка.
    #[serde(default)]
    pub variants: Vec<String>,
}

/// Полная карточка для экрана просмотра и редактора.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CardDetail {
    #[serde(default)]
    pub id: Option<i64>,
    pub title: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub is_template: bool,
    #[serde(default = "default_slot_syntax")]
    pub slot_syntax: String,
    #[serde(default)]
    pub favorite: bool,
    #[serde(default)]
    pub created_at: String,
    #[serde(default)]
    pub updated_at: String,
    #[serde(default)]
    pub checked_version: String,
    #[serde(default)]
    pub checked_at: String,
    #[serde(default)]
    pub tags: Vec<String>,
    pub blocks: Vec<CodeBlock>,
}

fn default_slot_syntax() -> String {
    SLOT_DOUBLE_BRACE.to_string()
}

/// Сводка переноса разбора: сколько диапазонов сдвинулось, изменилось и потерялось.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct MovedSummary {
    pub moved: i64,
    pub changed: i64,
    pub lost: i64,
}

/// Результат сохранения карточки.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "status", rename_all = "camelCase")]
pub enum SaveResult {
    /// Сохранено.
    Ok { id: i64 },
    /// Нужно подтверждение: разбор изменится, фронтенд показывает сводку.
    NeedsConfirmation { summary: MovedSummary },
}

/// Фильтр списка и поиска.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Filter {
    /// Теги: все указанные должны присутствовать (И).
    #[serde(default)]
    pub tags: Vec<String>,
    /// Язык кода.
    #[serde(default)]
    pub language: Option<String>,
    /// Подборка.
    #[serde(default)]
    pub collection_id: Option<i64>,
    /// Только избранные.
    #[serde(default)]
    pub favorite_only: bool,
    /// Сортировка: recent | alpha | created.
    #[serde(default)]
    pub sort: Option<String>,
}

/// Тег с числом карточек.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TagWithCount {
    pub id: i64,
    pub name: String,
    pub color: String,
    pub count: i64,
}

/// Узел дерева подборок.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CollectionNode {
    pub id: i64,
    pub name: String,
    pub parent_id: Option<i64>,
    pub sort_order: i64,
    pub card_count: i64,
    #[serde(default)]
    pub children: Vec<CollectionNode>,
}

/// Статистика экспорта архива.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Stats {
    pub cards: i64,
    pub blocks: i64,
    pub notes: i64,
    pub slots: i64,
    pub tags: i64,
    pub collections: i64,
    /// Термины словаря.
    #[serde(default)]
    pub terms: i64,
}

/// Отчёт об импорте архива.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ImportReport {
    pub cards: i64,
    pub blocks: i64,
    pub notes: i64,
    pub slots: i64,
    pub tags: i64,
    pub collections: i64,
    /// Карточки, которые пришлось создать заново из-за совпадения id.
    pub recreated: i64,
    /// Термины словаря: новые и обновлённые.
    #[serde(default)]
    pub terms: i64,
}

/// Термин словаря: короткая запись и её значение.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Term {
    #[serde(default)]
    pub id: Option<i64>,
    pub term: String,
    #[serde(default)]
    pub meaning: String,
    #[serde(default)]
    pub created_at: String,
    #[serde(default)]
    pub updated_at: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn save_result_serializes_with_status_tag() {
        let ok = serde_json::to_string(&SaveResult::Ok { id: 7 }).unwrap();
        assert!(ok.contains("\"status\":\"ok\""), "получено: {ok}");
        assert!(ok.contains("\"id\":7"));

        let confirm = serde_json::to_string(&SaveResult::NeedsConfirmation {
            summary: MovedSummary {
                moved: 4,
                changed: 2,
                lost: 1,
            },
        })
        .unwrap();
        assert!(confirm.contains("\"status\":\"needsConfirmation\""));
        assert!(confirm.contains("\"moved\":4"));
        assert!(confirm.contains("\"lost\":1"));
    }

    #[test]
    fn card_detail_fields_are_camel_case() {
        let detail = CardDetail {
            id: None,
            title: "Тест".into(),
            description: String::new(),
            is_template: false,
            slot_syntax: default_slot_syntax(),
            favorite: false,
            created_at: String::new(),
            updated_at: String::new(),
            checked_version: String::new(),
            checked_at: String::new(),
            tags: vec![],
            blocks: vec![],
        };
        let json = serde_json::to_string(&detail).unwrap();
        assert!(json.contains("\"isTemplate\""));
        assert!(json.contains("\"slotSyntax\""));
    }
}
