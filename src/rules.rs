//! Display rules: how sizes and the one-line descriptions on the summary page
//! are written. Parts of a line are joined with " · ", like on the programs page.

use std::sync::atomic::{AtomicBool, Ordering};

const SEP: &str = "  ·  ";

/// A number with one decimal, without a trailing ".0": 8, 465.8.
fn num(x: f64) -> String {
    let s = format!("{x:.1}");
    s.strip_suffix(".0").map(str::to_string).unwrap_or(s)
}

/// Disk sizes are counted the way the box says (1 ГБ = 1000 МБ) when this
/// is on, otherwise the way Windows counts them (1 ГБ = 1024 МБ).
static DECIMAL: AtomicBool = AtomicBool::new(false);

pub fn set_decimal(on: bool) {
    DECIMAL.store(on, Ordering::Relaxed);
}

pub fn decimal() -> bool {
    DECIMAL.load(Ordering::Relaxed)
}

/// Runs `f` with Windows-style units whatever the person chose: the data
/// two computers are compared by must not depend on a setting.
pub fn with_binary<T>(f: impl FnOnce() -> T) -> T {
    let was = decimal();
    set_decimal(false);
    let out = f();
    set_decimal(was);
    out
}

/// A size in the largest unit that fits, with `step` units in the next one.
fn size_in(bytes: u64, step: f64) -> String {
    let kb = bytes as f64 / step;
    let mb = kb / step;
    let gb = mb / step;
    if mb < 1.0 {
        format!("{} КБ", kb.round() as u64)
    } else if mb < 1000.0 {
        format!("{} МБ", mb.round() as u64)
    } else if gb < 1000.0 {
        format!("{} ГБ", num(gb))
    } else {
        format!("{} ТБ", num(gb / step))
    }
}

/// A disk or file size: "512 МБ", "465.8 ГБ", "1.8 ТБ" — or "500.1 ГБ", "2 ТБ"
/// when the person chose the units printed on the box.
pub fn size(bytes: u64) -> String {
    size_in(bytes, if decimal() { 1000.0 } else { 1024.0 })
}

/// A memory size: always 1 ГБ = 1024 МБ, nobody counts memory any other way.
pub fn mem_size(bytes: u64) -> String {
    size_in(bytes, 1024.0)
}

/// A transfer speed: "300 КБ/с", "1.2 ГБ/с".
pub fn rate(bytes_per_sec: f64) -> String {
    format!("{}/с", size(bytes_per_sec.max(0.0) as u64))
}

/// "16 ГБ  ·  DDR4"; "Неизвестно" when the size is not known.
pub fn memory_line(bytes: u64, mem_type: &str) -> String {
    if bytes == 0 {
        return "Неизвестно".into();
    }
    join(&[mem_size(bytes), mem_type.to_string()])
}

/// "AMD Ryzen 5 5600X  ·  6C/12T  ·  3.7 ГГц": name, cores and threads, clock.
/// Several processors get "2 × " in front.
pub fn cpu_line(name: &str, cores: u32, threads: u32, sockets: u32, clock_mhz: u32) -> String {
    let mut name = name.trim().to_string();
    if sockets > 1 {
        name = format!("{sockets} × {name}");
    }
    let count = match (cores, threads) {
        (0, _) => String::new(),
        (c, t) if t > c => format!("{c}C/{t}T"),
        (c, _) => format!("{c}C"),
    };
    let clock = if clock_mhz > 0 { format!("{} ГГц", num(clock_mhz as f64 / 1000.0)) } else { String::new() };
    join(&[name, count, clock])
}

/// "CT500P3SSD8  ·  465.8 ГБ  ·  NVMe".
pub fn disk_line(model: &str, bytes: u64, bus: &str) -> String {
    let size = if bytes > 0 { size(bytes) } else { String::new() };
    join(&[model.trim().to_string(), size, bus.trim().to_string()])
}

/// "свободно 14.8 ГБ из 150.3 ГБ".
pub fn space_line(total_bytes: u64, free_bytes: u64) -> String {
    format!("свободно {} из {}", size(free_bytes), size(total_bytes))
}

/// "Dell U2419H  ·  24\"": name and diagonal. The serial number and the year
/// are on the monitor page.
pub fn monitor_line(name: &str, diag_inch: Option<u32>) -> String {
    let diag = diag_inch.filter(|d| *d > 0).map(|d| format!("{d}\"")).unwrap_or_default();
    join(&[name.trim().to_string(), diag])
}

/// One memory module: "8 ГБ  ·  DDR4-3200  ·  Kingston KF432C16BB/8".
pub fn dimm_line(bytes: u64, mem_type: &str, speed: u32, maker: &str, part: &str) -> String {
    let size = if bytes > 0 { mem_size(bytes) } else { String::new() };
    let grade = match (mem_type.is_empty(), speed) {
        (true, 0) => String::new(),
        (true, s) => format!("{s} MT/s"),
        (false, 0) => mem_type.to_string(),
        (false, s) => format!("{mem_type}-{s}"),
    };
    let id = [maker.trim(), part.trim()]
        .iter()
        .filter(|s| !s.is_empty() && !s.eq_ignore_ascii_case("unknown"))
        .cloned()
        .collect::<Vec<_>>()
        .join(" ");
    join(&[size, grade, id])
}

/// The non-empty parts, joined.
fn join(parts: &[String]) -> String {
    parts.iter().filter(|p| !p.is_empty()).cloned().collect::<Vec<_>>().join(SEP)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes() {
        assert_eq!(size(300 * 1024), "300 КБ");
        assert_eq!(size(512 << 20), "512 МБ");
        assert_eq!(size(8 << 30), "8 ГБ");
        assert_eq!(size(500_107_862_016), "465.8 ГБ");
        assert_eq!(size(2_000_398_934_016), "1.8 ТБ");
        // the units printed on the box
        assert_eq!(size_in(500_107_862_016, 1000.0), "500.1 ГБ");
        assert_eq!(size_in(2_000_398_934_016, 1000.0), "2 ТБ");
        assert_eq!(size_in(512_000_000, 1000.0), "512 МБ");
        assert_eq!(mem_size(16 << 30), "16 ГБ");
        assert_eq!(with_binary(|| size(8 << 30)), "8 ГБ");
    }

    #[test]
    fn lines() {
        assert_eq!(memory_line(16 << 30, "DDR4"), "16 ГБ  ·  DDR4");
        assert_eq!(memory_line(0, "DDR4"), "Неизвестно");
        assert_eq!(cpu_line(" AMD Ryzen 5 5600X ", 6, 12, 1, 3701), "AMD Ryzen 5 5600X  ·  6C/12T  ·  3.7 ГГц");
        assert_eq!(cpu_line("Xeon", 8, 8, 2, 0), "2 × Xeon  ·  8C");
        assert_eq!(cpu_line("CPU", 0, 0, 1, 0), "CPU");
        assert_eq!(disk_line("CT500P3SSD8", 500_107_862_016, "NVMe"), "CT500P3SSD8  ·  465.8 ГБ  ·  NVMe");
        assert_eq!(disk_line("Disk", 0, ""), "Disk");
        assert_eq!(space_line(150 << 30, 15 << 30), "свободно 15 ГБ из 150 ГБ");
        assert_eq!(monitor_line("Dell U2419H", Some(24)), "Dell U2419H  ·  24\"");
        assert_eq!(monitor_line("Dell U2419H", None), "Dell U2419H");
        assert_eq!(dimm_line(8 << 30, "DDR4", 3200, "Kingston", "KF432"), "8 ГБ  ·  DDR4-3200  ·  Kingston KF432");
        assert_eq!(dimm_line(8 << 30, "", 0, "Unknown", ""), "8 ГБ");
    }
}
