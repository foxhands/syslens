//! Two computers side by side: this one and another whose JSON report was
//! opened. Each is reduced to plain facts — (section, name, value) per page,
//! always written the same way whatever the language or units — and the facts
//! are compared row by row.

use crate::info::{self, Page, Section};
use crate::{privacy, rules};
use serde_json::{json, Value};

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
pub fn plain_title(t: &str) -> String {
    match t.rfind(" (") {
        Some(i) if t.ends_with(')') && t[i + 2..t.len() - 1].chars().all(|c| c.is_ascii_digit()) => t[..i].to_string(),
        _ => t.to_string(),
    }
}

/// One fact: section, row name, value.
pub type Fact = (String, String, String);

/// The facts of one page worth comparing. Rows that identify the computer or
/// its owner are left out: they always differ and are nobody else's business.
pub fn facts(id: &str, d: &Value, user: &privacy::Names) -> Vec<Fact> {
    let mut out = Vec::new();
    let mut page = rules::with_binary(|| info::page(id, d));
    privacy::hide_lan(id, &mut page);
    for s in &mut page.sections {
        s.title = privacy::hide_user(&s.title, user);
        let title = plain_title(&s.title);
        if VOLATILE_SECTIONS.contains(&title.as_str()) {
            continue;
        }
        for (k, v) in &s.rows {
            if !VOLATILE.contains(&k.as_str()) && !privacy::PERSONAL.contains(&k.as_str()) {
                out.push((title.clone(), privacy::hide_user(k, user), privacy::hide_user(v, user)));
            }
        }
    }
    out
}

/// The part of the JSON report another SysLens compares with.
pub fn to_json(pages: &[(&str, &str)], d: &Value, user: &privacy::Names) -> Value {
    let pages: Vec<Value> = pages
        .iter()
        .map(|(id, _)| {
            let rows: Vec<Value> = facts(id, d, user).into_iter().map(|(s, k, v)| json!([s, k, v])).collect();
            json!({ "id": id, "rows": rows })
        })
        .collect();
    json!({ "made": made(d), "pages": pages })
}

/// When the facts were gathered, as the system wrote it ("14.11.2026 00:17").
fn made(d: &Value) -> String {
    d.pointer("/os/Now").and_then(|v| v.as_str()).unwrap_or_default().replace(" / ", " ")
}

/// Another computer, read from its report.
#[derive(Debug)]
pub struct Other {
    pub made: String,
    pages: Vec<(String, Vec<Fact>)>,
}

/// Reads the comparison part of a JSON report.
pub fn parse(report: &Value) -> Result<Other, String> {
    if report.get("program").and_then(|v| v.as_str()) != Some("SysLens") {
        return Err("Это не отчёт SysLens в формате JSON".into());
    }
    let Some(list) = report.pointer("/compare/pages").and_then(|v| v.as_array()) else {
        return Err("Этот отчёт сделан старой версией SysLens — сохраните его заново в новой".into());
    };
    let text = |v: Option<&Value>| v.and_then(|x| x.as_str()).unwrap_or_default().to_string();
    let pages = list
        .iter()
        .map(|p| {
            let rows = p
                .get("rows")
                .and_then(|r| r.as_array())
                .map(|rows| rows.iter().map(|r| (text(r.get(0)), text(r.get(1)), text(r.get(2)))).collect())
                .unwrap_or_default();
            (text(p.get("id")), rows)
        })
        .collect();
    Ok(Other { made: text(report.pointer("/compare/made")), pages })
}

/// A page of differences: one section per place that differs, titled
/// "page  ›  section"; rows say what only one computer has and what differs.
pub fn compare(pages: &[(&str, &str)], other: &Other, this: &Value, user: &privacy::Names) -> Page {
    let mut result = Page { sections: Vec::new() };
    for (id, page_title) in pages {
        let mut theirs: Vec<Fact> = other.pages.iter().find(|p| p.0 == *id).map(|p| p.1.clone()).unwrap_or_default();
        let mut ours = facts(id, this, user);
        // rows both computers have are not differences
        theirs.retain(|row| match ours.iter().position(|x| x == row) {
            Some(i) => {
                ours.remove(i);
                false
            }
            None => true,
        });
        let mut rows: Vec<Fact> = Vec::new(); // section, name, text
        for (sec, k, a) in theirs {
            match ours.iter().position(|x| x.0 == sec && x.1 == k) {
                Some(i) => {
                    let b = ours.remove(i).2;
                    rows.push((sec, k, format!("другой: {a}  →  этот: {b}")));
                }
                None => rows.push((sec, k, format!("только на другом: {a}"))),
            }
        }
        for (sec, k, b) in ours {
            rows.push((sec, k, format!("только на этом: {b}")));
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

#[cfg(test)]
mod tests {
    use super::*;

    const PAGES: [(&str, &str); 4] =
        [("os", "Операционная система"), ("apps", "Установленные программы"), ("usb", "Устройства USB"), ("board", "Системная плата")];

    fn names(user: &str) -> privacy::Names {
        privacy::Names::of(&[user])
    }

    fn other_pc() -> Value {
        json!({
            "os": { "Now": "01.01.2026 / 10:00", "UptimeSec": 10, "RegisteredUser": "boris@example.com", "WindowsDirectory": "C:\\Users\\boris\\W" },
            "cs": { "Name": "BORIS-PC" },
            "board": { "Manufacturer": "ASUS", "SerialNumber": "111" },
            "apps": [
                { "Name": "Alpha", "Version": "1.0" },
                { "Name": "Beta", "Version": "2.0" },
            ],
            "usb": [{ "Name": "Mouse", "Manufacturer": "Logi" }],
        })
    }

    fn this_pc() -> Value {
        json!({
            "os": { "Now": "05.01.2026 / 12:00", "UptimeSec": 99999, "RegisteredUser": "anna@example.com", "WindowsDirectory": "C:\\Users\\anna\\W" },
            "cs": { "Name": "ANNA-PC" },
            "board": { "Manufacturer": "ASUS", "SerialNumber": "222" },
            "apps": [
                { "Name": "Alpha", "Version": "1.1" },
                { "Name": "Gamma", "Version": "3.0" },
            ],
            "usb": [{ "Name": "Mouse", "Manufacturer": "Logi" }],
        })
    }

    #[test]
    fn finds_what_differs() {
        // the other computer's facts travel through its JSON report
        let report = json!({ "program": "SysLens", "compare": to_json(&PAGES, &other_pc(), &names("boris")) });
        let text = serde_json::to_string(&report).unwrap();
        let other = parse(&serde_json::from_str(&text).unwrap()).unwrap();
        assert_eq!(other.made, "01.01.2026 10:00");
        let p = compare(&PAGES, &other, &this_pc(), &names("anna"));
        // time, uptime, names and serial numbers differ, but they are not compared; USB is the same
        assert_eq!(p.sections.len(), 1);
        let s = &p.sections[0];
        assert_eq!(s.title, "Установленные программы"); // the count "(2)" is not part of the name
        assert!(s.rows.contains(&("Alpha".into(), "другой: версия 1.0  →  этот: версия 1.1".into())));
        assert!(s.rows.contains(&("Beta".into(), "только на другом: версия 2.0".into())));
        assert!(s.rows.contains(&("Gamma".into(), "только на этом: версия 3.0".into())));
    }

    #[test]
    fn the_report_part_holds_nothing_personal() {
        let text = serde_json::to_string(&to_json(&PAGES, &other_pc(), &names("boris"))).unwrap();
        for secret in ["boris", "BORIS-PC", "111"] {
            assert!(!text.contains(secret), "{secret}");
        }
        assert!(text.contains("Users\\\\***\\\\W"));
    }

    #[test]
    fn printers_are_compared_without_the_neighbours() {
        let d = json!({ "printers": [{ "Name": "\\\\OFFICE-PC\\HP", "DriverName": "Drv", "PortName": "IP_192.168.1.50" }] });
        let rows = facts("printers", &d, &names("anna"));
        assert_eq!(rows, [("Принтеры".to_string(), "\\\\***\\HP".to_string(), "Drv  ·  IP_***".to_string())]);
        let text = serde_json::to_string(&to_json(&[("printers", "Принтеры")], &d, &names("anna"))).unwrap();
        assert!(!text.contains("OFFICE-PC") && !text.contains("192.168"));
    }

    #[test]
    fn same_computer_has_no_differences() {
        let d = this_pc();
        let other = parse(&json!({ "program": "SysLens", "compare": to_json(&PAGES, &d, &names("")) })).unwrap();
        assert!(compare(&PAGES, &other, &d, &names("")).sections.is_empty());
    }

    #[test]
    fn other_files_are_refused() {
        assert!(parse(&json!({ "hello": 1 })).is_err());
        // a report of version 0.3 has pages but nothing to compare by
        assert!(parse(&json!({ "program": "SysLens", "pages": [] })).unwrap_err().contains("старой версией"));
    }
}
