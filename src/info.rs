//! Gathers hardware information through Windows management data (WMI/CIM,
//! via PowerShell) and turns it into pages of "field / value" rows.

use crate::rules;
use serde_json::Value;
use std::process::Command;

const SCRIPT: &str = include_str!("collect.ps1");

/// How long the Windows collection may take before it is given up on.
#[cfg_attr(not(windows), allow(dead_code))]
const TIMEOUT_SECS: u64 = 90;

/// "64 бит"; nothing when the number itself is missing.
fn unit(v: String, u: &str) -> String {
    if v.is_empty() { v } else { format!("{v} {u}") }
}

pub struct Section {
    pub title: String,
    pub rows: Vec<(String, String)>,
}

pub struct Page {
    pub sections: Vec<Section>,
}

impl Page {
    fn new() -> Self {
        Page { sections: Vec::new() }
    }
    fn section(&mut self, title: &str) -> &mut Section {
        self.sections.push(Section { title: title.to_string(), rows: Vec::new() });
        self.sections.last_mut().unwrap()
    }
}

impl Section {
    fn row(&mut self, k: &str, v: impl Into<String>) {
        let v = v.into();
        if !v.trim().is_empty() {
            self.rows.push((k.to_string(), v));
        }
    }
}

/// Installed programs from the system's uninstall list: name → version, publisher, date, size.
fn apps(d: &Value, sort: AppSort) -> Page {
    let mut p = Page::new();
    let mut list = list(d, "apps");
    let date = |a: &Value| Some(s(a, "Date")).filter(|x| x.len() == 8 && x.chars().all(|c| c.is_ascii_digit()));
    list.sort_by_key(|a| s(a, "Name").to_lowercase());
    match sort {
        AppSort::Name => {}
        AppSort::Size => list.sort_by(|a, b| n(b, "SizeKb").unwrap_or(0.0).total_cmp(&n(a, "SizeKb").unwrap_or(0.0))),
        AppSort::Date => list.sort_by(|a, b| date(b).cmp(&date(a))), // newest first, undated last
        AppSort::Publisher => {
            list.sort_by_key(|a| {
                let publ = s(a, "Publisher").to_lowercase();
                (publ.is_empty(), publ)
            })
        }
    }
    let sec = p.section(&format!("Установленные программы ({})", list.len()));
    for a in &list {
        let mut parts = Vec::new();
        let ver = s(a, "Version");
        if !ver.is_empty() {
            parts.push(format!("версия {ver}"));
        }
        let publ = s(a, "Publisher");
        if !publ.is_empty() {
            parts.push(publ);
        }
        let date = s(a, "Date");
        let valid = date.len() == 8
            && date.chars().all(|c| c.is_ascii_digit())
            && (1..=12).contains(&date[4..6].parse::<u32>().unwrap_or(0))
            && (1..=31).contains(&date[6..8].parse::<u32>().unwrap_or(0));
        if valid {
            parts.push(format!("{}.{}.{}", &date[6..8], &date[4..6], &date[0..4]));
        }
        if let Some(kb) = n(a, "SizeKb").filter(|k| *k > 0.0) {
            parts.push(rules::size((kb * 1024.0) as u64));
        }
        if a.get("Store") == Some(&Value::Bool(true)) {
            parts.push("Microsoft Store".into());
        }
        if parts.is_empty() {
            parts.push("—".into());
        }
        sec.row(&s(a, "Name"), parts.join("  ·  "));
    }
    p
}

/// Gathers everything: on Linux from /proc and /sys, on macOS from the
/// system's own tools, elsewhere with the Windows script.
pub fn collect() -> Result<Value, String> {
    #[cfg(target_os = "linux")]
    return crate::linux::collect();
    #[cfg(target_os = "macos")]
    return crate::mac::collect();
    #[allow(unreachable_code)]
    collect_windows()
}

/// The script as two scripts that can run at the same time: hardware, and
/// programs with devices. Each gets the opening lines and the closing line.
#[cfg_attr(not(windows), allow(dead_code))]
fn script_halves() -> [String; 2] {
    let (first, second) = SCRIPT.split_once("# --- SECOND HALF ---").unwrap_or((SCRIPT, ""));
    let head: String = SCRIPT.lines().take(3).map(|l| format!("{l}\n")).collect();
    let tail = SCRIPT.trim_end().lines().last().unwrap_or_default();
    // "#" keeps the rest of the marker line a comment
    [format!("{first}\n{tail}\n"), format!("{head}#{second}")]
}

/// Runs both halves of the Windows collection script side by side and joins
/// their answers: together they take about as long as the slower one.
#[cfg_attr(not(windows), allow(dead_code))]
fn collect_windows() -> Result<Value, String> {
    let [first, second] = script_halves();
    let side = std::thread::spawn(move || run_script(second));
    let mut all = run_script(first)?;
    // Programs and devices are extra: without them the hardware pages still work.
    if let (Some(all), Ok(Ok(Value::Object(more)))) = (all.as_object_mut(), side.join()) {
        all.extend(more);
    }
    Ok(all)
}

/// Runs one PowerShell script and returns its JSON.
#[cfg_attr(not(windows), allow(dead_code))]
fn run_script(script: String) -> Result<Value, String> {
    use std::io::{Read, Write};
    use std::process::Stdio;
    // The script is handed to PowerShell through its input, so nothing is
    // written to disk. It must stay plain ASCII: the input has no fixed encoding.
    let mut cmd = Command::new("powershell.exe");
    cmd.args(["-NoProfile", "-NonInteractive", "-Command", "& ([scriptblock]::Create([Console]::In.ReadToEnd()))"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    let mut child = cmd.spawn().map_err(|e| format!("Не удалось запустить PowerShell: {e}"))?;
    if let Some(mut input) = child.stdin.take() {
        std::thread::spawn(move || {
            let _ = input.write_all(script.as_bytes());
        });
    }
    // Read the answer on the side, so a stuck system query can be given up on.
    let (tx, rx) = std::sync::mpsc::channel();
    if let Some(mut pipe) = child.stdout.take() {
        std::thread::spawn(move || {
            let mut buf = Vec::new();
            let _ = pipe.read_to_end(&mut buf);
            let _ = tx.send(buf);
        });
    }
    let got = rx.recv_timeout(std::time::Duration::from_secs(TIMEOUT_SECS));
    if got.is_err() {
        let _ = child.kill();
    }
    let _ = child.wait();
    let bytes = got.map_err(|_| "Система не ответила вовремя".to_string())?;
    let text = String::from_utf8_lossy(&bytes);
    let text = text.trim_start_matches('\u{feff}').trim();
    let start = text.find('{').ok_or_else(|| "Пустой ответ системы".to_string())?;
    serde_json::from_str(&text[start..]).map_err(|e| format!("Ошибка разбора данных: {e}"))
}

// ---------- helpers ----------

pub(crate) fn s(v: &Value, k: &str) -> String {
    match v.get(k) {
        Some(Value::String(x)) => x.trim().to_string(),
        Some(Value::Number(n)) => n.to_string(),
        Some(Value::Bool(b)) => if *b { "Включено".into() } else { "Отключено".into() },
        Some(Value::Array(a)) => a
            .iter()
            .filter_map(|x| x.as_str().map(|s| s.to_string()))
            .collect::<Vec<_>>()
            .join(", "),
        _ => String::new(),
    }
}

pub(crate) fn n(v: &Value, k: &str) -> Option<f64> {
    match v.get(k) {
        Some(Value::Number(x)) => x.as_f64(),
        Some(Value::String(x)) => x.trim().parse().ok(),
        _ => None,
    }
}

pub(crate) fn list<'a>(v: &'a Value, k: &str) -> Vec<&'a Value> {
    match v.get(k) {
        Some(Value::Array(a)) => a.iter().filter(|x| x.is_object()).collect(),
        Some(o @ Value::Object(_)) => vec![o],
        _ => vec![],
    }
}

pub(crate) fn obj<'a>(v: &'a Value, k: &str) -> &'a Value {
    static NULL: Value = Value::Null;
    match v.get(k) {
        Some(Value::Array(a)) => a.first().unwrap_or(&NULL),
        Some(o) => o,
        None => &NULL,
    }
}

const MIB: f64 = 1024.0 * 1024.0;

/// Memory sizes: binary megabytes, like "16258 МБ".
pub fn mb(bytes: f64) -> String {
    format!("{} МБ", (bytes / MIB).round() as u64)
}

/// Module / VRAM sizes: whole gigabytes when exact, else megabytes.
fn gb_or_mb(bytes: f64) -> String {
    let m = bytes / MIB;
    let g = (m / 1024.0).round();
    // within 1% of a whole number of gigabytes -> show gigabytes
    if g >= 1.0 && ((m - g * 1024.0).abs() / (g * 1024.0)) < 0.01 {
        format!("{} ГБ", g as u64)
    } else {
        format!("{} МБ", m.round() as u64)
    }
}

/// Volume sizes in megabytes, like "475834 МБ" (or "500107 МБ" in the units
/// printed on the box).
fn vol_mb(bytes: f64) -> String {
    let step = if rules::decimal() { 1e6 } else { MIB };
    format!("{} МБ", (bytes / step).floor() as u64)
}

pub(crate) fn mem_type(code: f64) -> &'static str {
    match code as u32 {
        20 => "DDR",
        21 => "DDR2",
        22 => "DDR2 FB-DIMM",
        24 => "DDR3",
        26 => "DDR4",
        27 => "LPDDR",
        28 => "LPDDR2",
        29 => "LPDDR3",
        30 => "LPDDR4",
        34 => "DDR5",
        35 => "LPDDR5",
        _ => "",
    }
}

fn form_factor(code: f64) -> &'static str {
    match code as u32 {
        8 => "DIMM",
        12 => "SO-DIMM",
        13 => "Micro-DIMM",
        _ => "",
    }
}

fn pc_type(code: f64) -> &'static str {
    match code as u32 {
        1 => "Настольный компьютер",
        2 => "Мобильный компьютер",
        3 => "Рабочая станция",
        4 | 5 | 7 => "Сервер",
        8 => "Планшет",
        _ => "Компьютер",
    }
}

fn uptime(sec: f64) -> String {
    let t = sec as u64;
    let d = t / 86400;
    let h = (t % 86400) / 3600;
    let m = (t % 3600) / 60;
    let sc = t % 60;
    if d > 0 {
        format!("{d} дн. {h} ч {m} мин {sc} с")
    } else {
        format!("{h} ч {m} мин {sc} с")
    }
}

/// Slot name: the board's own label, or "DIMMn" when the labels repeat.
fn slot_name(mods: &[&Value], i: usize) -> String {
    let loc = s(mods[i], "DeviceLocator");
    let dup = mods.iter().filter(|m| s(m, "DeviceLocator") == loc).count() > 1;
    if loc.is_empty() || dup {
        let bank = s(mods[i], "BankLabel");
        if !bank.is_empty() && mods.iter().filter(|m| s(m, "BankLabel") == bank).count() == 1 {
            format!("{loc} / {bank}")
        } else {
            format!("DIMM{}", i + 1)
        }
    } else {
        loc.replace(' ', "")
    }
}

pub(crate) fn vram_of(d: &Value, gpu: &Value) -> Option<f64> {
    let name = s(gpu, "Name");
    for v in list(d, "vram") {
        if s(v, "Desc") == name {
            return n(v, "Size");
        }
    }
    n(gpu, "AdapterRAM").filter(|x| *x > 0.0)
}

pub(crate) fn os_name(d: &Value) -> String {
    let os = obj(d, "os");
    let x = obj(d, "osx");
    let mut name = s(os, "Caption");
    let dv = s(x, "DisplayVersion");
    if !dv.is_empty() {
        name.push_str(&format!(" {dv}"));
    }
    name
}

fn os_build(d: &Value) -> String {
    let os = obj(d, "os");
    let x = obj(d, "osx");
    let ubr = s(x, "UBR");
    let v = s(os, "Version");
    if ubr.is_empty() { v } else { format!("{v}.{ubr}") }
}

// ---------- pages ----------

/// How the programs page is ordered.
#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub enum AppSort {
    #[default]
    Name,
    Size,
    Date,
    Publisher,
}

pub fn page(id: &str, d: &Value) -> Page {
    page_sorted(id, d, AppSort::Name)
}

pub fn page_sorted(id: &str, d: &Value, sort: AppSort) -> Page {
    match id {
        "battery" => battery(d),
        "startup" => listing(d, "startup", "Автозагрузка", "Name", |a| vec![s(a, "Command"), s(a, "Location")]),
        "services" => listing(d, "services", "Службы с автозапуском", "DisplayName", |a| {
            vec![match s(a, "State").as_str() {
                "Running" => "Работает".to_string(),
                "Stopped" => "Остановлена".to_string(),
                other => other.to_string(),
            }]
        }),
        "drivers" => listing(d, "drivers", "Драйверы сторонних производителей", "DeviceName", |a| {
            let ver = s(a, "DriverVersion");
            vec![if ver.is_empty() { ver } else { format!("версия {ver}") }, s(a, "DriverProviderName"), s(a, "Date")]
        }),
        "updates" => listing(d, "updates", "Обновления системы", "HotFixID", |a| vec![s(a, "Description"), s(a, "Date")]),
        "usb" => listing(d, "usb", "Устройства USB", "Name", |a| vec![s(a, "Manufacturer")]),
        "printers" => listing(d, "printers", "Принтеры", "Name", |a| {
            let main = if a.get("Default") == Some(&Value::Bool(true)) { "основной" } else { "" };
            vec![s(a, "DriverName"), s(a, "PortName"), main.to_string()]
        }),
        "summary" => summary(d),
        "cpu" => cpu(d),
        "board" => board(d),
        "memory" => memory(d),
        "spd" => spd(d),
        "bios" => bios(d),
        "os" => os(d),
        "gpu" => gpu(d),
        "monitor" => monitor(d),
        "audio" => audio(d),
        "pdisk" => pdisk(d),
        "ldisk" => ldisk(d),
        "net" => net(d),
        "apps" => apps(d, sort),
        _ => Page::new(),
    }
}

fn summary(d: &Value) -> Page {
    let mut p = Page::new();
    let cs = obj(d, "cs");
    let os = obj(d, "os");

    let sec = p.section("Компьютер");
    sec.row("Тип компьютера", pc_type(n(cs, "PCSystemType").unwrap_or(0.0)));
    sec.row("Операционная система", os_name(d));
    sec.row("Сборка ОС", os_build(d));
    sec.row("Имя компьютера", s(cs, "Name"));
    sec.row("Имя пользователя", s(cs, "UserName"));
    sec.row("Дата и время", s(os, "Now"));
    if let Some(u) = n(os, "UptimeSec") {
        sec.row("Время работы", uptime(u));
    }

    let sec = p.section("Системная плата");
    let sockets = list(d, "cpu").len() as u32;
    if let Some(c) = list(d, "cpu").first() {
        sec.row(
            "Тип ЦП",
            rules::cpu_line(
                &s(c, "Name"),
                n(c, "NumberOfCores").unwrap_or(0.0) as u32,
                n(c, "NumberOfLogicalProcessors").unwrap_or(0.0) as u32,
                sockets,
                n(c, "MaxClockSpeed").unwrap_or(0.0) as u32,
            ),
        );
    }
    let b = obj(d, "board");
    sec.row("Системная плата", format!("{} {}", s(b, "Manufacturer"), s(b, "Product")).trim().to_string());
    let mods = list(d, "mem");
    let (total, ty) = installed_memory(d);
    sec.row("Системная память", rules::memory_line(total, ty));
    for (i, m) in mods.iter().enumerate() {
        sec.row(
            &slot_name(&mods, i),
            rules::dimm_line(
                n(m, "Capacity").unwrap_or(0.0) as u64,
                n(m, "SMBIOSMemoryType").map(mem_type).unwrap_or(""),
                n(m, "Speed").unwrap_or(0.0) as u32,
                &s(m, "Manufacturer"),
                &s(m, "PartNumber"),
            ),
        );
    }
    let bi = obj(d, "bios");
    let (maker, date) = (s(bi, "Manufacturer"), s(bi, "Date"));
    sec.row("Тип BIOS", if date.is_empty() { maker } else { format!("{maker} ({date})").trim().to_string() });

    let sec = p.section("Отображение");
    for g in list(d, "gpu") {
        let mem = vram_of(d, g)
            .filter(|x| *x >= 1.0)
            .map(|x| format!("  ·  {}", rules::mem_size(x as u64)))
            .unwrap_or_default();
        sec.row("Видеоадаптер", format!("{}{}", s(g, "Name"), mem));
    }
    for m in list(d, "mon") {
        let name = s(m, "Name");
        if !name.is_empty() {
            sec.row("Монитор", monitor_text(m));
        }
    }

    let sec = p.section("Мультимедиа");
    for a in list(d, "audio") {
        sec.row("Звуковой адаптер", s(a, "Name"));
    }

    let sec = p.section("Хранение данных");
    for k in list(d, "disk") {
        sec.row(
            "Дисковый накопитель",
            rules::disk_line(&s(k, "Model"), n(k, "Size").unwrap_or(0.0) as u64, &disk_bus(d, k)),
        );
    }

    let sec = p.section("Разделы");
    let (mut tt, mut tf) = (0u64, 0u64);
    for v in list(d, "vol") {
        if n(v, "DriveType") != Some(3.0) {
            continue; // built-in disks only, removable ones come and go
        }
        let total = n(v, "Size").unwrap_or(0.0) as u64;
        let free = n(v, "FreeSpace").unwrap_or(0.0) as u64;
        tt += total;
        tf += free;
        sec.row(&format!("{} ({})", s(v, "DeviceID"), s(v, "FileSystem")), rules::space_line(total, free));
    }
    if sec.rows.len() > 1 {
        sec.row("Общий размер", rules::space_line(tt, tf));
    }

    let sec = p.section("Сеть");
    for a in list(d, "ip") {
        sec.row("Первичный адрес IP", first(a, "IPAddress"));
    }
    for a in list(d, "net") {
        sec.row("Сетевой адаптер", s(a, "Name"));
    }
    p.sections.retain(|s| !s.rows.is_empty());
    p
}

/// What is installed: the modules added up (without module data, what the
/// system sees), and the kind of memory ("DDR4").
pub(crate) fn installed_memory(d: &Value) -> (u64, &'static str) {
    let mods = list(d, "mem");
    let ty = mods.first().and_then(|m| n(m, "SMBIOSMemoryType")).map(mem_type).unwrap_or("");
    let installed: u64 = mods.iter().filter_map(|m| n(m, "Capacity")).map(|x| x as u64).sum();
    let total = if installed > 0 { installed } else { n(obj(d, "cs"), "TotalPhysicalMemory").unwrap_or(0.0) as u64 };
    (total, ty)
}

/// The computer in one line, to paste into a chat:
/// "Ryzen 5 5600X (6C/12T) / RTX 3060 12 ГБ / 32 ГБ DDR4 / SSD 465.8 ГБ NVMe / Windows 11 Pro".
/// Nothing in it says whose computer it is.
pub fn summary_line(d: &Value) -> String {
    let tidy = |x: String| x.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut parts: Vec<String> = Vec::new();
    if let Some(c) = list(d, "cpu").first() {
        let cores = n(c, "NumberOfCores").unwrap_or(0.0) as u32;
        let threads = n(c, "NumberOfLogicalProcessors").unwrap_or(0.0) as u32;
        let count = match (cores, threads) {
            (0, _) => String::new(),
            (c, t) if t > c => format!(" ({c}C/{t}T)"),
            (c, _) => format!(" ({c}C)"),
        };
        parts.push(format!("{}{count}", tidy(s(c, "Name"))));
    }
    let gpus: Vec<String> = list(d, "gpu")
        .iter()
        .map(|g| {
            let mem = vram_of(d, g).filter(|x| *x >= 1.0).map(|x| format!(" {}", rules::mem_size(x as u64))).unwrap_or_default();
            format!("{}{mem}", tidy(s(g, "Name")))
        })
        .filter(|x| !x.is_empty())
        .collect();
    parts.push(gpus.join(" + "));
    let (total, ty) = installed_memory(d);
    if total > 0 {
        parts.push(format!("{} {ty}", rules::mem_size(total)).trim().to_string());
    }
    let disks: Vec<String> = list(d, "disk")
        .iter()
        .map(|k| {
            let size = n(k, "Size").filter(|x| *x > 0.0).map(|x| rules::size(x as u64)).unwrap_or_default();
            tidy(format!("{} {size} {}", s(k, "Model"), disk_bus(d, k)))
        })
        .filter(|x| !x.is_empty())
        .collect();
    parts.push(disks.join(" + "));
    parts.push(tidy(os_name(d)));
    parts.retain(|x| !x.is_empty());
    parts.join(" / ")
}

/// Monitor value: maker, name and the diagonal from the screen's own size (cm).
fn monitor_text(m: &Value) -> String {
    let name = format!("{} {}", s(m, "Maker"), s(m, "Name"));
    let w = n(m, "WidthCm").unwrap_or(0.0);
    let h = n(m, "HeightCm").unwrap_or(0.0);
    let diag = if w > 0.0 && h > 0.0 { Some(((w * w + h * h).sqrt() / 2.54).round() as u32) } else { None };
    rules::monitor_line(&name, diag)
}

/// How a disk is connected ("NVMe", "SATA", "USB"): from the storage list,
/// else the older interface name.
pub(crate) fn disk_bus(d: &Value, disk: &Value) -> String {
    list(d, "pdisk")
        .iter()
        .find(|x| s(x, "DeviceId") == s(disk, "Index"))
        .map(|x| s(x, "Bus"))
        .filter(|b| !b.is_empty())
        .unwrap_or_else(|| s(disk, "InterfaceType"))
}

fn first(v: &Value, k: &str) -> String {
    match v.get(k) {
        Some(Value::Array(a)) => a.first().and_then(|x| x.as_str()).unwrap_or("").to_string(),
        _ => s(v, k),
    }
}

fn cpu(d: &Value) -> Page {
    let mut p = Page::new();
    for (i, c) in list(d, "cpu").into_iter().enumerate() {
        let title = if i == 0 { "Свойства ЦП".to_string() } else { format!("Свойства ЦП{}", i + 1) };
        let sec = p.section(&title);
        sec.row("Тип ЦП", s(c, "Name"));
        sec.row("Производитель", s(c, "Manufacturer"));
        sec.row("Описание", s(c, "Description"));
        sec.row("Разъём", s(c, "SocketDesignation"));
        sec.row("Ядра", s(c, "NumberOfCores"));
        sec.row("Логические процессоры", s(c, "NumberOfLogicalProcessors"));
        if let Some(x) = n(c, "MaxClockSpeed") {
            sec.row("Исходная частота", format!("{} МГц", x as u32));
        }
        if let Some(x) = n(c, "CurrentClockSpeed") {
            sec.row("Текущая частота", format!("{} МГц", x as u32));
        }
        sec.row("Разрядность", unit(s(c, "AddressWidth"), "бит"));
        sec.row("ID процессора", s(c, "ProcessorId"));
        sec.row("Виртуализация", s(c, "VirtualizationFirmwareEnabled"));
    }

    // One cache section for the whole computer: the system reports the caches
    // of all processors together.
    let sec = p.section("Кэш");
    // Win32_CacheMemory.Level: 3 = primary (L1), 4 = secondary (L2), 5 = tertiary (L3)
    let mut sums = [0u64; 3];
    for k in list(d, "cache") {
        let lvl = n(k, "Level").unwrap_or(0.0) as usize;
        if (3..=5).contains(&lvl) {
            sums[lvl - 3] += n(k, "InstalledSize").unwrap_or(0.0) as u64;
        }
    }
    for (i, name) in ["Кэш L1", "Кэш L2", "Кэш L3"].iter().enumerate() {
        if sums[i] > 0 {
            sec.row(name, format!("{} КБ", sums[i]));
        }
    }
    // Fallback: the per-processor figures, added up over all processors.
    for (name, field) in [("Кэш L2", "L2CacheSize"), ("Кэш L3", "L3CacheSize")] {
        if !sec.rows.iter().any(|r| r.0 == name) {
            let total: u64 = list(d, "cpu").iter().filter_map(|c| n(c, field)).map(|x| x as u64).sum();
            if total > 0 {
                sec.row(name, format!("{total} КБ"));
            }
        }
    }
    p.sections.retain(|s| !s.rows.is_empty());
    p
}

fn board(d: &Value) -> Page {
    let mut p = Page::new();
    let b = obj(d, "board");
    let cs = obj(d, "cs");
    let pr = obj(d, "sysenc");
    let sec = p.section("Свойства системной платы");
    sec.row("Производитель", s(b, "Manufacturer"));
    sec.row("Системная плата", s(b, "Product"));
    sec.row("Версия", s(b, "Version"));
    sec.row("Серийный номер", s(b, "SerialNumber"));
    let sec = p.section("Компьютер");
    sec.row("Производитель", s(cs, "Manufacturer"));
    sec.row("Модель", s(cs, "Model"));
    sec.row("UUID", s(pr, "UUID"));
    sec.row("Тип системы", s(cs, "SystemType"));
    sec.row("Число процессоров", s(cs, "NumberOfProcessors"));
    let sec = p.section("Свойства шины памяти");
    sec.row("Разъёмы памяти", s(d, "slots"));
    sec.row("Занято разъёмов", list(d, "mem").len().to_string());
    if let Some(x) = n(d, "maxmem").filter(|x| *x > 0.0) {
        sec.row("Макс. объём памяти", format!("{} ГБ", (x / 1024.0 / 1024.0).round() as u64));
    }
    if let Some(m) = list(d, "mem").first() {
        let ty = n(m, "SMBIOSMemoryType").map(mem_type).unwrap_or("");
        sec.row("Тип памяти", ty);
        if let Some(x) = n(m, "ConfiguredClockSpeed") {
            sec.row("Скорость памяти", format!("{} MT/s", x as u32));
        }
    }
    p.sections.retain(|s| !s.rows.is_empty());
    p
}

pub struct MemUse {
    pub total: f64,
    pub used: f64,
    pub vtotal: f64,
    pub vused: f64,
}

pub fn mem_use(d: &Value) -> MemUse {
    let os = obj(d, "os");
    let total = n(os, "TotalVisibleMemorySize").unwrap_or(0.0) * 1024.0;
    let free = n(os, "FreePhysicalMemory").unwrap_or(0.0) * 1024.0;
    let vtotal = n(os, "TotalVirtualMemorySize").unwrap_or(0.0) * 1024.0;
    let vfree = n(os, "FreeVirtualMemory").unwrap_or(0.0) * 1024.0;
    MemUse { total, used: total - free, vtotal, vused: vtotal - vfree }
}

fn memory(d: &Value) -> Page {
    let mut p = Page::new();
    let u = mem_use(d);
    let pct = |a: f64, b: f64| if b > 0.0 { format!("{} %", (a / b * 100.0).round() as u32) } else { String::new() };
    let sec = p.section("Физическая память");
    sec.row("Всего", mb(u.total));
    sec.row("Занято", mb(u.used));
    sec.row("Свободно", mb(u.total - u.used));
    sec.row("Загрузка", pct(u.used, u.total));
    let sec = p.section("Виртуальная память");
    sec.row("Всего", mb(u.vtotal));
    sec.row("Занято", mb(u.vused));
    sec.row("Свободно", mb(u.vtotal - u.vused));
    sec.row("Загрузка", pct(u.vused, u.vtotal));
    p
}

fn spd(d: &Value) -> Page {
    let mut p = Page::new();
    let mods = list(d, "mem");
    for (i, m) in mods.iter().enumerate() {
        let title = format!("Модуль памяти  {}", slot_name(&mods, i));
        let sec = p.section(&title);
        sec.row("Производитель", s(m, "Manufacturer"));
        sec.row("Модель", s(m, "PartNumber"));
        sec.row("Серийный номер", s(m, "SerialNumber"));
        if let Some(x) = n(m, "Capacity") {
            sec.row("Размер модуля", gb_or_mb(x));
        }
        sec.row("Тип модуля", n(m, "FormFactor").map(form_factor).unwrap_or(""));
        let ty = n(m, "SMBIOSMemoryType").map(mem_type).unwrap_or("");
        if !ty.is_empty() {
            sec.row("Тип памяти", format!("{ty} SDRAM"));
        }
        if let Some(x) = n(m, "Speed") {
            let mhz = format!("{} МГц", (x / 2.0) as u32);
            sec.row("Скорость памяти", if ty.is_empty() { mhz } else { format!("{ty}-{} ({mhz})", x as u32) });
        }
        if let Some(x) = n(m, "ConfiguredClockSpeed") {
            sec.row("Текущая скорость", format!("{} MT/s", x as u32));
        }
        if let Some(x) = n(m, "ConfiguredVoltage").filter(|x| *x > 0.0) {
            sec.row("Напряжение", format!("{:.3} В", x / 1000.0));
        }
        sec.row("Ширина шины", unit(s(m, "DataWidth"), "бит"));
        sec.row("Банк", s(m, "BankLabel"));
    }
    if p.sections.is_empty() {
        p.section("Модуль памяти").row("Статус", "Нет данных о модулях памяти");
    }
    p
}

fn bios(d: &Value) -> Page {
    let mut p = Page::new();
    let b = obj(d, "bios");
    let sec = p.section("Свойства BIOS");
    sec.row("Производитель", s(b, "Manufacturer"));
    sec.row("Версия BIOS", s(b, "SMBIOSBIOSVersion"));
    sec.row("Дата BIOS", s(b, "Date"));
    let major = s(b, "SMBIOSMajorVersion");
    if !major.is_empty() {
        sec.row("Версия SMBIOS", format!("{major}.{}", s(b, "SMBIOSMinorVersion")).trim_end_matches('.').to_string());
    }
    p.sections.retain(|s| !s.rows.is_empty());
    p
}

fn os(d: &Value) -> Page {
    let mut p = Page::new();
    let o = obj(d, "os");
    let cs = obj(d, "cs");
    let sec = p.section("Свойства операционной системы");
    sec.row("Название ОС", os_name(d));
    sec.row("Версия ОС", os_build(d));
    sec.row("Разрядность", s(o, "OSArchitecture"));
    sec.row("Дата установки", s(o, "InstallDate"));
    sec.row("Зарегистрированный пользователь", s(o, "RegisteredUser"));
    sec.row("Папка Windows", s(o, "WindowsDirectory"));
    sec.row("Системная папка", s(o, "SystemDirectory"));
    let sec = p.section("Текущий сеанс");
    sec.row("Имя компьютера", s(cs, "Name"));
    sec.row("Пользователь", s(cs, "UserName"));
    sec.row("Домен / рабочая группа", s(cs, "Domain"));
    sec.row("Последняя загрузка", s(o, "LastBoot"));
    if let Some(u) = n(o, "UptimeSec") {
        sec.row("Время работы", uptime(u));
    }
    p
}

fn gpu(d: &Value) -> Page {
    let mut p = Page::new();
    for g in list(d, "gpu") {
        let sec = p.section(&format!("Свойства видеоадаптера  {}", s(g, "Name")));
        sec.row("Видеоадаптер", s(g, "Name"));
        sec.row("Производитель", s(g, "AdapterCompatibility"));
        sec.row("Графический процессор", s(g, "VideoProcessor"));
        if let Some(x) = vram_of(d, g) {
            sec.row("Объём видеопамяти", rules::mem_size(x as u64));
        }
        sec.row("Тип ЦАП", s(g, "AdapterDACType"));
        sec.row("ID устройства", s(g, "PNPDeviceID"));
        let sec = p.section("Драйвер");
        sec.row("Версия драйвера", s(g, "DriverVersion"));
        sec.row("Дата драйвера", s(g, "DriverDate"));
        let w = n(g, "CurrentHorizontalResolution");
        let h = n(g, "CurrentVerticalResolution");
        if let (Some(w), Some(h)) = (w, h) {
            let sec = p.section("Текущий видеорежим");
            sec.row("Разрешение", format!("{} x {}", w as u32, h as u32));
            sec.row("Глубина цвета", unit(s(g, "CurrentBitsPerPixel"), "бит"));
            sec.row("Частота обновления", unit(s(g, "CurrentRefreshRate"), "Гц"));
        }
    }
    p.sections.retain(|s| !s.rows.is_empty());
    p
}

fn monitor(d: &Value) -> Page {
    let mut p = Page::new();
    for m in list(d, "mon") {
        let sec = p.section(&format!("Свойства монитора  {}", s(m, "Name")));
        sec.row("Название", s(m, "Name"));
        sec.row("Производитель (код)", s(m, "Maker"));
        sec.row("Серийный номер", s(m, "Serial"));
        sec.row("Год выпуска", s(m, "Year"));
    }
    if p.sections.is_empty() {
        p.section("Монитор").row("Статус", "Нет данных");
    }
    p
}

fn audio(d: &Value) -> Page {
    let mut p = Page::new();
    let sec = p.section("Звуковые устройства");
    for a in list(d, "audio") {
        sec.row(&s(a, "Name"), s(a, "Manufacturer"));
    }
    p
}

fn pdisk(d: &Value) -> Page {
    let mut p = Page::new();
    let pd = list(d, "pdisk");
    for k in list(d, "disk") {
        let model = s(k, "Model");
        let sec = p.section(&format!("Физический диск #{}  {}", s(k, "Index"), model));
        sec.row("Модель", model.clone());
        if let Some(x) = n(k, "Size") {
            sec.row("Общий размер", rules::size(x as u64));
        }
        let extra = pd.iter().find(|x| s(x, "DeviceId") == s(k, "Index"));
        match extra {
            Some(e) => {
                sec.row("Тип носителя", s(e, "Media"));
                sec.row("Интерфейс", s(e, "Bus"));
                sec.row("Состояние", s(e, "Health"));
            }
            None => sec.row("Интерфейс", s(k, "InterfaceType")),
        }
        sec.row("Версия прошивки", s(k, "FirmwareRevision"));
        sec.row("Серийный номер", s(k, "SerialNumber"));
        sec.row("Разделы", s(k, "Partitions"));
        // Health counters, when the system gives them out.
        if let Some(h) = list(d, "rel").iter().find(|x| s(x, "DeviceId") == s(k, "Index")) {
            if let Some(x) = n(h, "Temperature").filter(|x| *x > 0.0) {
                sec.row("Температура", format!("{} °C", x as u32));
            }
            if let Some(x) = n(h, "Wear") {
                sec.row("Износ", format!("{} %", x as u32));
            }
            if let Some(x) = n(h, "PowerOnHours") {
                sec.row("Наработка", format!("{} ч", x as u64));
            }
            sec.row("Ошибки чтения", s(h, "ReadErrors"));
            sec.row("Ошибки записи", s(h, "WriteErrors"));
        } else if d.get("admin") == Some(&Value::Bool(false)) {
            sec.row("Подробности", "Температура и износ видны при запуске от имени администратора");
        }
    }
    p
}

/// A plain list page: one section "Title (N)", one row per item, the value
/// made of the non-empty parts.
fn listing(d: &Value, key: &str, title: &str, name: &str, parts: impl Fn(&Value) -> Vec<String>) -> Page {
    let mut p = Page::new();
    let items = list(d, key);
    let sec = p.section(&format!("{title} ({})", items.len()));
    for a in &items {
        let v: Vec<String> = parts(a).into_iter().filter(|x| !x.trim().is_empty()).collect();
        sec.row(&s(a, name), if v.is_empty() { "—".to_string() } else { v.join("  ·  ") });
    }
    p.sections.retain(|s| !s.rows.is_empty());
    p
}

fn battery(d: &Value) -> Page {
    let mut p = Page::new();
    let x = obj(d, "batx");
    for b in list(d, "bat") {
        let sec = p.section(&format!("Батарея  {}", s(b, "Name")).trim().to_string());
        sec.row("Заряд", unit(s(b, "EstimatedChargeRemaining"), "%"));
        sec.row(
            "Питание",
            match n(b, "BatteryStatus").unwrap_or(0.0) as u32 {
                1 | 4 | 5 => "Разряжается",
                3 => "Заряжена",
                6..=9 => "Заряжается",
                2 => "От сети",
                _ => "",
            },
        );
        // minutes; the system gives a huge number when it cannot tell
        if let Some(m) = n(b, "EstimatedRunTime").filter(|m| *m > 0.0 && *m < 6000.0) {
            sec.row("Осталось времени", format!("{} ч {} мин", m as u32 / 60, m as u32 % 60));
        }
        let unit_name = Some(s(x, "Unit")).filter(|u| !u.is_empty()).unwrap_or_else(|| "мВт·ч".into());
        let full = n(x, "Full").unwrap_or(0.0);
        let design = n(x, "Design").unwrap_or(0.0);
        if full > 0.0 {
            sec.row("Полная ёмкость", format!("{} {unit_name}", full as u64));
        }
        if design > 0.0 {
            sec.row("Заводская ёмкость", format!("{} {unit_name}", design as u64));
        }
        if full > 0.0 && design > 0.0 {
            sec.row("Износ", format!("{} %", (100.0 - full / design * 100.0).clamp(0.0, 100.0).round() as u32));
        }
        if let Some(c) = n(x, "Cycles").filter(|c| *c > 0.0) {
            sec.row("Циклов заряда", (c as u64).to_string());
        }
    }
    if p.sections.is_empty() {
        p.section("Батарея").row("Статус", "Батарея не обнаружена");
    }
    p
}

fn ldisk(d: &Value) -> Page {
    let mut p = Page::new();
    for v in list(d, "vol") {
        let total = n(v, "Size").unwrap_or(0.0);
        let free = n(v, "FreeSpace").unwrap_or(0.0);
        let sec = p.section(&format!("Диск {}  {}", s(v, "DeviceID"), s(v, "VolumeName")));
        sec.row("Метка тома", s(v, "VolumeName"));
        sec.row("Тип", if n(v, "DriveType") == Some(2.0) { "Съёмный диск" } else { "Локальный диск" });
        sec.row("Файловая система", s(v, "FileSystem"));
        sec.row("Общий размер", vol_mb(total));
        sec.row("Занято", vol_mb(total - free));
        sec.row("Свободно", vol_mb(free));
        if total > 0.0 {
            sec.row("Свободно %", format!("{} %", (free / total * 100.0).round() as u32));
        }
    }
    p
}

fn net(d: &Value) -> Page {
    let mut p = Page::new();
    for a in list(d, "net") {
        let sec = p.section(&format!("Сетевой адаптер  {}", s(a, "Name")));
        sec.row("Производитель", s(a, "Manufacturer"));
        sec.row("MAC-адрес", s(a, "MACAddress"));
        if let Some(x) = n(a, "Speed").filter(|x| *x > 0.0 && *x < 1e15) {
            sec.row("Скорость", format!("{} Мбит/с", (x / 1e6) as u64));
        }
        match a.get("NetEnabled") {
            Some(Value::Bool(true)) => sec.row("Подключён", "Да"),
            Some(Value::Bool(false)) => sec.row("Подключён", "Нет"),
            _ => {}
        }
    }
    for a in list(d, "ip") {
        let sec = p.section(&format!("Адреса  {}", s(a, "Description")));
        sec.row("IP-адреса", s(a, "IPAddress"));
        sec.row("Шлюз", s(a, "DefaultIPGateway"));
        sec.row("DNS", s(a, "DNSServerSearchOrder"));
    }
    p.sections.retain(|s| !s.rows.is_empty());
    p
}

/// Text report of one page.
pub fn report(title: &str, p: &Page) -> String {
    let mut out = format!("--------[ {title} ]--------\r\n\r\n");
    for s in &p.sections {
        out.push_str(&format!("    {}:\r\n", s.title));
        for (k, v) in &s.rows {
            // Values line up in one column; a long name still keeps a gap.
            let gap = 34usize.saturating_sub(k.chars().count()).max(2);
            out.push_str(&format!("      {k}{}{v}\r\n", " ".repeat(gap)));
        }
        out.push_str("\r\n");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn report_keeps_a_gap_after_long_names() {
        let long = "x".repeat(50);
        let p = Page {
            sections: vec![Section { title: "T".into(), rows: vec![("Имя".into(), "a".into()), (long.clone(), "b".into())] }],
        };
        let r = report("P", &p);
        assert!(r.contains(&format!("      Имя{}a\r\n", " ".repeat(31))));
        assert!(r.contains(&format!("      {long}  b\r\n")));
    }

    #[test]
    fn script_is_plain_ascii() {
        // It reaches PowerShell through its input, which has no fixed encoding.
        assert!(SCRIPT.is_ascii());
    }

    #[test]
    fn script_splits_into_two_whole_scripts() {
        let [first, second] = script_halves();
        for half in [&first, &second] {
            assert!(half.starts_with("$ErrorActionPreference"));
            assert!(half.contains("$r = [ordered]@{}"));
            assert!(half.trim_end().ends_with("ConvertTo-Json -Depth 5 -Compress"));
        }
        assert!(first.contains("Win32_Processor") && !first.contains("Win32_Printer"));
        assert!(second.contains("Win32_Printer") && !second.contains("Win32_Processor"));
    }

    #[test]
    fn new_pages() {
        let d = json!({
            "admin": false,
            "disk": [{ "Model": "SSD", "Index": 0, "Size": 1000 }],
            "apps": [
                { "Name": "beta", "SizeKb": 10, "Date": "20260101", "Publisher": "Zed" },
                { "Name": "Alpha", "SizeKb": 500, "Date": "20250101", "Store": true },
                { "Name": "Gamma", "SizeKb": 50, "Date": "20260601", "Publisher": "Acme" },
            ],
            "services": [{ "DisplayName": "Print Spooler", "State": "Running" }],
            "printers": [{ "Name": "PDF", "DriverName": "Drv", "PortName": "P:", "Default": true }],
            "bat": [{ "Name": "BAT0", "EstimatedChargeRemaining": 80, "BatteryStatus": 1, "EstimatedRunTime": 125 }],
            "batx": { "Full": 40000, "Design": 50000, "Cycles": 120 },
        });
        let rows = |id: &str, sort: AppSort| -> Vec<(String, String)> {
            page_sorted(id, &d, sort).sections.into_iter().flat_map(|s| s.rows).collect()
        };
        let names = |sort| rows("apps", sort).into_iter().map(|r| r.0).collect::<Vec<_>>();
        assert_eq!(names(AppSort::Name), ["Alpha", "beta", "Gamma"]);
        assert_eq!(names(AppSort::Size), ["Alpha", "Gamma", "beta"]);
        assert_eq!(names(AppSort::Date), ["Gamma", "beta", "Alpha"]);
        assert_eq!(names(AppSort::Publisher), ["Gamma", "beta", "Alpha"]);
        assert!(rows("apps", AppSort::Name)[0].1.ends_with("Microsoft Store"));
        assert_eq!(rows("services", AppSort::Name), [("Print Spooler".to_string(), "Работает".to_string())]);
        assert_eq!(rows("printers", AppSort::Name)[0].1, "Drv  ·  P:  ·  основной");
        let bat = rows("battery", AppSort::Name);
        assert!(bat.contains(&("Заряд".into(), "80 %".into())));
        assert!(bat.contains(&("Питание".into(), "Разряжается".into())));
        assert!(bat.contains(&("Осталось времени".into(), "2 ч 5 мин".into())));
        assert!(bat.contains(&("Износ".into(), "20 %".into())));
        assert!(rows("pdisk", AppSort::Name).iter().any(|r| r.0 == "Подробности"));
        assert!(page("usb", &d).sections.is_empty());
        assert_eq!(rows("battery", AppSort::Name).len(), 7);
    }

    #[test]
    fn one_line_summary() {
        let d = json!({
            "cs": { "Name": "ANNA-PC", "UserName": "anna" },
            "os": { "Caption": "Windows 11 Pro" },
            "osx": { "DisplayVersion": "24H2" },
            "cpu": [{ "Name": "AMD Ryzen 5 5600X   6-Core Processor ", "NumberOfCores": 6, "NumberOfLogicalProcessors": 12 }],
            "gpu": [{ "Name": "RTX 3060", "AdapterRAM": 12884901888u64 }],
            "mem": [{ "Capacity": 17179869184u64, "SMBIOSMemoryType": 26 }, { "Capacity": 17179869184u64, "SMBIOSMemoryType": 26 }],
            "disk": [{ "Model": "CT500P3SSD8", "Size": 500107862016u64, "InterfaceType": "SCSI", "Index": 0 }],
            "pdisk": [{ "DeviceId": "0", "Bus": "NVMe" }],
        });
        assert_eq!(
            summary_line(&d),
            "AMD Ryzen 5 5600X 6-Core Processor (6C/12T) / RTX 3060 12 ГБ / 32 ГБ DDR4 / CT500P3SSD8 465.8 ГБ NVMe / Windows 11 Pro 24H2"
        );
        assert!(!summary_line(&d).contains("ANNA") && !summary_line(&d).contains("anna"));
        assert_eq!(summary_line(&json!({})), "");
    }

    #[test]
    fn missing_numbers_leave_no_rows() {
        // What Linux gives: a resolution but no colour depth or refresh rate, no SMBIOS version.
        let d = json!({
            "gpu": [{ "Name": "GPU", "CurrentHorizontalResolution": 1920, "CurrentVerticalResolution": 1080 }],
            "bios": { "Manufacturer": "AMI" },
            "cpu": [{ "Name": "A", "L2CacheSize": 512 }, { "Name": "B", "L2CacheSize": 512 }],
            "net": [{ "Name": "eth", "NetEnabled": true }],
        });
        let rows = |id: &str| -> Vec<(String, String)> {
            page(id, &d).sections.into_iter().flat_map(|s| s.rows).collect()
        };
        assert!(rows("gpu").iter().all(|(k, _)| k != "Глубина цвета" && k != "Частота обновления"));
        assert!(rows("bios").iter().all(|(k, _)| k != "Версия SMBIOS"));
        assert!(rows("summary").contains(&("Тип BIOS".into(), "AMI".into())));
        assert!(rows("net").contains(&("Подключён".into(), "Да".into())));
        // two processors: one cache section, sizes added up
        let cpu = page("cpu", &d);
        assert_eq!(cpu.sections.iter().filter(|s| s.title == "Кэш").count(), 1);
        assert!(rows("cpu").contains(&("Кэш L2".into(), "1024 КБ".into())));
    }
}
