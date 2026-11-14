//! What identifies this computer or its owner, and how it is hidden in
//! screenshots, reports and the data two computers are compared by.

use crate::info::Page;

/// Rows that identify this computer or its owner.
pub const PERSONAL: &[&str] = &[
    "Имя компьютера",
    "Имя пользователя",
    "Пользователь",
    "Зарегистрированный пользователь",
    "Домен / рабочая группа",
    "Первичный адрес IP",
    "Серийный номер",
    "UUID",
    "MAC-адрес",
    "IP-адреса",
    "Шлюз",
    "DNS",
];

/// The names the owner's folder goes by: the account name and the profile
/// folder when it is called differently ("anna.ANNA-PC", "ANNA~1").
#[derive(Clone, Debug, Default)]
pub struct Names(Vec<String>);

impl Names {
    /// Explicit names; empty ones mean nothing and are dropped.
    pub fn of(names: &[&str]) -> Names {
        let mut list: Vec<String> = Vec::new();
        for n in names {
            if !n.is_empty() && !list.iter().any(|x| x == n) {
                list.push(n.to_string());
            }
        }
        // the longest first: "anna.ANNA-PC" is tried before "anna"
        list.sort_by_key(|n| std::cmp::Reverse(n.len()));
        Names(list)
    }
}

/// The names of the account the program runs under.
pub fn account() -> Names {
    let var = |k: &str| std::env::var(k).unwrap_or_default();
    let (user, home) = if cfg!(windows) { ("USERNAME", "USERPROFILE") } else { ("USER", "HOME") };
    Names::of(&[&var(user), last_folder(&var(home))])
}

/// "C:\Users\anna.ANNA-PC\" → "anna.ANNA-PC".
fn last_folder(path: &str) -> &str {
    path.trim_end_matches(['/', '\\']).rsplit(['/', '\\']).next().unwrap_or_default()
}

/// Replaces everything that identifies the computer or its owner.
pub fn hide_personal(page: &mut Page, names: &Names) {
    for s in &mut page.sections {
        s.title = hide_user(&s.title, names);
        for (k, v) in &mut s.rows {
            if PERSONAL.contains(&k.as_str()) {
                *v = "скрыто".into();
                continue;
            }
            *k = hide_user(k, names);
            *v = hide_user(v, names);
        }
    }
}

/// The owner's folder in a path ("/media/anna/USB", "C:\Users\anna") → "***".
/// Windows does not tell "Anna" from "anna"; other systems do.
pub fn hide_user(s: &str, names: &Names) -> String {
    hide_names(s, names, cfg!(windows))
}

/// Only a whole folder name is replaced, together with what the system adds
/// to it ("anna.ANNA-PC", "ANNA~1"); "annapurna" stays.
fn hide_names(s: &str, names: &Names, ignore_case: bool) -> String {
    let mut out = String::new();
    let mut rest = s;
    while let Some(i) = rest.find(['/', '\\']) {
        out.push_str(&rest[..=i]);
        rest = &rest[i + 1..];
        let found = names.0.iter().find_map(|n| {
            let len = prefix_len(rest, n, ignore_case)?;
            rest[len..].chars().next().map_or(true, |c| !c.is_alphanumeric()).then_some(len)
        });
        if let Some(mut len) = found {
            // the rest of the folder's name would give away the computer's name
            if rest[len..].starts_with(['.', '-', '_', '~']) {
                let more = rest[len..].find(|c: char| !c.is_alphanumeric() && !".-_~".contains(c));
                len = more.map_or(rest.len(), |m| len + m);
            }
            out.push_str("***");
            rest = &rest[len..];
        }
    }
    out.push_str(rest);
    out
}

/// How many bytes of `s` the name takes when `s` starts with it.
fn prefix_len(s: &str, name: &str, ignore_case: bool) -> Option<usize> {
    if !ignore_case {
        return s.starts_with(name).then_some(name.len());
    }
    let mut chars = s.char_indices();
    for n in name.chars() {
        let (_, c) = chars.next()?;
        if !c.to_lowercase().eq(n.to_lowercase()) {
            return None;
        }
    }
    Some(chars.next().map_or(s.len(), |(i, _)| i))
}

/// Hides other computers on the local network: only the printers page names
/// them (a shared printer's computer, a network printer's address).
pub fn hide_lan(id: &str, page: &mut Page) {
    if id != "printers" {
        return;
    }
    for s in &mut page.sections {
        for (k, v) in &mut s.rows {
            *k = hide_addresses(&hide_host(k));
            *v = hide_addresses(&hide_host(v));
        }
    }
}

/// The computer in a network name: "\\OFFICE-PC\HP" → "\\***\HP".
pub fn hide_host(s: &str) -> String {
    let mut out = String::new();
    let mut rest = s;
    while let Some(i) = rest.find("\\\\") {
        let start = i + 2;
        let len = rest[start..].find(|c: char| c == '\\' || c.is_whitespace()).unwrap_or(rest.len() - start);
        out.push_str(&rest[..start]);
        if len > 0 && rest[start + len..].starts_with('\\') {
            out.push_str("***");
            rest = &rest[start + len..];
        } else {
            rest = &rest[start..];
        }
    }
    out.push_str(rest);
    out
}

/// IPv4 addresses: "IP_192.168.1.50" → "IP_***". A version such as
/// "10.0.19041.1" is not an address and stays.
pub fn hide_addresses(s: &str) -> String {
    let mut out = String::new();
    let mut prev: Option<char> = None;
    let mut i = 0;
    while let Some(c) = s[i..].chars().next() {
        let starts = c.is_ascii_digit() && !prev.map_or(false, |p| p.is_ascii_digit() || p == '.');
        match starts.then(|| ipv4_len(&s[i..])).flatten() {
            Some(len) => {
                out.push_str("***");
                i += len;
                prev = Some('0');
            }
            None => {
                out.push(c);
                i += c.len_utf8();
                prev = Some(c);
            }
        }
    }
    out
}

/// The length of the IPv4 address `s` starts with: four numbers up to 255
/// with dots between them and no further number after them.
fn ipv4_len(s: &str) -> Option<usize> {
    let b = s.as_bytes();
    let mut i = 0;
    for group in 0..4 {
        if group > 0 {
            if b.get(i) != Some(&b'.') {
                return None;
            }
            i += 1;
        }
        let start = i;
        while i < b.len() && b[i].is_ascii_digit() {
            i += 1;
        }
        if i == start || i - start > 3 || s[start..i].parse::<u16>().map_or(true, |n| n > 255) {
            return None;
        }
    }
    let more = b.get(i) == Some(&b'.') && b.get(i + 1).map_or(false, |c| c.is_ascii_digit());
    (!more).then_some(i)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::info::Section;

    #[test]
    fn private_mode_hides_the_owner() {
        let rows = [
            ("Зарегистрированный пользователь", "anna@example.com"),
            ("Домен / рабочая группа", "HOME"),
            ("Имя компьютера", "ANNA-PC"),
            ("MAC-адрес", "AA:BB:CC:DD:EE:FF"),
            ("/media/anna/USB (vfat)", "100 МБ"),
            ("Тип ЦП", "annapurna 9000"),
        ];
        let mut page = Page {
            sections: vec![Section {
                title: "Диск /home/anna  sda1".into(),
                rows: rows.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect(),
            }],
        };
        hide_personal(&mut page, &Names::of(&["anna"]));
        let s = &page.sections[0];
        assert_eq!(s.title, "Диск /home/***  sda1");
        for i in 0..4 {
            assert_eq!(s.rows[i].1, "скрыто", "{}", s.rows[i].0);
        }
        assert_eq!(s.rows[4].0, "/media/***/USB (vfat)");
        assert_eq!(s.rows[5].1, "annapurna 9000"); // only whole folder names
    }

    #[test]
    fn the_profile_folder_is_hidden_whole() {
        let anna = Names::of(&["anna"]);
        // what Windows adds to the folder's name is the computer's name
        assert_eq!(hide_names("C:\\Users\\anna.ANNA-PC\\Docs", &anna, true), "C:\\Users\\***\\Docs");
        assert_eq!(hide_names("C:\\Users\\ANNA~1\\x.png", &anna, true), "C:\\Users\\***\\x.png");
        assert_eq!(hide_names("(C:\\Users\\Anna.ANNA-PC)", &anna, true), "(C:\\Users\\***)");
        // only Windows takes "Anna" and "anna" for one name
        assert_eq!(hide_names("/home/Anna/x", &anna, false), "/home/Anna/x");
        assert_eq!(hide_names("/home/anna/x /home/annapurna", &anna, false), "/home/***/x /home/annapurna");
        // a folder that is not called like the account: a renamed account
        let renamed = Names::of(&["Анна Иванова", last_folder("C:\\Users\\anna\\"), ""]);
        assert_eq!(hide_names("C:\\Users\\ANNA\\a и D:\\анна иванова\\b", &renamed, true), "C:\\Users\\***\\a и D:\\***\\b");
        assert_eq!(hide_names("C:\\Users\\anna", &Names::default(), true), "C:\\Users\\anna");
        assert_eq!(last_folder("/home/anna"), "anna");
        assert_eq!(last_folder("/"), "");
    }

    #[test]
    fn printers_do_not_name_the_neighbours() {
        assert_eq!(hide_host("\\\\OFFICE-PC\\HP LaserJet"), "\\\\***\\HP LaserJet");
        assert_eq!(hide_host("HP LaserJet"), "HP LaserJet");
        assert_eq!(hide_addresses("IP_192.168.1.50"), "IP_***");
        assert_eq!(hide_addresses("Drv  ·  192.168.1.50  ·  основной"), "Drv  ·  ***  ·  основной");
        assert_eq!(hide_addresses("10.0.0.7_1, 10.0.0.8"), "***_1, ***");
        // versions are not addresses
        for v in ["PCL 6 v7.0.1.24923", "10.0.19041.1.5", "1.2.3", "WSD-5f2c-1.2", "300.1.1.1"] {
            assert_eq!(hide_addresses(v), v);
        }
        let d = serde_json::json!({ "printers": [
            { "Name": "\\\\OFFICE-PC\\HP", "DriverName": "Drv", "PortName": "IP_192.168.1.50" },
            { "Name": "PDF", "DriverName": "Drv", "PortName": "P:" },
        ] });
        let mut page = crate::info::page("printers", &d);
        hide_lan("printers", &mut page);
        let rows = &page.sections[0].rows;
        assert_eq!(rows[0], ("\\\\***\\HP".to_string(), "Drv  ·  IP_***".to_string()));
        assert_eq!(rows[1], ("PDF".to_string(), "Drv  ·  P:".to_string()));
        // other pages are full of numbers with dots
        let mut os = Page { sections: vec![Section { title: "ОС".into(), rows: vec![("Версия".into(), "10.0.0.1".into())] }] };
        hide_lan("os", &mut os);
        assert_eq!(os.sections[0].rows[0].1, "10.0.0.1");
    }

    #[test]
    fn every_personal_row_is_a_real_row_name() {
        // A renamed row would silently stop being hidden.
        let src = include_str!("info.rs");
        for name in PERSONAL {
            assert!(src.contains(&format!("\"{name}\"")), "{name}");
        }
    }
}
