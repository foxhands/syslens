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

/// The name of the account the program runs under.
pub fn account() -> String {
    std::env::var(if cfg!(windows) { "USERNAME" } else { "USER" }).unwrap_or_default()
}

/// Replaces everything that identifies the computer or its owner.
pub fn hide_personal(page: &mut Page, user: &str) {
    for s in &mut page.sections {
        s.title = hide_user(&s.title, user);
        for (k, v) in &mut s.rows {
            if PERSONAL.contains(&k.as_str()) {
                *v = "скрыто".into();
                continue;
            }
            *k = hide_user(k, user);
            *v = hide_user(v, user);
        }
    }
}

/// The account name as a folder in a path ("/media/anna/USB", "C:\Users\anna") → "***".
pub fn hide_user(s: &str, user: &str) -> String {
    if user.is_empty() {
        return s.to_string();
    }
    let mut out = String::new();
    let mut rest = s;
    while let Some(i) = rest.find(user) {
        let before = rest[..i].chars().last();
        let after = rest[i + user.len()..].chars().next();
        let folder = matches!(before, Some('/') | Some('\\')) && after.map_or(true, |c| !c.is_alphanumeric());
        out.push_str(&rest[..i]);
        out.push_str(if folder { "***" } else { user });
        rest = &rest[i + user.len()..];
    }
    out.push_str(rest);
    out
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
        hide_personal(&mut page, "anna");
        let s = &page.sections[0];
        assert_eq!(s.title, "Диск /home/***  sda1");
        for i in 0..4 {
            assert_eq!(s.rows[i].1, "скрыто", "{}", s.rows[i].0);
        }
        assert_eq!(s.rows[4].0, "/media/***/USB (vfat)");
        assert_eq!(s.rows[5].1, "annapurna 9000"); // only whole folder names
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
