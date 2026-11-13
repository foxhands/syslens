//! macOS: the basics from the system's own tools (sysctl, sw_vers, df,
//! system_profiler). Fewer pages are filled than on Windows and Linux.
//! The answer has the same shape as the Windows script's.

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

/// Lines of `df -k -l`: mounted local volumes with their sizes.
pub fn df_volumes(text: &str) -> Vec<Value> {
    text.lines()
        .skip(1)
        .filter_map(|line| {
            let f: Vec<&str> = line.split_whitespace().collect();
            // Filesystem 1024-blocks Used Available Capacity iused ifree %iused Mounted on
            let mount_at = f.iter().position(|x| x.starts_with('/') && !x.starts_with("/dev"))?;
            let mount = f[mount_at..].join(" ");
            let total: f64 = f.get(1)?.parse().ok()?;
            let free: f64 = f.get(3)?.parse().ok()?;
            let shown = mount == "/" || mount == "/System/Volumes/Data" || mount.starts_with("/Volumes/");
            (f[0].starts_with("/dev/") && shown && total > 0.0)
                .then(|| json!({ "DeviceID": mount, "FileSystem": "APFS", "Size": total * 1024.0, "FreeSpace": free * 1024.0, "DriveType": 3 }))
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
    let page: f64 = sysctl("hw.pagesize").parse().unwrap_or(4096.0);
    let free_pages: f64 = sysctl("vm.page_free_count").parse().unwrap_or(0.0);
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
            "Model": sysctl("hw.model"),
            "TotalPhysicalMemory": memory,
            "SystemType": run("uname", &["-m"]),
            "UserName": std::env::var("USER").unwrap_or_default(),
            "PCSystemType": if sysctl("hw.model").contains("Book") { 2 } else { 1 },
        },
        "os": {
            "Caption": format!("{} {}", run("sw_vers", &["-productName"]), run("sw_vers", &["-productVersion"])),
            "Version": run("sw_vers", &["-buildVersion"]),
            "OSArchitecture": run("uname", &["-m"]),
            "TotalVisibleMemorySize": memory / 1024.0,
            "FreePhysicalMemory": free_pages * page / 1024.0,
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
        "vol": df_volumes(&run("df", &["-k", "-l"])),
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
        let df = "Filesystem   1024-blocks      Used Available Capacity iused      ifree %iused  Mounted on\n\
                  /dev/disk3s1s1 482797652  10000000 200000000     5%  404167 2000000000    0%   /\n\
                  devfs                200       200         0   100%     692          0  100%   /dev\n\
                  /dev/disk3s6   482797652      1000 200000000     1%       0 2000000000    0%   /System/Volumes/VM\n\
                  /dev/disk5s1     1000000    500000    500000    50%       1          1   50%   /Volumes/My Disk\n";
        let v = df_volumes(df);
        assert_eq!(v.len(), 2);
        assert_eq!(v[0]["DeviceID"], "/");
        assert_eq!(v[1]["DeviceID"], "/Volumes/My Disk");
        assert_eq!(v[1]["FreeSpace"], 500000.0 * 1024.0);
        let (gpus, mons) = displays(
            r#"{"SPDisplaysDataType":[{"sppci_model":"Apple M2","spdisplays_vendor":"sppci_vendor_Apple","spdisplays_ndrvs":[{"_name":"Color LCD"}]}]}"#,
        );
        assert_eq!(gpus[0]["Name"], "Apple M2");
        assert_eq!(gpus[0]["AdapterCompatibility"], "Apple");
        assert_eq!(mons[0]["Name"], "Color LCD");
        assert!(displays("not json").0.is_empty());
    }
}
