//! macOS: the basics from the system's own tools (sysctl, sw_vers, df, mount,
//! vm_stat, pmset, system_profiler). Fewer pages are filled than on Windows
//! and Linux. The answer has the same shape as the Windows script's.

use serde_json::{json, Value};
use std::process::Command;

fn run(prog: &str, args: &[&str]) -> String {
    Command::new(prog)
        .args(args)
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default()
}

fn sysctl(name: &str) -> String {
    run("sysctl", &["-n", name])
}

/// "{ sec = 1700000000, usec = 0 } Tue Nov 14 ..." → 1700000000.
pub fn boot_seconds(text: &str) -> Option<u64> {
    text.split("sec = ").nth(1)?.split(|c: char| !c.is_ascii_digit()).next()?.parse().ok()
}

/// `mount` output → (mount point, file system as people know it).
/// A line looks like "/dev/disk3s1s1 on / (apfs, sealed, local, read-only, journaled)".
pub fn mount_types(text: &str) -> Vec<(String, String)> {
    text.lines()
        .filter_map(|line| {
            let (_, rest) = line.split_once(" on ")?;
            let (mount, options) = rest.rsplit_once(" (")?;
            let kind = options.split([',', ')']).next()?.trim();
            let shown = match kind {
                "apfs" => "APFS",
                "hfs" => "HFS+",
                "exfat" => "exFAT",
                "msdos" => "FAT",
                "ntfs" => "NTFS",
                other => other,
            };
            Some((mount.to_string(), shown.to_string()))
        })
        .collect()
}

/// The data half of the system disk; since Catalina it shares one container
/// with the read-only `/`, so only one of the two is shown.
const DATA: &str = "/System/Volumes/Data";

/// Lines of `df -k -l`: mounted local volumes with their sizes. The file
/// system of each comes from `mount` output, empty when it is not there.
pub fn df_volumes(text: &str, mounts: &str) -> Vec<Value> {
    let types = mount_types(mounts);
    let mut found: Vec<(String, f64, f64)> = text
        .lines()
        .skip(1)
        .filter_map(|line| {
            let f: Vec<&str> = line.split_whitespace().collect();
            // Filesystem 1024-blocks Used Available Capacity iused ifree %iused Mounted on
            let mount_at = f.iter().position(|x| x.starts_with('/') && !x.starts_with("/dev"))?;
            let mount = f[mount_at..].join(" ");
            let total: f64 = f.get(1)?.parse().ok()?;
            let free: f64 = f.get(3)?.parse().ok()?;
            let shown = mount == "/" || mount == DATA || mount.starts_with("/Volumes/");
            (f[0].starts_with("/dev/") && shown && total > 0.0).then_some((mount, total, free))
        })
        .collect();
    if found.iter().any(|(mount, ..)| mount == DATA) {
        found.retain(|(mount, ..)| mount != "/");
    }
    // the system disk first
    found.sort_by_key(|(mount, ..)| mount != "/" && mount != DATA);
    found
        .into_iter()
        .map(|(mount, total, free)| {
            let kind = types.iter().find(|(at, _)| *at == mount).map(|(_, kind)| kind.as_str()).unwrap_or_default();
            let name = if mount == DATA { "/" } else { mount.as_str() };
            json!({ "DeviceID": name, "FileSystem": kind, "Size": total * 1024.0, "FreeSpace": free * 1024.0, "DriveType": 3 })
        })
        .collect()
}

/// Memory that programs can still get, in bytes, from `vm_stat` output:
/// free, inactive, speculative and purgeable pages. The free pages alone are
/// a few percent on a healthy Mac, the rest is given back on demand.
pub fn available_memory(vm_stat: &str) -> Option<f64> {
    let page: f64 = vm_stat.split("page size of ").nth(1)?.split_whitespace().next()?.parse().ok()?;
    let pages = |name: &str| -> Option<f64> {
        vm_stat.lines().find_map(|line| {
            let (key, value) = line.split_once(':')?;
            if key.trim() != name {
                return None;
            }
            value.trim().trim_end_matches('.').parse().ok()
        })
    };
    let free = pages("Pages free")?;
    let rest: f64 = ["Pages inactive", "Pages speculative", "Pages purgeable"].iter().filter_map(|name| pages(name)).sum();
    Some((free + rest) * page)
}

/// The built-in battery from `pmset -g batt`, shaped like Win32_Battery.
/// A line looks like " -InternalBattery-0 (id=4653155)  85%; discharging; 4:32 remaining present: true".
pub fn batteries(pmset: &str) -> Vec<Value> {
    pmset
        .lines()
        .filter_map(|line| {
            let at = line.find("InternalBattery")?;
            let name = line[at..].split_whitespace().next()?;
            let mut parts = line.split(';').map(str::trim);
            let before = parts.next()?.strip_suffix('%')?;
            let digits = before.len() - before.bytes().rev().take_while(u8::is_ascii_digit).count();
            let charge: u32 = before[digits..].parse().ok()?;
            let status = match parts.next().unwrap_or_default() {
                "discharging" => 1,
                "charged" => 3,
                "charging" | "finishing charge" => 6,
                _ => 2,
            };
            let mut battery = json!({ "Name": name, "EstimatedChargeRemaining": charge, "BatteryStatus": status });
            // "4:32 remaining" is the time left only while it discharges
            let left = parts.next().and_then(|rest| rest.split_whitespace().next()).and_then(|time| time.split_once(':'));
            if let (1, Some((hours, minutes))) = (status, left) {
                if let (Ok(hours), Ok(minutes)) = (hours.parse::<u32>(), minutes.parse::<u32>()) {
                    battery["EstimatedRunTime"] = json!(hours * 60 + minutes);
                }
            }
            Some(battery)
        })
        .collect()
}

/// Graphics cards and screens from `system_profiler SPDisplaysDataType -json`.
pub fn displays(text: &str) -> (Vec<Value>, Vec<Value>) {
    let v: Value = serde_json::from_str(text).unwrap_or(Value::Null);
    let (mut gpus, mut mons) = (Vec::new(), Vec::new());
    for g in v.get("SPDisplaysDataType").and_then(|x| x.as_array()).into_iter().flatten() {
        let text = |k: &str| g.get(k).and_then(|x| x.as_str()).unwrap_or_default().to_string();
        gpus.push(json!({ "Name": text("sppci_model"), "AdapterCompatibility": text("spdisplays_vendor").replace("sppci_vendor_", "") }));
        for m in g.get("spdisplays_ndrvs").and_then(|x| x.as_array()).into_iter().flatten() {
            mons.push(json!({ "Name": m.get("_name").and_then(|x| x.as_str()).unwrap_or_default() }));
        }
    }
    (gpus, mons)
}

pub fn collect() -> Result<Value, String> {
    let memory: f64 = sysctl("hw.memsize").parse().unwrap_or(0.0);
    let boot = boot_seconds(&sysctl("kern.boottime")).unwrap_or(0);
    let now: u64 = run("date", &["+%s"]).parse().unwrap_or(0);
    // without vm_stat: the free pages alone, which is far too little
    let available = available_memory(&run("vm_stat", &[])).unwrap_or_else(|| {
        let page: f64 = sysctl("hw.pagesize").parse().unwrap_or(4096.0);
        sysctl("vm.page_free_count").parse::<f64>().unwrap_or(0.0) * page
    });
    let model = sysctl("hw.model");
    // Apple-silicon models are named like "Mac14,2", so the battery tells a laptop
    let bat = batteries(&run("pmset", &["-g", "batt"]));
    let laptop = !bat.is_empty() || model.contains("Book");
    let (gpus, mons) = displays(&run("system_profiler", &["SPDisplaysDataType", "-json"]));
    let apps: Vec<Value> = std::fs::read_dir("/Applications")
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| e.file_name().to_str().and_then(|n| n.strip_suffix(".app")).map(|n| json!({ "Name": n })))
        .collect();
    let cpu = match sysctl("machdep.cpu.brand_string") {
        name if name.is_empty() => "Apple".to_string(),
        name => name,
    };
    Ok(json!({
        "cs": {
            "Name": run("hostname", &["-s"]),
            "Manufacturer": "Apple",
            "Model": model,
            "TotalPhysicalMemory": memory,
            "SystemType": run("uname", &["-m"]),
            "UserName": std::env::var("USER").unwrap_or_default(),
            "PCSystemType": if laptop { 2 } else { 1 },
        },
        "os": {
            "Caption": format!("{} {}", run("sw_vers", &["-productName"]), run("sw_vers", &["-productVersion"])),
            "Version": run("sw_vers", &["-buildVersion"]),
            "OSArchitecture": run("uname", &["-m"]),
            "TotalVisibleMemorySize": memory / 1024.0,
            "FreePhysicalMemory": available.min(memory) / 1024.0,
            "LastBoot": run("date", &["-r", &boot.to_string(), "+%d.%m.%Y %H:%M"]),
            "UptimeSec": now.saturating_sub(boot),
            "Now": run("date", &["+%d.%m.%Y / %H:%M"]),
        },
        "cpu": [{
            "Name": cpu,
            "Manufacturer": if sysctl("machdep.cpu.vendor").is_empty() { "Apple".to_string() } else { sysctl("machdep.cpu.vendor") },
            "NumberOfCores": sysctl("hw.physicalcpu").parse::<u32>().unwrap_or(0),
            "NumberOfLogicalProcessors": sysctl("hw.logicalcpu").parse::<u32>().unwrap_or(0),
            "AddressWidth": 64,
        }],
        "gpu": gpus,
        "mon": mons,
        "vol": df_volumes(&run("df", &["-k", "-l"]), &run("mount", &[])),
        "bat": bat,
        "apps": apps,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parsers() {
        assert_eq!(boot_seconds("{ sec = 1700000000, usec = 123 } Tue Nov 14 22:13:20 2023"), Some(1700000000));
        assert_eq!(boot_seconds("nothing"), None);
        let (gpus, mons) = displays(
            r#"{"SPDisplaysDataType":[{"sppci_model":"Apple M2","spdisplays_vendor":"sppci_vendor_Apple","spdisplays_ndrvs":[{"_name":"Color LCD"}]}]}"#,
        );
        assert_eq!(gpus[0]["Name"], "Apple M2");
        assert_eq!(gpus[0]["AdapterCompatibility"], "Apple");
        assert_eq!(mons[0]["Name"], "Color LCD");
        assert!(displays("not json").0.is_empty());
    }

    const DF: &str = "Filesystem   1024-blocks      Used Available Capacity iused      ifree %iused  Mounted on\n\
                      /dev/disk3s1s1 482797652  10000000 200000000     5%  404167 2000000000    0%   /\n\
                      devfs                200       200         0   100%     692          0  100%   /dev\n\
                      /dev/disk3s6   482797652      1000 200000000     1%       0 2000000000    0%   /System/Volumes/VM\n\
                      /dev/disk3s5   482797652 270000000 200000000    58% 1500000 2000000000    0%   /System/Volumes/Data\n\
                      /dev/disk5s1     1000000    500000    500000    50%       1          1   50%   /Volumes/My Disk\n";

    const MOUNT: &str = "/dev/disk3s1s1 on / (apfs, sealed, local, read-only, journaled)\n\
                         devfs on /dev (devfs, local, nobrowse)\n\
                         /dev/disk3s6 on /System/Volumes/VM (apfs, local, noexec, journaled, noatime, nobrowse)\n\
                         /dev/disk3s5 on /System/Volumes/Data (apfs, local, journaled, nobrowse, protect, root data)\n\
                         map auto_home on /System/Volumes/Data/home (autofs, automounted, nobrowse)\n\
                         /dev/disk5s1 on /Volumes/My Disk (exfat, local, nodev, nosuid, noowners)\n\
                         /dev/disk6s2 on /Volumes/Old (1) (hfs, local, nodev, nosuid, journaled)\n";

    #[test]
    fn mounts() {
        let types = mount_types(MOUNT);
        let kind = |at: &str| types.iter().find(|(mount, _)| mount == at).map(|(_, kind)| kind.as_str());
        assert_eq!(kind("/"), Some("APFS"));
        assert_eq!(kind("/System/Volumes/Data"), Some("APFS"));
        assert_eq!(kind("/Volumes/My Disk"), Some("exFAT"));
        assert_eq!(kind("/Volumes/Old (1)"), Some("HFS+"));
        assert_eq!(kind("/dev"), Some("devfs"));
        assert_eq!(kind("/nowhere"), None);
        assert!(mount_types("garbage\n\n").is_empty());
    }

    #[test]
    fn volumes() {
        // the system disk once, under the name "/", with the numbers of its data half
        let v = df_volumes(DF, MOUNT);
        assert_eq!(v.len(), 2);
        assert_eq!(v[0]["DeviceID"], "/");
        assert_eq!(v[0]["FileSystem"], "APFS");
        assert_eq!(v[0]["Size"], 482797652.0 * 1024.0);
        assert_eq!(v[1]["DeviceID"], "/Volumes/My Disk");
        assert_eq!(v[1]["FileSystem"], "exFAT");
        assert_eq!(v[1]["FreeSpace"], 500000.0 * 1024.0);
        // before Catalina there is only "/"
        let old: String = DF.lines().filter(|line| !line.contains("/System/Volumes/Data")).map(|line| format!("{line}\n")).collect();
        let v = df_volumes(&old, MOUNT);
        assert_eq!(v.len(), 2);
        assert_eq!(v[0]["DeviceID"], "/");
        assert_eq!(v[0]["FileSystem"], "APFS");
        // no `mount` output: the type is left empty
        let v = df_volumes(DF, "");
        assert_eq!(v.len(), 2);
        assert_eq!(v[0]["FileSystem"], "");
        assert_eq!(v[1]["FileSystem"], "");
    }

    #[test]
    fn memory() {
        let vm_stat = "Mach Virtual Memory Statistics: (page size of 16384 bytes)\n\
                       Pages free:                               12345.\n\
                       Pages active:                            234567.\n\
                       Pages inactive:                          223456.\n\
                       Pages speculative:                         1234.\n\
                       Pages throttled:                              0.\n\
                       Pages wired down:                        123456.\n\
                       Pages purgeable:                           5678.\n\
                       \"Translation faults\":                 123456789.\n\
                       Pages copy-on-write:                    1234567.\n\
                       Pages zero filled:                     98765432.\n\
                       Pages reactivated:                      2345678.\n\
                       Pages purged:                            345678.\n\
                       File-backed pages:                       150000.\n\
                       Anonymous pages:                         309257.\n\
                       Pages stored in compressor:              400000.\n\
                       Pages occupied by compressor:            120000.\n\
                       Decompressions:                         1234567.\n\
                       Compressions:                           2345678.\n\
                       Pageins:                                3456789.\n\
                       Pageouts:                                 12345.\n\
                       Swapins:                                      0.\n\
                       Swapouts:                                     0.\n";
        assert_eq!(available_memory(vm_stat), Some((12345.0 + 223456.0 + 1234.0 + 5678.0) * 16384.0));
        // an Intel Mac: 4 KiB pages; a missing line counts as nothing
        let intel = "Mach Virtual Memory Statistics: (page size of 4096 bytes)\nPages free: 1000.\nPages inactive: 500.\n";
        assert_eq!(available_memory(intel), Some(1500.0 * 4096.0));
        assert_eq!(available_memory(""), None);
        assert_eq!(available_memory("Pages free: 1000.\n"), None);
    }

    #[test]
    fn battery() {
        let b = batteries("Now drawing from 'Battery Power'\n -InternalBattery-0 (id=4653155)\t85%; discharging; 4:32 remaining present: true\n");
        assert_eq!(b.len(), 1);
        assert_eq!(b[0]["Name"], "InternalBattery-0");
        assert_eq!(b[0]["EstimatedChargeRemaining"], 85);
        assert_eq!(b[0]["BatteryStatus"], 1);
        assert_eq!(b[0]["EstimatedRunTime"], 272);
        // on the charger the time is the time to full, which is not shown
        let b = batteries("Now drawing from 'AC Power'\n -InternalBattery-0 (id=4653155)\t47%; charging; 1:05 remaining present: true\n");
        assert_eq!(b[0]["EstimatedChargeRemaining"], 47);
        assert_eq!(b[0]["BatteryStatus"], 6);
        assert!(b[0].get("EstimatedRunTime").is_none());
        let b = batteries("Now drawing from 'AC Power'\n -InternalBattery-0 (id=4653155)\t100%; charged; 0:00 remaining present: true\n");
        assert_eq!(b[0]["EstimatedChargeRemaining"], 100);
        assert_eq!(b[0]["BatteryStatus"], 3);
        let b = batteries("Now drawing from 'AC Power'\n -InternalBattery-0 (id=4653155)\t80%; AC attached; not charging present: true\n");
        assert_eq!(b[0]["EstimatedChargeRemaining"], 80);
        assert_eq!(b[0]["BatteryStatus"], 2);
        let b = batteries("Now drawing from 'Battery Power'\n -InternalBattery-0 (id=4653155)\t9%; discharging; (no estimate) present: true\n");
        assert_eq!(b[0]["EstimatedChargeRemaining"], 9);
        assert!(b[0].get("EstimatedRunTime").is_none());
        // a desktop Mac has no such line
        assert!(batteries("Now drawing from 'AC Power'\n").is_empty());
        assert!(batteries("").is_empty());
    }
}
