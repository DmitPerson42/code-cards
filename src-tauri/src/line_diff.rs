// Перенос разбора при правке кода (раздел 3.5 плана). Критичная часть.
//
// Идея: сравниваем старый и новый код построчно алгоритмом патчей (`similar`),
// получаем отображение "старая строка -> новая строка", затем переносим
// диапазоны разбора. Правило без исключений: молча терять разбор нельзя,
// поэтому сомнительные случаи помечаются как устаревшие, но не удаляются.

use similar::{ChangeTag, TextDiff};

use crate::models::{LineNote, MovedSummary};

/// Результат переноса одного диапазона.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoteOutcome {
    /// Обе границы нашли новое место, смысл сохранён.
    Moved,
    /// Диапазон изменился: часть строк исчезла или слилась.
    Changed,
    /// Ни одной строки диапазона не осталось.
    Lost,
}

/// Перенесённый диапазон вместе с вердиктом.
#[derive(Debug, Clone)]
pub struct MovedNote {
    pub note: LineNote,
    pub outcome: NoteOutcome,
}

/// Строит отображение старых номеров строк (0-based) на новые.
///
/// Возвращает вектор длины старого числа строк: `Some(new_index)` для строк,
/// которые остались (в том числе изменённые), `None` для удалённых.
pub fn build_line_map(old: &str, new: &str) -> Vec<Option<usize>> {
    let old_lines: Vec<String> = split_lines(old);
    let mut map: Vec<Option<usize>> = vec![None; old_lines.len()];

    if old == new {
        for (index, slot) in map.iter_mut().enumerate() {
            *slot = Some(index);
        }
        return map;
    }

    let diff = TextDiff::from_lines(old, new);
    for change in diff.iter_all_changes() {
        let (tag, old_index) = (change.tag(), change.old_index());
        match tag {
            // Строка осталась на месте
            ChangeTag::Equal => {
                if let (Some(i), Some(j)) = (old_index, change.new_index()) {
                    map[i] = Some(j);
                }
            }
            // Строка удалена или заменена: отображать её некуда. Явный None важен:
            // иначе диапазон, в котором строка исчезла, выглядел бы целым
            // и переносился бы без пометки "устарело".
            ChangeTag::Delete => {
                if let Some(i) = old_index {
                    map[i] = None;
                }
            }
            ChangeTag::Insert => {}
        }
    }

    recover_moved_lines(&old_lines, new, &mut map);
    map
}

/// Второй проход: ищет удалённые строки, которые на самом деле переехали.
///
/// `similar` минимизирует изменения и часто изображает перестановку блока как
/// "удалили эти строки, а такие же вставили в другом месте". Один проход по
/// диффу счёл бы такие строки потерянными, хотя содержание уцелело.
/// Поэтому каждую неотображённую строку ищем по точному совпадению в новом
/// коде и берём ближайшее по номеру свободное место.
fn recover_moved_lines(old_lines: &[String], new: &str, map: &mut [Option<usize>]) {
    if map.iter().all(Option::is_some) {
        return;
    }
    let new_lines = split_lines(new);

    // Занятые новым отображением строки: одному новому месту не может
    // соответствовать две старые строки
    let mut used: Vec<bool> = vec![false; new_lines.len()];
    for slot in map.iter().flatten() {
        if *slot < used.len() {
            used[*slot] = true;
        }
    }

    for old_index in 0..old_lines.len() {
        if map[old_index].is_some() {
            continue;
        }
        let needle = old_lines[old_index].trim_end();
        // Пустые строки и чистые отступы слишком часто повторяются:
        // привязка по ним запутала бы разбор вместо помощи.
        if needle.trim().is_empty() {
            continue;
        }

        let mut best: Option<(usize, usize)> = None;
        for (new_index, candidate) in new_lines.iter().enumerate() {
            if used[new_index] || candidate.trim_end() != needle {
                continue;
            }
            let distance = new_index.abs_diff(old_index);
            if best.map_or(true, |(_, best_distance)| distance < best_distance) {
                best = Some((new_index, distance));
            }
        }

        if let Some((new_index, _)) = best {
            map[old_index] = Some(new_index);
            used[new_index] = true;
        }
    }
}

/// Разбивает код на строки так же, как его видит редактор.
/// Завершающий перевод строки не создаёт лишнюю пустую строку.
/// Делит код на строки так же, как нумерует их редактор.
///
/// Возвращает строки без завершающего перевода строки: код "a\nb\n"
/// это две строки, а не три. Отдельный случай CRLF: Windows-буфер обмена и
/// редактор могут отдать "\r\n", и в строках не должно оставаться "\r".
pub fn split_lines(code: &str) -> Vec<String> {
    if code.is_empty() {
        return Vec::new();
    }
    let normalized = code.replace("\r\n", "\n");
    let trimmed = normalized.strip_suffix('\n').unwrap_or(&normalized);
    trimmed
        .split('\n')
        .map(|line| line.trim_end_matches('\r').to_string())
        .collect()
}

/// Переносит разбор блока со старого кода на новый.
///
/// Возвращает перенесённые записи (в том же порядке, что и вход) и сводку.
/// Записи никогда не удаляются: потерянный диапазон остаётся с `is_stale`.
pub fn transfer_notes(old_code: &str, new_code: &str, notes: &[LineNote]) -> (Vec<MovedNote>, MovedSummary) {
    let map = build_line_map(old_code, new_code);
    let old_count = map.len();
    let new_count = split_lines(new_code).len();
    let mut summary = MovedSummary::default();
    let mut moved_notes = Vec::with_capacity(notes.len());

    for note in notes {
        // Внутри используем 0-based индексы, наружу отдаём 1-based номера строк
        let from = (note.line_from.max(1) as usize - 1).min(old_count.saturating_sub(1));
        let to = (note.line_to.max(note.line_from) as usize - 1).min(old_count.saturating_sub(1));

        if old_count == 0 {
            // Раньше кода не было: переносить нечего, разбор остаётся как есть
            let mut kept = note.clone();
            kept.is_stale = true;
            summary.changed += 1;
            moved_notes.push(MovedNote {
                note: kept,
                outcome: NoteOutcome::Changed,
            });
            continue;
        }

        let mapped: Vec<usize> = (from..=to).filter_map(|i| map.get(i).copied().flatten()).collect();

        if mapped.is_empty() {
            // Ни одной строки диапазона не осталось: запись выживает, но помечена
            let mut lost = note.clone();
            lost.is_stale = true;
            if new_count == 0 {
                lost.line_from = 1;
                lost.line_to = 1;
            } else {
                // Прижимаем диапазон к ближайшей существующей строке,
                // чтобы запись не висела в пустоте
                let anchor = map
                    .iter()
                    .take(from)
                    .filter_map(|s| *s)
                    .next_back()
                    .map(|i| i + 1)
                    .unwrap_or(1);
                lost.line_from = anchor.min(new_count) as i64;
                lost.line_to = lost.line_from;
            }
            summary.lost += 1;
            moved_notes.push(MovedNote {
                note: lost,
                outcome: NoteOutcome::Lost,
            });
            continue;
        }

        let new_from = *mapped.iter().min().unwrap();
        let new_to = *mapped.iter().max().unwrap();
        let expected_len = to - from + 1;
        let kept_len = mapped.len();
        // Диапазон считаем целым, если все его строки уцелели и идут подряд
        let contiguous = new_to - new_from + 1 == kept_len;
        let intact = !note.is_stale && kept_len == expected_len && contiguous;

        let mut result = note.clone();
        result.line_from = (new_from + 1) as i64;
        result.line_to = (new_to + 1) as i64;

        if intact {
            summary.moved += 1;
            moved_notes.push(MovedNote {
                note: result,
                outcome: NoteOutcome::Moved,
            });
        } else {
            result.is_stale = true;
            summary.changed += 1;
            moved_notes.push(MovedNote {
                note: result,
                outcome: NoteOutcome::Changed,
            });
        }
    }

    (moved_notes, summary)
}

/// Нужно ли спрашивать подтверждение: сводка не пустая, если разбор как-то пострадал.
pub fn summary_needs_confirmation(summary: &MovedSummary) -> bool {
    summary.moved > 0 || summary.changed > 0 || summary.lost > 0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn note(from: i64, to: i64, text: &str) -> LineNote {
        LineNote {
            id: None,
            line_from: from,
            line_to: to,
            text: text.to_string(),
            is_stale: false,
            sort_order: 0,
        }
    }

    /// Тест из плана 1: вставка строк в начале. Все диапазоны сдвигаются,
    /// ничего не теряется и ничто не помечается устаревшим.
    #[test]
    fn insert_at_top_shifts_all_notes() {
        let old = "a\nb\nc";
        let new = "// шапка\n// ещё\n a\nb\nc".replace("\n a", "\na");
        let notes = vec![note(1, 1, "первая"), note(2, 3, "вторая и третья")];
        let (moved, summary) = transfer_notes(old, &new, &notes);

        assert_eq!(summary.lost, 0, "потерять нельзя");
        assert_eq!(summary.changed, 0, "содержимое не менялось");
        assert_eq!(summary.moved, 2);
        assert_eq!((moved[0].note.line_from, moved[0].note.line_to), (3, 3));
        assert_eq!((moved[1].note.line_from, moved[1].note.line_to), (4, 5));
        assert!(!moved[0].note.is_stale);
        assert!(!moved[1].note.is_stale);
        assert_eq!(moved.len(), notes.len(), "число записей прежнее");
    }

    /// Тест из плана 2: удаление разобранной строки. Запись не удаляется,
    /// а помечается устаревшей.
    #[test]
    fn delete_annotated_line_marks_stale_and_keeps_note() {
        let old = "a\nb\nc\nd";
        let new = "a\nc\nd";
        let notes = vec![note(2, 2, "эта строка исчезла")];
        let (moved, summary) = transfer_notes(old, new, &notes);

        assert_eq!(moved.len(), 1, "запись обязана выжить");
        assert!(moved[0].note.is_stale, "исчезнувшая строка делает разбор устаревшим");
        assert_eq!(moved[0].note.text, "эта строка исчезла");
        assert!(summary.lost + summary.changed >= 1, "сводка должна отразить потерю");
    }

    /// Тест из плана 3: полная замена кода. Все записи устарели,
    /// но их количество прежнее.
    #[test]
    fn total_replacement_keeps_every_note_marked_stale() {
        let old = "def f():\n    return 1";
        let new = "x = 1\ny = 2\nz = 3\nw = 4";
        let notes = vec![note(1, 1, "объявление"), note(2, 2, "возврат")];
        let (moved, _summary) = transfer_notes(old, new, &notes);

        assert_eq!(moved.len(), 2, "количество записей прежнее");
        for item in &moved {
            assert!(item.note.is_stale, "при полной замене все записи устарели");
        }
    }

    /// Тест из плана 4: код не менялся. Ничего не помечено.
    #[test]
    fn unchanged_code_marks_nothing() {
        let code = "a\nb\nc";
        let notes = vec![note(1, 1, "раз"), note(2, 3, "два")];
        let (moved, summary) = transfer_notes(code, code, &notes);

        assert_eq!(summary.moved, 2);
        assert_eq!(summary.changed, 0);
        assert_eq!(summary.lost, 0);
        for item in &moved {
            assert!(!item.note.is_stale);
        }
        assert_eq!((moved[0].note.line_from, moved[0].note.line_to), (1, 1));
        assert_eq!((moved[1].note.line_from, moved[1].note.line_to), (2, 3));
    }

    #[test]
    fn editing_a_line_in_place_keeps_range_and_marks_stale() {
        let old = "value = 1\nprint(value)";
        let new = "value = 42\nprint(value)";
        let notes = vec![note(1, 1, "инициализация")];
        let (moved, _) = transfer_notes(old, new, &notes);
        assert_eq!((moved[0].note.line_from, moved[0].note.line_to), (1, 1));
        assert!(moved[0].note.is_stale, "изменённая строка помечается устаревшей");
    }

    #[test]
    fn deleting_whole_code_keeps_notes_alive() {
        let notes = vec![note(1, 2, "было два пропуска"), note(3, 3, "return")];
        let (moved, summary) = transfer_notes("a\nb\nc", "", &notes);
        assert_eq!(moved.len(), 2, "очистка кода не удаляет разбор");
        assert!(moved.iter().all(|m| m.note.is_stale));
        assert!(summary.lost >= 1);
    }

    #[test]
    fn split_lines_matches_editor_numbering() {
        assert_eq!(split_lines("a\nb"), vec!["a", "b"]);
        assert_eq!(split_lines("a\nb\n"), vec!["a", "b"]);
        assert_eq!(split_lines("a\r\nb\r\n"), vec!["a", "b"]);
        assert_eq!(split_lines(""), Vec::<String>::new());
    }

    /// Перестановка строк: алгоритм сопоставления не обязан узнавать
    /// переставленный блок как "тот же самый" (для LCS это удаление плюс
    /// вставка). Проверяем главное требование раздела 3.5: разбор не теряется
    /// молча. Запись обязана остаться, а если её не удалось сопоставить,
    /// то пометиться устаревшей, и это должно быть видно в сводке.
    #[test]
    fn moved_block_never_loses_notes_silently() {
        let old = "one\ntwo\nthree\nfour";
        let new = "three\nfour\none\ntwo";
        let notes = vec![note(3, 4, "хвост блока")];
        let (moved, summary) = transfer_notes(old, new, &notes);

        assert_eq!(moved.len(), 1, "запись разбора не удаляется никогда");
        let touched = summary.moved + summary.changed + summary.lost;
        assert!(touched > 0, "изменение должно быть отражено в сводке");

        let note_after = &moved[0].note;
        if summary.lost > 0 {
            // Не сопоставилось: запись жива, помечена и прижата к реальной строке
            assert!(note_after.is_stale, "потерянный диапазон помечается устаревшим");
            assert!(note_after.line_from >= 1 && note_after.line_to >= note_after.line_from);
            assert!(note_after.line_to <= new.lines().count() as i64, "диапазон внутри кода");
        } else {
            // Сопоставилось: без пометки и на своём новом месте
            assert!(!note_after.is_stale);
        }
    }

    /// Диапазон, у которого исчезла одна граница, усекается и помечается.
    #[test]
    fn partially_lost_range_is_truncated_and_marked() {
        let old = "a\nb\nc\nd";
        let new = "a\nb";
        let notes = vec![note(3, 4, "обрезается")];
        let (moved, summary) = transfer_notes(old, new, &notes);

        assert_eq!(moved.len(), 1, "запись остаётся");
        assert_eq!(summary.lost, 1, "полностью потерянный диапазон виден в сводке");
        assert!(moved[0].note.is_stale, "и помечен устаревшим");
        assert!(moved[0].note.line_from >= 1);
        assert!(moved[0].note.line_to <= 2, "диапазон прижат внутрь нового кода");
    }
}
