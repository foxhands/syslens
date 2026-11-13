//! The full report as text, a web page or JSON, and where to save it.

use crate::info::{self, Page};
use serde_json::json;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Format {
    Txt,
    Html,
    Json,
}

/// The format a file name asks for; plain text unless it ends in .html or .json.
pub fn format_of(path: &Path) -> Format {
    match path.extension().and_then(|e| e.to_str()).map(|e| e.to_ascii_lowercase()).as_deref() {
        Some("html") | Some("htm") => Format::Html,
        Some("json") => Format::Json,
        _ => Format::Txt,
    }
}

/// The report file's contents. `pages` are (page title, page), already in the
/// language and with the privacy the person chose.
pub fn render(format: Format, heading: &str, pages: &[(String, Page)]) -> Vec<u8> {
    match format {
        Format::Txt => {
            let mut text = format!("{heading}\r\n\r\n");
            for (title, page) in pages {
                text.push_str(&info::report(title, page));
            }
            // the mark at the start lets Notepad read the letters correctly
            let mut bytes = vec![0xEF, 0xBB, 0xBF];
            bytes.extend_from_slice(text.as_bytes());
            bytes
        }
        Format::Html => html(heading, pages).into_bytes(),
        Format::Json => {
            let pages: Vec<_> = pages
                .iter()
                .map(|(title, page)| {
                    let sections: Vec<_> = page
                        .sections
                        .iter()
                        .map(|s| {
                            let rows: Vec<_> = s.rows.iter().map(|(k, v)| json!({ "name": k, "value": v })).collect();
                            json!({ "title": s.title, "rows": rows })
                        })
                        .collect();
                    json!({ "title": title, "sections": sections })
                })
                .collect();
            let doc = json!({ "program": "SysLens", "version": env!("CARGO_PKG_VERSION"), "title": heading, "pages": pages });
            serde_json::to_vec_pretty(&doc).unwrap_or_default()
        }
    }
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

fn html(heading: &str, pages: &[(String, Page)]) -> String {
    let mut out = String::from("<!doctype html>\n<html>\n<head>\n<meta charset=\"utf-8\">\n");
    out.push_str("<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n");
    out.push_str(&format!("<title>{}</title>\n", esc(heading)));
    out.push_str(
        "<style>\n\
         body{font:14px/1.45 'Segoe UI',system-ui,sans-serif;margin:0;padding:24px 16px;background:#f3f5f9;color:#141821}\n\
         main{max-width:920px;margin:0 auto}\n\
         h1{font-size:24px;margin:0 0 20px}\n\
         h2{font-size:18px;margin:28px 0 10px}\n\
         section{background:#fff;border:1px solid #e2e6ed;border-radius:12px;padding:14px 18px;margin:0 0 12px}\n\
         h3{font-size:15px;margin:0 0 8px}\n\
         table{border-collapse:collapse;width:100%}\n\
         th,td{text-align:left;vertical-align:top;padding:5px 0;border-top:1px solid #e2e6ed;overflow-wrap:anywhere}\n\
         tr:first-child th,tr:first-child td{border-top:0}\n\
         th{font-weight:400;color:#6e7684;width:36%;padding-right:16px}\n\
         @media (prefers-color-scheme:dark){body{background:#111216;color:#eef0f4}section{background:#1d1f25;border-color:#2c2f38}\
         th,td{border-color:#2c2f38}th{color:#8c93a0}}\n\
         </style>\n</head>\n<body>\n<main>\n",
    );
    out.push_str(&format!("<h1>{}</h1>\n", esc(heading)));
    for (title, page) in pages {
        if page.sections.is_empty() {
            continue;
        }
        out.push_str(&format!("<h2>{}</h2>\n", esc(title)));
        for s in &page.sections {
            out.push_str(&format!("<section>\n<h3>{}</h3>\n<table>\n", esc(&s.title)));
            for (k, v) in &s.rows {
                out.push_str(&format!("<tr><th>{}</th><td>{}</td></tr>\n", esc(k), esc(v)));
            }
            out.push_str("</table>\n</section>\n");
        }
    }
    out.push_str("</main>\n</body>\n</html>\n");
    out
}

/// What the "Save as" window ended with.
pub enum Ask {
    Chosen(PathBuf),
    Cancelled,
    /// This system has no such window to show.
    #[cfg_attr(windows, allow(dead_code))]
    NoDialog,
}

/// Asks where to save the report. `name` has no extension: the person picks
/// text, web page or JSON in the window.
#[cfg(windows)]
pub fn ask_path(title: &str, name: &str, dir: &Path) -> Ask {
    let picked = rfd::FileDialog::new()
        .set_title(title)
        .set_directory(dir)
        .set_file_name(name)
        .add_filter("Text (*.txt)", &["txt"])
        .add_filter("HTML (*.html)", &["html"])
        .add_filter("JSON (*.json)", &["json"])
        .save_file();
    match picked {
        Some(p) => Ask::Chosen(with_extension(p)),
        None => Ask::Cancelled,
    }
}

/// On Linux the desktop's own helper is asked: zenity (GNOME and others) or kdialog (KDE).
#[cfg(not(windows))]
pub fn ask_path(title: &str, name: &str, dir: &Path) -> Ask {
    use std::process::Command;
    let start = dir.join(format!("{name}.txt"));
    let tries: [(&str, Vec<String>); 2] = [
        (
            "zenity",
            vec![
                "--file-selection".into(),
                "--save".into(),
                "--confirm-overwrite".into(),
                format!("--title={title}"),
                format!("--filename={}", start.display()),
            ],
        ),
        ("kdialog", vec!["--title".into(), title.into(), "--getsavefilename".into(), start.display().to_string()]),
    ];
    for (prog, args) in tries {
        if let Ok(out) = Command::new(prog).args(&args).output() {
            let picked = String::from_utf8_lossy(&out.stdout).trim().to_string();
            return if out.status.success() && !picked.is_empty() {
                Ask::Chosen(with_extension(PathBuf::from(picked)))
            } else {
                Ask::Cancelled
            };
        }
    }
    Ask::NoDialog
}

/// A name typed without an extension becomes a text file.
fn with_extension(p: PathBuf) -> PathBuf {
    if p.extension().is_some() { p } else { p.with_extension("txt") }
}

/// A file name in `dir` that is not taken yet: "name.txt", "name-2.txt", ...
pub fn free_name(dir: &Path, name: &str, ext: &str) -> PathBuf {
    let first = dir.join(format!("{name}.{ext}"));
    if !first.exists() {
        return first;
    }
    (2..10_000)
        .map(|i| dir.join(format!("{name}-{i}.{ext}")))
        .find(|p| !p.exists())
        .unwrap_or(first)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::info::Section;

    fn sample() -> Vec<(String, Page)> {
        vec![(
            "Страница <1>".to_string(),
            Page { sections: vec![Section { title: "Раздел".into(), rows: vec![("Имя & Ко".into(), "a \"b\"".into())] }] },
        )]
    }

    #[test]
    fn formats() {
        assert_eq!(format_of(Path::new("a/report.HTML")), Format::Html);
        assert_eq!(format_of(Path::new("report.json")), Format::Json);
        assert_eq!(format_of(Path::new("report")), Format::Txt);
        assert_eq!(with_extension(PathBuf::from("report")), PathBuf::from("report.txt"));
        assert_eq!(with_extension(PathBuf::from("report.html")), PathBuf::from("report.html"));
    }

    #[test]
    fn html_is_escaped() {
        let h = String::from_utf8(render(Format::Html, "Отчёт", &sample())).unwrap();
        assert!(h.contains("<h2>Страница &lt;1&gt;</h2>"));
        assert!(h.contains("<tr><th>Имя &amp; Ко</th><td>a &quot;b&quot;</td></tr>"));
    }

    #[test]
    fn json_keeps_the_structure() {
        let v: serde_json::Value = serde_json::from_slice(&render(Format::Json, "Отчёт", &sample())).unwrap();
        assert_eq!(v["program"], "SysLens");
        assert_eq!(v["pages"][0]["sections"][0]["rows"][0]["name"], "Имя & Ко");
        assert_eq!(v["pages"][0]["sections"][0]["rows"][0]["value"], "a \"b\"");
    }

    #[test]
    fn text_starts_with_the_mark() {
        let t = render(Format::Txt, "Отчёт", &sample());
        assert_eq!(&t[..3], &[0xEF, 0xBB, 0xBF]);
        assert!(String::from_utf8_lossy(&t).contains("--------[ Страница <1> ]--------"));
    }
}
