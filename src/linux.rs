//! Linux: the same information the Windows script gathers, read from /proc,
//! /sys and a few standard commands (df, ip, lspci, date), and returned in the
//! same shape, so every page works unchanged. Nothing here needs root; what
//! only root may read (memory modules, serial numbers) is simply left out.
//!
//! The text parsers are plain functions so they can be tested on any system.
#![cfg_attr(windows, allow(dead_code))]

use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

// ---------- small helpers ----------

fn read(p: impl AsRef<Path>) -> String {
    fs::read_to_string(p).map(|s| s.trim().to_string()).unwrap_or_default()
}

fn run(prog: &str, args: &[&str]) -> Option<String> {
    let out = Command::new(prog).args(args).output().ok()?;
    if !out.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&out.stdout).into_owned())
}

fn dirs(p: impl AsRef<Path>) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = fs::read_dir(p)
        .map(|it| it.filter_map(|e| e.ok().map(|e| e.path())).collect())
        .unwrap_or_default();
    v.sort();
    v
}

fn file_name(p: &Path) -> String {
    p.file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default()
}

/// Firmware fields that only hold a placeholder count as empty.
pub fn clean(v: &str) -> String {
    let l = v.trim().to_ascii_lowercase();
    let junk = [
        "", "to be filled by o.e.m.", "default string", "not specified", "not applicable",
        "none", "system product name", "system manufacturer", "o.e.m.", "unknown",
    ];
    if junk.contains(&l.as_str()) { String::new() } else { v.trim().to_string() }
}

fn dmi(k: &str) -> String {
    clean(&read(format!("/sys/class/dmi/id/{k}")))
}

/// "0x10de" → a short vendor name.
pub fn pci_vendor(id: &str) -> &'static str {
    match id.trim().to_ascii_lowercase().trim_start_matches("0x") {
        "10de" => "NVIDIA",
        "1002" | "1022" => "AMD",
        "8086" => "Intel",
        "10ec" => "Realtek",
        "14e4" => "Broadcom",
        "168c" | "1969" | "17cb" => "Qualcomm Atheros",
        "14c3" => "MediaTek",
        "15b3" => "Mellanox",
        "1af4" => "Red Hat (virtio)",
        "15ad" => "VMware",
        "80ee" => "VirtualBox",
        "1414" => "Microsoft",
        "1b36" => "QEMU",
        "5143" => "Qualcomm",
        _ => "",
    }
}

/// Quoted fields of one `lspci -mm` line.
pub fn lspci_fields(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut inside = false;
    for c in line.chars() {
        match c {
            '"' if inside => {
                out.push(std::mem::take(&mut cur));
                inside = false;
            }
            '"' => inside = true,
            _ if inside => cur.push(c),
            _ => {}
        }
    }
    out
}

/// The device's marketing name from lspci ("GA106 [GeForce RTX 3060]" → "GeForce RTX 3060").
fn lspci_name(slot: &str) -> Option<(String, String)> {
    let text = run("lspci", &["-mm", "-s", slot])?;
    let f = lspci_fields(text.lines().next()?);
    if f.len() < 3 {
        return None;
    }
    let dev = &f[2];
    let short = match (dev.find('['), dev.rfind(']')) {
        (Some(a), Some(b)) if a < b => dev[a + 1..b].to_string(),
        _ => dev.clone(),
    };
    Some((f[1].clone(), short))
}

// ---------- parsers ----------

/// /proc/meminfo → kB values.
pub fn meminfo(text: &str) -> BTreeMap<String, u64> {
    text.lines()
        .filter_map(|l| {
            let (k, v) = l.split_once(':')?;
            let n = v.split_whitespace().next()?.parse().ok()?;
            Some((k.trim().to_string(), n))
        })
        .collect()
}

/// /etc/os-release → key/value.
pub fn os_release(text: &str) -> BTreeMap<String, String> {
    text.lines()
        .filter_map(|l| {
            let (k, v) = l.split_once('=')?;
            Some((k.trim().to_string(), v.trim().trim_matches('"').trim_matches('\'').to_string()))
        })
        .collect()
}

/// /proc/cpuinfo → one record per processor package.
pub fn cpus(text: &str) -> Vec<Value> {
    let mut packages: BTreeMap<String, Vec<BTreeMap<String, String>>> = BTreeMap::new();
    let mut global = BTreeMap::new(); // ARM keeps "Hardware" outside the blocks
    for block in text.split("\n\n") {
        let mut m = BTreeMap::new();
        for l in block.lines() {
            if let Some((k, v)) = l.split_once(':') {
                m.insert(k.trim().to_string(), v.trim().to_string());
            }
        }
        if m.contains_key("processor") && m.get("processor").map_or(false, |p| p.parse::<u32>().is_ok()) {
            let pkg = m.get("physical id").cloned().unwrap_or_else(|| "0".into());
            packages.entry(pkg).or_default().push(m);
        } else {
            global.extend(m);
        }
    }
    packages
        .values()
        .map(|threads| {
            let f = &threads[0];
            let get = |k: &str| f.get(k).or_else(|| global.get(k)).cloned().unwrap_or_default();
            let name = ["model name", "Processor", "cpu model", "Hardware"]
                .iter()
                .map(|k| get(k))
                .find(|v| !v.is_empty())
                .unwrap_or_else(|| "CPU".into());
            let vendor = match get("vendor_id").as_str() {
                "GenuineIntel" => "Intel".to_string(),
                "AuthenticAMD" => "AMD".to_string(),
                v => v.to_string(),
            };
            let core_ids: BTreeSet<String> = threads.iter().filter_map(|t| t.get("core id").cloned()).collect();
            let cores = get("cpu cores")
                .parse::<u64>()
                .ok()
                .or(if core_ids.is_empty() { None } else { Some(core_ids.len() as u64) })
                .unwrap_or(threads.len() as u64);
            let flags = get("flags");
            let fl: BTreeSet<&str> = flags.split_whitespace().collect();
            let mut c = json!({
                "Name": name,
                "Manufacturer": vendor,
                "NumberOfCores": cores,
                "NumberOfLogicalProcessors": threads.len(),
            });
            if let Ok(mhz) = get("cpu MHz").parse::<f64>() {
                c["CurrentClockSpeed"] = json!(mhz.round() as u64);
                c["MaxClockSpeed"] = json!(mhz.round() as u64);
            }
            let fam = get("cpu family");
            if !fam.is_empty() {
                c["Description"] =
                    json!(format!("Family {} Model {} Stepping {}", fam, get("model"), get("stepping")));
            }
            if !fl.is_empty() {
                c["AddressWidth"] = json!(if fl.contains("lm") { 64 } else { 32 });
                c["VirtualizationFirmwareEnabled"] = json!(fl.contains("vmx") || fl.contains("svm"));
            } else if cfg!(target_pointer_width = "64") {
                c["AddressWidth"] = json!(64);
            }
            c
        })
        .collect()
}

/// Cache sizes "32K", "1024K", "16M" → kB.
pub fn size_kb(s: &str) -> u64 {
    let s = s.trim();
    let (num, mul) = match s.chars().last() {
        Some('K') => (&s[..s.len() - 1], 1),
        Some('M') => (&s[..s.len() - 1], 1024),
        Some('G') => (&s[..s.len() - 1], 1024 * 1024),
        _ => (s, 1),
    };
    num.trim().parse::<u64>().unwrap_or(0) * mul
}

/// Monitor identity from its EDID block.
pub fn edid(b: &[u8]) -> Option<Value> {
    if b.len() < 128 || b[..8] != [0, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0] {
        return None;
    }
    let id = u16::from_be_bytes([b[8], b[9]]);
    let maker: String = [(id >> 10) & 31, (id >> 5) & 31, id & 31]
        .iter()
        .map(|&c| char::from(b'@' + c as u8))
        .collect();
    let mut name = String::new();
    let mut serial = String::new();
    for i in 0..4 {
        let o = 54 + 18 * i;
        if b[o] == 0 && b[o + 1] == 0 && b[o + 2] == 0 {
            let text: String = b[o + 5..o + 18]
                .iter()
                .take_while(|&&c| c != 0x0A)
                .map(|&c| c as char)
                .collect::<String>()
                .trim()
                .to_string();
            match b[o + 3] {
                0xFC => name = text,
                0xFF => serial = text,
                _ => {}
            }
        }
    }
    if name.is_empty() {
        name = format!("{maker}{:04X}", u16::from_le_bytes([b[10], b[11]]));
    }
    let mut m = json!({ "Name": name, "Maker": maker, "Serial": serial });
    if b[17] != 0 && b[17] != 0xFF {
        m["Year"] = json!(1990 + b[17] as u32);
    }
    if b[21] > 0 && b[22] > 0 {
        m["WidthCm"] = json!(b[21]);
        m["HeightCm"] = json!(b[22]);
    }
    Some(m)
}

/// /proc/asound/cards → sound devices.
pub fn sound_cards(text: &str) -> Vec<Value> {
    text.lines()
        .filter_map(|l| {
            let t = l.trim_start();
            let first = t.split_whitespace().next()?;
            first.parse::<u32>().ok()?;
            let (_, rest) = t.split_once("]: ")?;
            let (driver, name) = rest.split_once(" - ").unwrap_or((rest, rest));
            Some(json!({ "Name": name.trim(), "Manufacturer": driver.trim() }))
        })
        .collect()
}

/// `df -B1 --output=source,fstype,size,avail,target` → volumes on real disks.
pub fn df_volumes(text: &str) -> Vec<Value> {
    let skip = ["squashfs", "tmpfs", "devtmpfs", "overlay", "iso9660", "efivarfs"];
    let mut seen = BTreeSet::new();
    let mut out = Vec::new();
    for l in text.lines().skip(1) {
        let f: Vec<&str> = l.split_whitespace().collect();
        if f.len() < 5 || !f[0].starts_with("/dev/") || skip.contains(&f[1]) {
            continue;
        }
        if !seen.insert(f[0].to_string()) {
            continue; // the same partition mounted twice (btrfs subvolumes...)
        }
        let target = f[4..].join(" ");
        let removable = target.starts_with("/media/") || target.starts_with("/run/media/");
        out.push(json!({
            "DeviceID": target,
            "VolumeName": f[0].trim_start_matches("/dev/"),
            "FileSystem": f[1],
            "Size": f[2].parse::<u64>().unwrap_or(0),
            "FreeSpace": f[3].parse::<u64>().unwrap_or(0),
            "DriveType": if removable { 2 } else { 3 },
        }));
    }
    out
}

/// A .desktop file → (name, comment) for programs shown in menus.
pub fn desktop_entry(text: &str) -> Option<(String, String)> {
    let mut inside = false;
    let (mut name, mut comment, mut app) = (String::new(), String::new(), false);
    for l in text.lines() {
        let l = l.trim();
        if l.starts_with('[') {
            inside = l == "[Desktop Entry]";
            continue;
        }
        if !inside {
            continue;
        }
        match l.split_once('=') {
            Some(("Type", v)) => app = v.trim() == "Application",
            Some(("Name", v)) => name = v.trim().to_string(),
            Some(("Comment", v)) => comment = v.trim().to_string(),
            Some(("NoDisplay", v)) | Some(("Hidden", v)) if v.trim() == "true" => return None,
            _ => {}
        }
    }
    if app && !name.is_empty() { Some((name, comment)) } else { None }
}

/// "12/25/2023" (the firmware's order) → "25.12.2023".
pub fn bios_date(s: &str) -> String {
    let p: Vec<&str> = s.split('/').collect();
    if p.len() == 3 { format!("{}.{}.{}", p[1], p[0], p[2]) } else { s.to_string() }
}

/// The chassis type from the firmware → the Windows PC system type numbers the pages use.
pub fn pc_system_type(chassis: &str) -> u32 {
    match chassis.trim().parse::<u32>().unwrap_or(0) {
        3..=7 | 13 | 15 | 16 | 24 | 35 | 36 => 1,
        8..=12 | 14 | 30..=32 => 2,
        17 | 23 | 28 | 29 => 4,
        _ => 0,
    }
}

// ---------- collection ----------

fn date(args: &[&str]) -> String {
    run("date", args).map(|s| s.trim().to_string()).unwrap_or_default()
}

fn caches() -> Vec<Value> {
    let mut seen = BTreeSet::new();
    let mut sums = [0u64; 3];
    for cpu in dirs("/sys/devices/system/cpu") {
        let name = file_name(&cpu);
        if !name.starts_with("cpu") || !name[3..].chars().all(|c| c.is_ascii_digit()) {
            continue;
        }
        for idx in dirs(cpu.join("cache")) {
            let level: usize = read(idx.join("level")).parse().unwrap_or(0);
            let key = (level, read(idx.join("type")), read(idx.join("shared_cpu_list")));
            if (1..=3).contains(&level) && seen.insert(key) {
                sums[level - 1] += size_kb(&read(idx.join("size")));
            }
        }
    }
    sums.iter()
        .enumerate()
        .filter(|(_, s)| **s > 0)
        .map(|(i, s)| json!({ "Level": i + 3, "InstalledSize": s }))
        .collect()
}

fn gpus_and_monitors() -> (Vec<Value>, Vec<Value>) {
    let mut gpus = Vec::new();
    let mut mons = Vec::new();
    let mut slots = BTreeSet::new();
    for card in dirs("/sys/class/drm") {
        let cname = file_name(&card);
        let is_card = cname.starts_with("card") && cname[4..].chars().all(|c| c.is_ascii_digit());
        if is_card {
            let dev = card.join("device");
            let slot = fs::canonicalize(&dev).map(|p| file_name(&p)).unwrap_or_default();
            if slot.is_empty() || !slots.insert(slot.clone()) {
                continue;
            }
            let vid = read(dev.join("vendor"));
            let did = read(dev.join("device"));
            let vendor = pci_vendor(&vid).to_string();
            let (vendor_long, model) = lspci_name(&slot).unwrap_or_default();
            let name = if model.is_empty() {
                format!("{} GPU [{}:{}]", if vendor.is_empty() { "PCI" } else { &vendor }, vid, did)
            } else if !vendor.is_empty() && !model.starts_with(&vendor) {
                format!("{vendor} {model}")
            } else {
                model.clone()
            };
            let driver = fs::read_link(dev.join("driver")).map(|p| file_name(&p)).unwrap_or_default();
            let mut ver = read(format!("/sys/module/{driver}/version"));
            if ver.is_empty() {
                ver = read("/proc/sys/kernel/osrelease");
            }
            let mut g = json!({
                "Name": name,
                "AdapterCompatibility": if vendor.is_empty() { vendor_long } else { vendor },
                "VideoProcessor": model,
                "DriverVersion": if driver.is_empty() { ver } else { format!("{driver} {ver}") },
                "PNPDeviceID": format!("PCI {}:{} ({slot})", vid.trim_start_matches("0x"), did.trim_start_matches("0x")).to_uppercase(),
            });
            if let Ok(v) = read(dev.join("mem_info_vram_total")).parse::<u64>() {
                g["AdapterRAM"] = json!(v);
            }
            // first connected screen: its current mode
            for conn in dirs("/sys/class/drm") {
                let n = file_name(&conn);
                if n.starts_with(&format!("{cname}-")) && read(conn.join("status")) == "connected" {
                    if let Some((w, h)) = read(conn.join("modes")).lines().next().and_then(|m| m.split_once('x')) {
                        let h: String = h.chars().take_while(|c| c.is_ascii_digit()).collect();
                        g["CurrentHorizontalResolution"] = json!(w.parse::<u32>().unwrap_or(0));
                        g["CurrentVerticalResolution"] = json!(h.parse::<u32>().unwrap_or(0));
                    }
                    break;
                }
            }
            gpus.push(g);
        } else if cname.starts_with("card") && cname.contains('-') && read(card.join("status")) == "connected" {
            if let Some(m) = fs::read(card.join("edid")).ok().and_then(|b| edid(&b)) {
                mons.push(m);
            }
        }
    }
    (gpus, mons)
}

fn disks() -> (Vec<Value>, Vec<Value>) {
    let mut disk = Vec::new();
    let mut pdisk = Vec::new();
    let skip = ["loop", "ram", "zram", "dm-", "md", "sr", "fd", "nbd"];
    let mut index = 0;
    for b in dirs("/sys/block") {
        let name = file_name(&b);
        if skip.iter().any(|p| name.starts_with(p)) {
            continue;
        }
        let bytes = read(b.join("size")).parse::<u64>().unwrap_or(0) * 512;
        if bytes == 0 {
            continue;
        }
        let path = fs::canonicalize(&b).map(|p| p.to_string_lossy().into_owned()).unwrap_or_default();
        let nvme = name.starts_with("nvme");
        let usb = path.contains("/usb");
        let (bus, iface, pnp) = if nvme {
            ("NVMe", "NVMe", format!("NVME\\{name}"))
        } else if usb {
            ("USB", "USB", format!("USBSTOR\\{name}"))
        } else if name.starts_with("mmcblk") {
            ("SD", "SD", format!("SD\\{name}"))
        } else {
            ("SATA", "SCSI", format!("SCSI\\{name}"))
        };
        let dev = b.join("device");
        let vendor = clean(&read(dev.join("vendor")));
        let mut model = clean(&read(dev.join("model")));
        if model.is_empty() {
            model = clean(&read(dev.join("name"))); // SD cards
        }
        if !vendor.is_empty() && !nvme && !model.to_lowercase().starts_with(&vendor.to_lowercase()) && vendor != "ATA" {
            model = format!("{vendor} {model}");
        }
        if model.is_empty() {
            model = name.clone();
        }
        let parts = dirs(&b).iter().filter(|p| file_name(p).starts_with(&name)).count();
        let mut fw = read(dev.join("firmware_rev"));
        if fw.is_empty() {
            fw = read(dev.join("rev"));
        }
        disk.push(json!({
            "Model": model,
            "Size": bytes,
            "InterfaceType": iface,
            "SerialNumber": read(dev.join("serial")),
            "FirmwareRevision": fw,
            "Index": index,
            "Partitions": parts,
            "PNPDeviceID": pnp,
        }));
        let media = match read(b.join("queue/rotational")).as_str() {
            "1" => "HDD",
            "0" => "SSD",
            _ => "",
        };
        pdisk.push(json!({ "DeviceId": index.to_string(), "Media": media, "Bus": bus }));
        index += 1;
    }
    (disk, pdisk)
}

fn network() -> (Vec<Value>, Vec<Value>) {
    let mut net = Vec::new();
    let mut names = BTreeMap::new();
    for n in dirs("/sys/class/net") {
        let iface = file_name(&n);
        let dev = n.join("device");
        if iface == "lo" || !dev.exists() {
            continue; // only real adapters
        }
        let slot = fs::canonicalize(&dev).map(|p| file_name(&p)).unwrap_or_default();
        let maker = pci_vendor(&read(dev.join("vendor"))).to_string();
        let model = lspci_name(&slot).map(|x| x.1).unwrap_or_default();
        let label = if model.is_empty() { iface.clone() } else { format!("{model} ({iface})") };
        names.insert(iface.clone(), label.clone());
        let mut a = json!({
            "Name": label,
            "Manufacturer": maker,
            "MACAddress": read(n.join("address")).to_uppercase(),
            "NetEnabled": read(n.join("operstate")) == "up",
        });
        if let Ok(sp) = read(n.join("speed")).parse::<i64>() {
            if sp > 0 {
                a["Speed"] = json!(sp as u64 * 1_000_000);
            }
        }
        net.push(a);
    }

    let mut ips = Vec::new();
    let gateways: BTreeMap<String, String> = run("ip", &["-j", "route", "show", "default"])
        .and_then(|t| serde_json::from_str::<Value>(&t).ok())
        .and_then(|v| v.as_array().cloned())
        .unwrap_or_default()
        .iter()
        .filter_map(|r| Some((r["dev"].as_str()?.to_string(), r["gateway"].as_str()?.to_string())))
        .collect();
    let dns: Vec<String> = fs::read_to_string("/etc/resolv.conf")
        .unwrap_or_default()
        .lines()
        .filter_map(|l| l.trim().strip_prefix("nameserver").map(|x| x.trim().to_string()))
        .collect();
    if let Some(Value::Array(list)) = run("ip", &["-j", "addr"]).and_then(|t| serde_json::from_str(&t).ok()) {
        for i in list {
            let iface = i["ifname"].as_str().unwrap_or_default().to_string();
            if iface == "lo" || !names.contains_key(&iface) {
                continue;
            }
            let mut addrs: Vec<(bool, String)> = i["addr_info"]
                .as_array()
                .map(|a| {
                    a.iter()
                        .filter_map(|x| Some((x["family"].as_str()? != "inet", x["local"].as_str()?.to_string())))
                        .collect()
                })
                .unwrap_or_default();
            addrs.sort(); // IPv4 first
            if addrs.is_empty() {
                continue;
            }
            ips.push(json!({
                "Description": names[&iface],
                "IPAddress": addrs.into_iter().map(|a| a.1).collect::<Vec<_>>(),
                "DefaultIPGateway": gateways.get(&iface).cloned().map(|g| vec![g]).unwrap_or_default(),
                "DNSServerSearchOrder": dns,
            }));
        }
    }
    (net, ips)
}

fn apps() -> Vec<Value> {
    let home = std::env::var("HOME").unwrap_or_default();
    let places = [
        "/usr/share/applications".to_string(),
        "/usr/local/share/applications".to_string(),
        "/var/lib/flatpak/exports/share/applications".to_string(),
        "/var/lib/snapd/desktop/applications".to_string(),
        format!("{home}/.local/share/applications"),
        format!("{home}/.local/share/flatpak/exports/share/applications"),
    ];
    let mut found: BTreeMap<String, (String, String)> = BTreeMap::new();
    for dir in places {
        for f in dirs(&dir) {
            if f.extension().map_or(true, |e| e != "desktop") {
                continue;
            }
            if let Some((name, comment)) = fs::read_to_string(&f).ok().and_then(|t| desktop_entry(&t)) {
                found.entry(name.to_lowercase()).or_insert((name, comment));
            }
        }
    }
    found.into_values().map(|(n, c)| json!({ "Name": n, "Version": "", "Publisher": c })).collect()
}

/// One key of a .desktop file's main section ("Exec").
pub fn desktop_key(text: &str, key: &str) -> String {
    let mut inside = false;
    for l in text.lines() {
        let l = l.trim();
        if l.starts_with('[') {
            inside = l == "[Desktop Entry]";
        } else if inside {
            if let Some((k, v)) = l.split_once('=') {
                if k.trim() == key {
                    return v.trim().to_string();
                }
            }
        }
    }
    String::new()
}

/// Laptop batteries from /sys/class/power_supply, and their capacity figures.
fn batteries() -> (Vec<Value>, Value) {
    let mut bats = Vec::new();
    let (mut full, mut design, mut cycles) = (0u64, 0u64, 0u64);
    let mut unit = "мВт·ч";
    for p in dirs("/sys/class/power_supply") {
        // "Device" scope is a mouse or a headset, not the computer's own battery
        if read(p.join("type")) != "Battery" || read(p.join("scope")) == "Device" {
            continue;
        }
        let num = |k: &str| read(p.join(k)).parse::<u64>().unwrap_or(0);
        let status = match read(p.join("status")).as_str() {
            "Discharging" => 1,
            "Full" => 3,
            "Charging" => 6,
            _ => 2,
        };
        let name = format!("{} {}", clean(&read(p.join("manufacturer"))), clean(&read(p.join("model_name"))));
        bats.push(json!({
            "Name": name.trim(),
            "EstimatedChargeRemaining": num("capacity"),
            "BatteryStatus": status,
        }));
        // Some batteries count energy (µWh), others charge (µAh).
        let (f, d) = if num("energy_full") > 0 {
            (num("energy_full"), num("energy_full_design"))
        } else {
            unit = "мА·ч";
            (num("charge_full"), num("charge_full_design"))
        };
        full += f / 1000;
        design += d / 1000;
        cycles = cycles.max(num("cycle_count"));
    }
    (bats, json!({ "Full": full, "Design": design, "Cycles": cycles, "Unit": unit }))
}

/// Programs started with the desktop session.
fn autostart() -> Vec<Value> {
    let home = std::env::var("HOME").unwrap_or_default();
    let mut out = Vec::new();
    for dir in [format!("{home}/.config/autostart"), "/etc/xdg/autostart".to_string()] {
        for f in dirs(&dir) {
            if f.extension().map_or(true, |e| e != "desktop") {
                continue;
            }
            let text = fs::read_to_string(&f).unwrap_or_default();
            if let Some((name, _)) = desktop_entry(&text) {
                out.push(json!({ "Name": name, "Command": desktop_key(&text, "Exec"), "Location": dir }));
            }
        }
    }
    out
}

/// First word of every line: unit names from `systemctl ... --no-legend --plain`.
pub fn first_words(text: &str) -> Vec<String> {
    text.lines().filter_map(|l| l.split_whitespace().next()).map(str::to_string).collect()
}

/// Services that start with the system, and whether they run now.
fn services() -> Vec<Value> {
    let list = |args: &[&str]| run("systemctl", args).map(|t| first_words(&t)).unwrap_or_default();
    let running: BTreeSet<String> =
        list(&["list-units", "--type=service", "--state=running", "--no-legend", "--plain", "--no-pager"]).into_iter().collect();
    list(&["list-unit-files", "--type=service", "--state=enabled", "--no-legend", "--no-pager"])
        .into_iter()
        .map(|name| {
            let state = if running.contains(&name) { "Running" } else { "Stopped" };
            json!({ "DisplayName": name.trim_end_matches(".service"), "Name": name, "State": state })
        })
        .collect()
}

/// Devices plugged into USB (the computer's own hubs left out).
fn usb_devices() -> Vec<Value> {
    dirs("/sys/bus/usb/devices")
        .iter()
        .filter(|p| !file_name(p).starts_with("usb"))
        .filter_map(|p| {
            let name = read(p.join("product"));
            if name.is_empty() {
                return None;
            }
            Some(json!({ "Name": name, "Manufacturer": read(p.join("manufacturer")) }))
        })
        .collect()
}

fn printers() -> Vec<Value> {
    run("lpstat", &["-e"])
        .map(|t| t.lines().filter(|l| !l.trim().is_empty()).map(|l| json!({ "Name": l.trim() })).collect())
        .unwrap_or_default()
}

/// Everything, in the shape of the Windows script's output.
pub fn collect() -> Result<Value, String> {
    let mem = meminfo(&fs::read_to_string("/proc/meminfo").unwrap_or_default());
    let kb = |k: &str| mem.get(k).copied().unwrap_or(0);
    let osr = os_release(&fs::read_to_string("/etc/os-release").unwrap_or_default());
    let uptime: f64 = read("/proc/uptime").split_whitespace().next().and_then(|x| x.parse().ok()).unwrap_or(0.0);
    let cpu = cpus(&fs::read_to_string("/proc/cpuinfo").unwrap_or_default());
    let mut cpu = cpu;
    if let Ok(khz) = read("/sys/devices/system/cpu/cpu0/cpufreq/cpuinfo_max_freq").parse::<u64>() {
        for c in cpu.iter_mut() {
            c["MaxClockSpeed"] = json!(khz / 1000);
        }
    }
    let (gpu, mon) = gpus_and_monitors();
    let (disk, pdisk) = disks();
    let (net, ip) = network();
    let vol = run("df", &["-B1", "--output=source,fstype,size,avail,target"]).map(|t| df_volumes(&t)).unwrap_or_default();
    let boot = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs().saturating_sub(uptime as u64))
        .unwrap_or(0);
    let user = std::env::var("USER").unwrap_or_default();
    let mut domain = read("/proc/sys/kernel/domainname");
    if domain == "(none)" {
        domain.clear();
    }
    let os_name = osr.get("PRETTY_NAME").or(osr.get("NAME")).cloned().unwrap_or_else(|| "Linux".into());
    let (bat, batx) = batteries();

    Ok(json!({
        "cs": {
            "Name": read("/proc/sys/kernel/hostname"),
            "Manufacturer": dmi("sys_vendor"),
            "Model": dmi("product_name"),
            "TotalPhysicalMemory": kb("MemTotal") * 1024,
            "SystemType": std::env::consts::ARCH,
            "NumberOfProcessors": cpu.len(),
            "UserName": user,
            "Domain": domain,
            "PCSystemType": pc_system_type(&read("/sys/class/dmi/id/chassis_type")),
        },
        "os": {
            "Caption": os_name,
            "Version": read("/proc/sys/kernel/osrelease"),
            "OSArchitecture": if cfg!(target_pointer_width = "64") { "64-bit" } else { "32-bit" },
            "TotalVisibleMemorySize": kb("MemTotal"),
            "FreePhysicalMemory": kb("MemAvailable"),
            "TotalVirtualMemorySize": kb("MemTotal") + kb("SwapTotal"),
            "FreeVirtualMemory": kb("MemAvailable") + kb("SwapFree"),
            "LastBoot": date(&["-d", &format!("@{boot}"), "+%d.%m.%Y %H:%M"]),
            "UptimeSec": uptime as u64,
            "Now": date(&["+%d.%m.%Y / %H:%M"]),
        },
        "osx": {},
        "cpu": cpu,
        "cache": caches(),
        "board": {
            "Manufacturer": dmi("board_vendor"),
            "Product": dmi("board_name"),
            "Version": dmi("board_version"),
            "SerialNumber": dmi("board_serial"),
        },
        "bios": {
            "Manufacturer": dmi("bios_vendor"),
            "SMBIOSBIOSVersion": dmi("bios_version"),
            "Date": bios_date(&dmi("bios_date")),
        },
        "sysenc": { "UUID": dmi("product_uuid") },
        "mem": [],
        "gpu": gpu,
        "vram": [],
        "mon": mon,
        "audio": sound_cards(&fs::read_to_string("/proc/asound/cards").unwrap_or_default()),
        "disk": disk,
        "pdisk": pdisk,
        "vol": vol,
        "net": net,
        "ip": ip,
        "apps": apps(),
        "bat": bat,
        "batx": batx,
        "startup": autostart(),
        "services": services(),
        "usb": usb_devices(),
        "printers": printers(),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    const CPUINFO: &str = "processor\t: 0\nvendor_id\t: AuthenticAMD\ncpu family\t: 25\nmodel\t\t: 33\nmodel name\t: AMD Ryzen 5 5600X 6-Core Processor\nstepping\t: 0\ncpu MHz\t\t: 3700.000\nphysical id\t: 0\ncore id\t\t: 0\ncpu cores\t: 6\nflags\t\t: fpu lm svm sse\n\nprocessor\t: 1\nvendor_id\t: AuthenticAMD\nmodel name\t: AMD Ryzen 5 5600X 6-Core Processor\nphysical id\t: 0\ncore id\t\t: 0\ncpu cores\t: 6\nflags\t\t: fpu lm svm sse\n";

    #[test]
    fn cpu_parse() {
        let c = cpus(CPUINFO);
        assert_eq!(c.len(), 1);
        assert_eq!(c[0]["Name"], "AMD Ryzen 5 5600X 6-Core Processor");
        assert_eq!(c[0]["Manufacturer"], "AMD");
        assert_eq!(c[0]["NumberOfCores"], 6);
        assert_eq!(c[0]["NumberOfLogicalProcessors"], 2);
        assert_eq!(c[0]["MaxClockSpeed"], 3700);
        assert_eq!(c[0]["VirtualizationFirmwareEnabled"], true);
        assert_eq!(c[0]["AddressWidth"], 64);
    }

    #[test]
    fn small_parsers() {
        let m = meminfo("MemTotal:       16384000 kB\nMemAvailable:    8000000 kB\n");
        assert_eq!(m["MemTotal"], 16384000);
        let o = os_release("NAME=\"Ubuntu\"\nPRETTY_NAME=\"Ubuntu 24.04 LTS\"\n");
        assert_eq!(o["PRETTY_NAME"], "Ubuntu 24.04 LTS");
        assert_eq!(size_kb("32K"), 32);
        assert_eq!(size_kb("32M"), 32768);
        assert_eq!(bios_date("12/25/2023"), "25.12.2023");
        assert_eq!(pc_system_type("3"), 1);
        assert_eq!(pc_system_type("10"), 2);
        assert_eq!(clean("To Be Filled By O.E.M."), "");
        assert_eq!(pci_vendor("0x10de"), "NVIDIA");
        assert_eq!(
            lspci_fields(r#"01:00.0 "VGA compatible controller" "NVIDIA Corporation" "GA106 [GeForce RTX 3060]" -ra1"#),
            vec!["VGA compatible controller", "NVIDIA Corporation", "GA106 [GeForce RTX 3060]"]
        );
    }

    #[test]
    fn sound_and_disks() {
        let s = sound_cards(" 0 [PCH            ]: HDA-Intel - HDA Intel PCH\n                      HDA Intel PCH at 0xf7f10000 irq 32\n 1 [NVidia         ]: HDA-Intel - HDA NVidia\n");
        assert_eq!(s.len(), 2);
        assert_eq!(s[0]["Name"], "HDA Intel PCH");
        let v = df_volumes("Filesystem Type 1B-blocks Avail Mounted on\n/dev/nvme0n1p2 ext4 500000000000 100000000000 /\n/dev/nvme0n1p2 ext4 500000000000 100000000000 /var/snap\ntmpfs tmpfs 100 100 /run\n/dev/sdb1 vfat 64000000000 1000 /media/user/USB STICK\n");
        assert_eq!(v.len(), 2);
        assert_eq!(v[0]["DeviceID"], "/");
        assert_eq!(v[0]["DriveType"], 3);
        assert_eq!(v[1]["DeviceID"], "/media/user/USB STICK");
        assert_eq!(v[1]["DriveType"], 2);
    }

    #[test]
    fn desktop_and_edid() {
        assert_eq!(
            desktop_entry("[Desktop Entry]\nType=Application\nName=Firefox\nComment=Web Browser\n[Desktop Action new]\nName=New Window\n"),
            Some(("Firefox".into(), "Web Browser".into()))
        );
        assert_eq!(desktop_entry("[Desktop Entry]\nType=Application\nName=Hidden\nNoDisplay=true\n"), None);
        assert_eq!(desktop_key("[Desktop Entry]\nName=A\nExec=a --start\n[Other]\nExec=b\n", "Exec"), "a --start");
        assert_eq!(first_words("ssh.service enabled enabled\n  cups.service  enabled\n\n"), vec!["ssh.service", "cups.service"]);

        let mut e = vec![0u8; 128];
        e[..8].copy_from_slice(&[0, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0]);
        // "LEN" = L(12) E(5) N(14): 0b0_01100_00101_01110
        let id: u16 = (12 << 10) | (5 << 5) | 14;
        e[8..10].copy_from_slice(&id.to_be_bytes());
        e[17] = 35; // 2025
        e[21] = 53;
        e[22] = 30;
        let o = 54;
        e[o + 3] = 0xFC;
        e[o + 5..o + 18].copy_from_slice(b"Legion 24\n   ");
        let m = edid(&e).unwrap();
        assert_eq!(m["Maker"], "LEN");
        assert_eq!(m["Name"], "Legion 24");
        assert_eq!(m["Year"], 2025);
        assert_eq!(m["WidthCm"], 53);
    }
}
