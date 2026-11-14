//! Plain-language judgements about the computer: what holds it back, what can
//! be added, what has not been updated for a long time and whether it is
//! ready for Windows 11. Sentences are put together in the language asked
//! for, so this page is not translated afterwards like the others.

use crate::i18n::{self, Lang};
use crate::info::{self, list, n, obj, s, Page, Section};
use crate::rules;
use serde_json::Value;

const GIB: f64 = 1024.0 * 1024.0 * 1024.0;

/// Memory use from which the computer is short of memory, %.
const MEMORY_FULL: f64 = 85.0;
/// A disk with less free space than this share is nearly full.
const DISK_FULL: f64 = 0.10;
/// Days without a restart worth mentioning.
const LONG_UPTIME_DAYS: u64 = 14;
/// Ages after which things count as old.
const BIOS_OLD_YEARS: i64 = 4;
const DRIVER_OLD_YEARS: i64 = 5;
const PROGRAM_OLD_YEARS: i64 = 3;
const UPDATE_OLD_DAYS: i64 = 60;
/// How many old drivers or programs are listed by name.
const LISTED: usize = 10;

/// Days since 1 January 1970 for a date; None when it is not a real date.
fn days(day: i64, month: i64, year: i64) -> Option<i64> {
    if !(1..=31).contains(&day) || !(1..=12).contains(&month) || !(1990..=2200).contains(&year) {
        return None;
    }
    let y = if month <= 2 { year - 1 } else { year };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * (month + if month > 2 { -3 } else { 9 }) + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    Some(era * 146097 + doe - 719468)
}

/// "14.11.2026" (anything may follow) → days.
fn dotted(text: &str) -> Option<i64> {
    let mut it = text.get(..10)?.split('.');
    let (d, m, y) = (it.next()?.parse().ok()?, it.next()?.parse().ok()?, it.next()?.parse().ok()?);
    days(d, m, y)
}

/// "20261114" → (days, "14.11.2026").
fn compact(text: &str) -> Option<(i64, String)> {
    if text.len() != 8 || !text.is_ascii() {
        return None;
    }
    let (y, m, d) = (&text[0..4], &text[4..6], &text[6..8]);
    Some((days(d.parse().ok()?, m.parse().ok()?, y.parse().ok()?)?, format!("{d}.{m}.{y}")))
}

struct Out {
    l: Lang,
    sections: Vec<Section>,
}

impl Out {
    fn section(&mut self, title: &str) {
        self.sections.push(Section { title: i18n::tr(title, self.l), rows: Vec::new() });
    }
    /// A row whose name and sentence are both known texts.
    fn say(&mut self, key: &str, template: &str, args: &[&str]) {
        let key = i18n::tr(key, self.l);
        self.raw(key, i18n::fmt(template, args, self.l));
    }
    fn raw(&mut self, key: String, text: String) {
        if let Some(sec) = self.sections.last_mut() {
            sec.rows.push((key, text));
        }
    }
    fn empty(&self) -> bool {
        self.sections.last().map_or(true, |s| s.rows.is_empty())
    }
}

/// The advice page in language `l`.
pub fn page(d: &Value, l: Lang) -> Page {
    let mut o = Out { l, sections: Vec::new() };
    let today = dotted(&s(obj(d, "os"), "Now"));
    bottlenecks(d, &mut o);
    upgrades(d, &mut o);
    outdated(d, today, &mut o);
    windows11(d, &mut o);
    o.sections.retain(|s| !s.rows.is_empty());
    Page { sections: o.sections }
}

/// Fixed disks: (letter, size, free).
fn volumes(d: &Value) -> Vec<(String, f64, f64)> {
    list(d, "vol")
        .iter()
        .filter(|v| n(v, "DriveType").map_or(true, |t| t == 3.0))
        .map(|v| (s(v, "DeviceID"), n(v, "Size").unwrap_or(0.0), n(v, "FreeSpace").unwrap_or(0.0)))
        .filter(|v| v.1 > 0.0)
        .collect()
}

fn bottlenecks(d: &Value, o: &mut Out) {
    o.section("Узкие места");
    let u = info::mem_use(d);
    if u.total > 0.0 {
        let pct = u.used / u.total * 100.0;
        if pct >= MEMORY_FULL {
            let pct = (pct.round() as u32).to_string();
            o.say("Память", "Память занята на {0} % — её не хватает для того, что сейчас запущено.", &[&pct]);
        }
    }
    let (total, _) = info::installed_memory(d);
    if total > 0 && (total as f64) < 7.5 * GIB {
        o.say("Память", "Всего {0} памяти — для современных программ этого мало.", &[&rules::mem_size(total)]);
    }
    let modules = list(d, "mem").len();
    let slots = n(d, "slots").unwrap_or(0.0) as usize;
    if modules == 1 && slots >= 2 {
        o.say("Память", "Один модуль памяти вместо двух: память работает в одноканальном режиме, а это медленнее.", &[]);
    }
    for (name, size, free) in volumes(d) {
        if free / size < DISK_FULL {
            let key = format!("{} {name}", i18n::tr("Диск", o.l));
            let text = i18n::fmt(
                "Осталось {0} свободного места из {1} — диск почти полон.",
                &[&rules::size(free as u64), &rules::size(size as u64)],
                o.l,
            );
            o.raw(key, text);
        }
    }
    let disks = list(d, "pdisk");
    if !disks.is_empty() && disks.iter().all(|x| s(x, "Media") == "HDD") {
        o.say(
            "Диск",
            "Нет ни одного SSD: система работает с жёсткого диска. С SSD компьютер загружается и работает заметно быстрее.",
            &[],
        );
    }
    for x in &disks {
        let health = s(x, "Health");
        if !health.is_empty() && health != "Healthy" {
            let key = format!("{} {}", i18n::tr("Диск", o.l), s(x, "FriendlyName"));
            o.raw(key, i18n::fmt("Состояние «{0}» — сделайте резервную копию важных файлов.", &[&health], o.l));
        }
    }
    for x in list(d, "rel") {
        if let Some(wear) = n(x, "Wear").filter(|w| *w >= 80.0) {
            let name = disks.iter().find(|p| s(p, "DeviceId") == s(x, "DeviceId")).map(|p| s(p, "FriendlyName")).unwrap_or_default();
            let key = format!("{} {name}", i18n::tr("Диск", o.l)).trim().to_string();
            o.raw(key, i18n::fmt("Износ {0} % — диск близок к концу срока службы.", &[&(wear as u32).to_string()], o.l));
        }
    }
    let bat = obj(d, "batx");
    if let (Some(full), Some(design)) = (n(bat, "Full").filter(|x| *x > 0.0), n(bat, "Design").filter(|x| *x > 0.0)) {
        let wear = (100.0 - full / design * 100.0).round();
        if wear >= 30.0 {
            o.say("Батарея", "Износ батареи {0} % — она держит заряд заметно меньше, чем новая.", &[&(wear as u32).to_string()]);
        }
    }
    if let Some(up) = n(obj(d, "os"), "UptimeSec") {
        let up_days = up as u64 / 86400;
        if up_days >= LONG_UPTIME_DAYS {
            o.say(
                "Перезагрузка",
                "Компьютер работает без перезагрузки {0} дн. — перезагрузка часто убирает подтормаживания.",
                &[&up_days.to_string()],
            );
        }
    }
    if o.empty() {
        o.say("Итог", "Явных узких мест не видно.", &[]);
    }
}

fn upgrades(d: &Value, o: &mut Out) {
    o.section("Что можно улучшить");
    let modules = list(d, "mem").len();
    let slots = n(d, "slots").unwrap_or(0.0) as usize;
    let (total, _) = info::installed_memory(d);
    // the system gives the board's limit in kilobytes
    let max = n(d, "maxmem").unwrap_or(0.0) as u64 * 1024;
    if slots > 0 && modules > 0 {
        if slots > modules {
            o.say("Слоты памяти", "Свободно слотов памяти: {0} из {1}.", &[&(slots - modules).to_string(), &slots.to_string()]);
        } else if max > total {
            o.say(
                "Слоты памяти",
                "Все слоты памяти заняты: чтобы добавить память, модули придётся заменить на более ёмкие.",
                &[],
            );
        }
    }
    if max > 0 && total > 0 {
        if max > total {
            o.say(
                "Максимум памяти",
                "Плата поддерживает до {0} памяти, сейчас установлено {1}.",
                &[&rules::mem_size(max), &rules::mem_size(total)],
            );
        } else {
            o.say("Максимум памяти", "Установлен максимум памяти, который поддерживает плата.", &[]);
        }
    }
    if o.empty() {
        o.say("Слоты памяти", "Сведений о слотах памяти нет.", &[]);
    }
}

fn outdated(d: &Value, today: Option<i64>, o: &mut Out) {
    let Some(today) = today else { return };
    let years = |then: i64| (today - then) / 365;
    o.section("Давно не обновлялось");
    let bios_date = s(obj(d, "bios"), "Date");
    if let Some(then) = dotted(&bios_date) {
        if years(then) >= BIOS_OLD_YEARS {
            o.say(
                "BIOS",
                "BIOS от {0} — ему больше {1} лет. Новая версия может быть на сайте производителя платы.",
                &[&bios_date, &years(then).to_string()],
            );
        }
    }
    for g in list(d, "gpu") {
        let date = s(g, "DriverDate");
        // drivers that come with Windows carry a made-up date of 2006
        if let Some(then) = dotted(&date).filter(|t| today - t > 365 && *t > days(1, 1, 2010).unwrap_or(0)) {
            let _ = then;
            o.raw(
                s(g, "Name"),
                i18n::fmt("Драйвер от {0} — ему больше года. Новая версия может быть на сайте производителя видеокарты.", &[&date], o.l),
            );
        }
    }
    let newest = list(d, "updates").iter().filter_map(|x| dotted(&s(x, "Date")).map(|t| (t, s(x, "Date")))).max();
    if let Some((then, date)) = newest {
        if today - then >= UPDATE_OLD_DAYS {
            o.say(
                "Обновления Windows",
                "Последнее обновление Windows установлено {0} — больше {1} дн. назад. Проверьте Центр обновления.",
                &[&date, &(today - then).to_string()],
            );
        }
    }
    if o.empty() {
        o.say("Итог", "Ничего давнего не найдено.", &[]);
    }

    // Oldest first; some vendors date drivers 1968 or 1970 on purpose, those are not real dates.
    let mut old: Vec<(i64, String, String)> = list(d, "drivers")
        .iter()
        .filter_map(|x| dotted(&s(x, "Date")).map(|t| (t, s(x, "DeviceName"), s(x, "Date"))))
        .filter(|x| years(x.0) >= DRIVER_OLD_YEARS)
        .collect();
    old.sort();
    old.dedup_by(|a, b| a.1 == b.1);
    listed(o, "Драйверы старше 5 лет", old, "драйвер от {0}");

    let mut old: Vec<(i64, String, String)> = list(d, "apps")
        .iter()
        .filter_map(|x| compact(&s(x, "Date")).map(|(t, date)| (t, s(x, "Name"), date)))
        .filter(|x| years(x.0) >= PROGRAM_OLD_YEARS)
        .collect();
    old.sort();
    // the date is that of installing: programs that update themselves do not touch it
    listed(o, "Программы, установленные более 3 лет назад", old, "установлена {0}");
}

/// A section "Title (N)" with the oldest items by name and "и ещё N" for the rest.
fn listed(o: &mut Out, title: &str, items: Vec<(i64, String, String)>, template: &str) {
    if items.is_empty() {
        return;
    }
    o.sections.push(Section { title: format!("{} ({})", i18n::tr(title, o.l), items.len()), rows: Vec::new() });
    for (_, name, date) in items.iter().take(LISTED) {
        o.raw(name.clone(), i18n::fmt(template, &[date], o.l));
    }
    if items.len() > LISTED {
        o.raw("…".into(), i18n::fmt("и ещё {0}", &[&(items.len() - LISTED).to_string()], o.l));
    }
}

/// How the requirements checked so far have come out.
#[derive(Default)]
struct Tally {
    /// At least one requirement is not met.
    failed: bool,
    /// At least one requirement could not be told.
    unknown: bool,
}

impl Tally {
    fn add(&mut self, met: Option<bool>) {
        match met {
            Some(true) => {}
            Some(false) => self.failed = true,
            None => self.unknown = true,
        }
    }
}

/// One requirement: Some(true) met, Some(false) not met, None could not be told.
/// Gives `met` back so that it can be counted.
fn check(o: &mut Out, key: &str, met: Option<bool>, text: String) -> Option<bool> {
    let (mark, text) = match met {
        Some(true) => ("✔", text),
        Some(false) => ("✖", text),
        None => ("?", i18n::tr("не удалось определить", o.l)),
    };
    o.raw(i18n::tr(key, o.l), format!("{mark}  {text}"));
    met
}

fn windows11(d: &Value, o: &mut Out) {
    // only Windows tells about its firmware and security chip
    let Some(fw) = d.get("fw").filter(|v| v.is_object()) else { return };
    let l = o.l;
    let tr = |x: &str| i18n::tr(x, l);
    o.section("Готовность к Windows 11");
    let mut tally = Tally::default();

    let cpu = list(d, "cpu");
    let c = cpu.first().copied().unwrap_or(&Value::Null);
    let cores = n(c, "NumberOfCores").unwrap_or(0.0);
    let bits = n(c, "AddressWidth").unwrap_or(0.0);
    let mhz = n(c, "MaxClockSpeed").unwrap_or(0.0);
    let met = if cpu.is_empty() { None } else { Some(cores >= 2.0 && bits >= 64.0 && mhz >= 1000.0) };
    tally.add(check(o, "Процессор: 64 бита, от 2 ядер, от 1 ГГц", met, tr(if met == Some(true) { "подходит" } else { "не подходит" })));

    let (total, _) = info::installed_memory(d);
    let met = if total == 0 { None } else { Some(total as f64 >= 3.5 * GIB) };
    tally.add(check(o, "Память: от 4 ГБ", met, i18n::value("", &rules::mem_size(total), l)));

    let letter: String = s(obj(d, "os"), "WindowsDirectory").chars().take(2).collect();
    let system = volumes(d).into_iter().find(|v| v.0.eq_ignore_ascii_case(&letter));
    let met = system.as_ref().map(|v| v.1 >= 60e9);
    let text = system.as_ref().map(|v| i18n::value("", &rules::size(v.1 as u64), l)).unwrap_or_default();
    tally.add(check(o, "Системный диск: от 64 ГБ", met, text));

    let kind = s(fw, "Type");
    let met = if kind.is_empty() { None } else { Some(kind.eq_ignore_ascii_case("UEFI")) };
    tally.add(check(o, "Загрузка через UEFI", met, kind.clone()));

    let met = match (n(fw, "SecureBoot"), kind.is_empty()) {
        (Some(x), _) => Some(x == 1.0),
        (None, false) => Some(false),
        (None, true) => None,
    };
    tally.add(check(o, "Secure Boot", met, tr(if met == Some(true) { "включён" } else { "выключен или не поддерживается" })));

    let tpm = s(fw, "Tpm");
    let met = Some(tpm.starts_with('2'));
    tally.add(check(o, "Модуль TPM 2.0", met, if tpm.is_empty() { tr("не найден") } else { format!("TPM {tpm}") }));

    // not a requirement of Windows 11 itself, but the update will not install without room
    if let Some(v) = &system {
        let text = i18n::value("", &format!("свободно {}", rules::size(v.2 as u64)), l);
        check(o, "Место для обновления: от 25 ГБ свободно", Some(v.2 >= 25.0 * GIB), text);
    }

    let verdict = if s(obj(d, "os"), "Caption").contains("Windows 11") {
        "На этом компьютере уже стоит Windows 11."
    } else if tally.failed {
        "Компьютер не подходит для Windows 11: выполнены не все требования."
    } else if tally.unknown {
        "Часть требований Windows 11 проверить не удалось (строки с «?»), остальные выполнены."
    } else {
        // only the basics are checked here: Setup also wants a processor from Microsoft's list
        "Основные требования Windows 11 выполнены. Модель процессора со списком Microsoft не сверялась."
    };
    o.say("Итог", verdict, &[]);
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn rows(d: &Value, l: Lang) -> Vec<(String, String, String)> {
        page(d, l).sections.into_iter().flat_map(|s| s.rows.into_iter().map(move |(k, v)| (s.title.clone(), k, v))).collect()
    }

    fn has(all: &[(String, String, String)], part: &str) -> bool {
        all.iter().any(|r| r.2.contains(part))
    }

    #[test]
    fn dates() {
        assert_eq!(days(1, 1, 2000), Some(10957));
        assert_eq!(days(1, 1, 1970), None); // made-up dates of old drivers
        assert_eq!(days(14, 11, 2026), Some(20771));
        assert_eq!(dotted("14.11.2026 / 00:17"), Some(20771));
        assert_eq!(compact("20261114"), Some((20771, "14.11.2026".into())));
        assert_eq!(dotted("31.13.2026"), None);
        assert_eq!(compact("2026"), None);
    }

    fn weak_pc() -> Value {
        json!({
            "os": { "Now": "14.11.2026 / 12:00", "Caption": "Windows 10 Home", "TotalVisibleMemorySize": 4000000, "FreePhysicalMemory": 200000,
                    "UptimeSec": 86400 * 20, "WindowsDirectory": "C:\\WINDOWS" },
            "cpu": [{ "Name": "Old", "NumberOfCores": 2, "AddressWidth": 64, "MaxClockSpeed": 2400 }],
            "mem": [{ "Capacity": 4294967296u64, "SMBIOSMemoryType": 24 }],
            "slots": 2,
            "maxmem": 16777216,
            "vol": [{ "DeviceID": "C:", "Size": 120e9, "FreeSpace": 5e9, "DriveType": 3 }],
            "pdisk": [{ "FriendlyName": "WD Blue", "Media": "HDD", "Health": "Warning", "DeviceId": "0" }],
            "bios": { "Date": "01.02.2015" },
            "gpu": [{ "Name": "GT 710", "DriverDate": "10.03.2021" }],
            "drivers": [{ "DeviceName": "Old thing", "Date": "05.05.2012" }, { "DeviceName": "Chipset", "Date": "18.07.1968" },
                        { "DeviceName": "New thing", "Date": "05.05.2026" }],
            "apps": [{ "Name": "Ancient", "Date": "20190101" }, { "Name": "Fresh", "Date": "20260901" }, { "Name": "Undated" }],
            "updates": [{ "HotFixID": "KB1", "Date": "01.03.2026" }, { "HotFixID": "KB2", "Date": "01.01.2026" }],
            "fw": { "Type": "Legacy", "SecureBoot": null, "Tpm": "1.2" },
        })
    }

    #[test]
    fn a_weak_computer_gets_told_everything() {
        let all = rows(&weak_pc(), Lang::Ru);
        for part in [
            "Память занята на 95 %",
            "Всего 4 ГБ памяти",
            "Один модуль памяти вместо двух",
            "Осталось 4.7 ГБ свободного места из 111.8 ГБ",
            "Нет ни одного SSD",
            "Состояние «Warning»",
            "без перезагрузки 20 дн.",
            "Свободно слотов памяти: 1 из 2.",
            "Плата поддерживает до 16 ГБ памяти, сейчас установлено 4 ГБ.",
            "BIOS от 01.02.2015 — ему больше 11 лет",
            "Драйвер от 10.03.2021",
            "установлено 01.03.2026 — больше 258 дн. назад",
            "драйвер от 05.05.2012",
            "установлена 01.01.2019",
            "✖  Legacy",
            "✖  TPM 1.2",
            "✖  выключен или не поддерживается",
            "✔  4 ГБ",
            "✖  свободно 4.7 ГБ",
            "Компьютер не подходит для Windows 11",
        ] {
            assert!(has(&all, part), "нет строки: {part}\n{all:#?}");
        }
        // made-up and recent dates are not "old"
        assert!(!all.iter().any(|r| r.1 == "Chipset" || r.1 == "New thing" || r.1 == "Fresh" || r.1 == "Undated"));
        assert!(all.iter().any(|r| r.0 == "Драйверы старше 5 лет (1)"));
    }

    #[test]
    fn a_good_computer_gets_good_news() {
        let d = json!({
            "os": { "Now": "14.11.2026 / 12:00", "Caption": "Windows 10 Pro", "TotalVisibleMemorySize": 33000000, "FreePhysicalMemory": 20000000,
                    "UptimeSec": 3600, "WindowsDirectory": "C:\\WINDOWS" },
            "cpu": [{ "Name": "New", "NumberOfCores": 6, "AddressWidth": 64, "MaxClockSpeed": 3700 }],
            "mem": [{ "Capacity": 17179869184u64 }, { "Capacity": 17179869184u64 }],
            "slots": 2,
            "maxmem": 33554432,
            "vol": [{ "DeviceID": "C:", "Size": 500e9, "FreeSpace": 300e9, "DriveType": 3 }],
            "pdisk": [{ "FriendlyName": "SSD", "Media": "SSD", "Health": "Healthy" }],
            "bios": { "Date": "01.02.2025" },
            "updates": [{ "HotFixID": "KB1", "Date": "10.11.2026" }],
            "fw": { "Type": "UEFI", "SecureBoot": 1, "Tpm": "2.0" },
        });
        let all = rows(&d, Lang::Ru);
        for part in [
            "Явных узких мест не видно.",
            "Установлен максимум памяти",
            "Ничего давнего не найдено.",
            "✔  UEFI",
            "✔  включён",
            "✔  TPM 2.0",
            "Основные требования Windows 11 выполнены. Модель процессора со списком Microsoft не сверялась.",
        ] {
            assert!(has(&all, part), "нет строки: {part}\n{all:#?}");
        }
        let en = rows(&d, Lang::En);
        assert!(has(&en, "No obvious bottlenecks."));
        assert!(has(&en, "The basic Windows 11 requirements are met. The processor model was not compared with Microsoft's list."));
        assert!(en.iter().any(|r| r.0 == "Windows 11 readiness" && r.1 == "Memory: 4 GB or more" && r.2 == "✔  32 GB"));
    }

    #[test]
    fn an_unknown_requirement_is_not_a_failed_one() {
        // no processor and no memory modules in the report: two rows cannot be told
        let d = json!({
            "os": { "Now": "14.11.2026 / 12:00", "Caption": "Windows 10 Pro", "WindowsDirectory": "C:\\WINDOWS" },
            "vol": [{ "DeviceID": "C:", "Size": 500e9, "FreeSpace": 300e9, "DriveType": 3 }],
            "fw": { "Type": "UEFI", "SecureBoot": 1, "Tpm": "2.0" },
        });
        let all = rows(&d, Lang::Ru);
        assert!(has(&all, "?  не удалось определить"), "{all:#?}");
        assert!(has(&all, "Часть требований Windows 11 проверить не удалось (строки с «?»), остальные выполнены."), "{all:#?}");
        assert!(!has(&all, "не подходит для Windows 11"));
        assert!(!has(&all, "Основные требования Windows 11 выполнены"));
        let en = rows(&d, Lang::En);
        assert!(has(&en, "Some Windows 11 requirements could not be checked (rows marked “?”); the rest are met."), "{en:#?}");

        // a failed requirement outweighs an unknown one
        let mut d = d;
        d["fw"]["Tpm"] = json!("1.2");
        let all = rows(&d, Lang::Ru);
        assert!(has(&all, "?  не удалось определить"));
        assert!(has(&all, "Компьютер не подходит для Windows 11: выполнены не все требования."), "{all:#?}");
    }

    #[test]
    fn other_systems_get_no_windows_section() {
        let d = json!({ "os": { "Now": "14.11.2026 / 12:00" }, "cpu": [{ "Name": "X" }] });
        assert!(!page(&d, Lang::Ru).sections.iter().any(|s| s.title.contains("Windows 11")));
    }
}
