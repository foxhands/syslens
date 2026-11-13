//! What changed since a saved snapshot: programs, drivers, devices, hardware.
//! Both states are turned into pages, and the pages are compared row by row.

use crate::info::{self, Page, Section};
use serde_json::Value;

/// Rows that change by themselves and say nothing about the computer.
const VOLATILE: &[&str] = &[
    "Дата и время",
    "Время работы",
    "Последняя загрузка",
    "Текущая частота",
    "Занято",
    "Свободно",
    "Свободно %",
    "Загрузка",
    "Предупреждение",
    "Заряд",
    "Питание",
    "Осталось времени",
    "Температура",
    "Наработка",
    "Подробности",
];

/// Sections made only of such rows.
const VOLATILE_SECTIONS: &[&str] = &["Разделы"];

/// "Установленные программы (63)" → "Установленные программы": the count is not part of the name.
fn plain_title(t: &str) -> String {
    match t.rfind(" (") {
        Some(i) if t.ends_with(')') && t[i + 2..t.len() - 1].chars().all(|c| c.is_ascii_digit()) => t[..i].to_string(),
        _ => t.to_string(),
    }
}

/// (section, row name, value) for every row worth comparing.
fn flat(id: &str, d: &Value) -> Vec<(String, String, String)> {
    let mut out = Vec::new();
    for s in info::page(id, d).sections {
        let title = plain_title(&s.title);
        if VOLATILE_SECTIONS.contains(&title.as_str()) {
            continue;
        }
        for (k, v) in s.rows {
            if !VOLATILE.contains(&k.as_str()) {
                out.push((title.clone(), k, v));
            }
        }
    }
    out
}

/// A page of differences: one section per changed place, titled
/// "page  ›  section"; rows say what was added, removed or changed.
pub fn compare(pages: &[(&str, &str)], old: &Value, new: &Value) -> Page {
    let mut result = Page { sections: Vec::new() };
    for (id, page_title) in pages {
        let mut gone = flat(id, old);
        let mut came = flat(id, new);
        // rows present in both states are not changes
        gone.retain(|row| match came.iter().position(|x| x == row) {
            Some(i) => {
                came.remove(i);
                false
            }
            None => true,
        });
        let mut rows: Vec<(String, String, String)> = Vec::new(); // section, name, text
        for (sec, k, was) in gone {
            match came.iter().position(|x| x.0 == sec && x.1 == k) {
                Some(i) => {
                    let now = came.remove(i).2;
                    rows.push((sec, k, format!("было: {was}  →  стало: {now}")));
                }
                None => rows.push((sec, k, format!("удалено: {was}"))),
            }
        }
        for (sec, k, now) in came {
            rows.push((sec, k, format!("добавлено: {now}")));
        }
        for (sec, k, text) in rows {
            let title = if sec == *page_title { sec } else { format!("{page_title}  ›  {sec}") };
            match result.sections.iter_mut().find(|s| s.title == title) {
                Some(s) => s.rows.push((k, text)),
                None => result.sections.push(Section { title, rows: vec![(k, text)] }),
            }
        }
    }
    result
}

/// When the snapshot was taken, as the system wrote it ("14.11.2026 / 00:17").
pub fn taken(snapshot: &Value) -> String {
    snapshot.pointer("/os/Now").and_then(|v| v.as_str()).unwrap_or_default().replace(" / ", " ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn finds_added_removed_and_changed() {
        let old = json!({
            "os": { "Now": "01.01.2026 / 10:00", "UptimeSec": 10 },
            "apps": [
                { "Name": "Alpha", "Version": "1.0" },
                { "Name": "Beta", "Version": "2.0" },
            ],
            "usb": [{ "Name": "Mouse", "Manufacturer": "Logi" }],
        });
        let new = json!({
            "os": { "Now": "05.01.2026 / 12:00", "UptimeSec": 99999 },
            "apps": [
                { "Name": "Alpha", "Version": "1.1" },
                { "Name": "Gamma", "Version": "3.0" },
            ],
            "usb": [{ "Name": "Mouse", "Manufacturer": "Logi" }],
        });
        let pages = [("os", "Операционная система"), ("apps", "Установленные программы"), ("usb", "Устройства USB")];
        let p = compare(&pages, &old, &new);
        // time and uptime differ, but they are not changes; USB is the same
        assert_eq!(p.sections.len(), 1);
        let s = &p.sections[0];
        assert_eq!(s.title, "Установленные программы"); // the count "(2)" is not part of the name
        assert!(s.rows.contains(&("Alpha".into(), "было: версия 1.0  →  стало: версия 1.1".into())));
        assert!(s.rows.contains(&("Beta".into(), "удалено: версия 2.0".into())));
        assert!(s.rows.contains(&("Gamma".into(), "добавлено: версия 3.0".into())));
        assert_eq!(taken(&old), "01.01.2026 10:00");
    }

    #[test]
    fn same_state_has_no_changes() {
        let d = json!({ "cpu": [{ "Name": "X", "NumberOfCores": 4 }], "apps": [{ "Name": "A" }] });
        assert!(compare(&[("cpu", "ЦП"), ("apps", "Установленные программы")], &d, &d).sections.is_empty());
    }
}
