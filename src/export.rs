//! The full report as text, a web page or JSON, and where to save it.

use crate::info::{self, Page};
use serde_json::{json, Value};
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
/// language and with the privacy the person chose. `compare` is what another
/// SysLens compares its computer with; it goes into the JSON report only.
pub fn render(format: Format, heading: &str, pages: &[(String, Page)], compare: &Value) -> Vec<u8> {
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
        Format::Html => html(heading, pages, false).into_bytes(),
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
            let doc = json!({
                "program": "SysLens",
                "version": env!("CARGO_PKG_VERSION"),
                "title": heading,
                "pages": pages,
                "compare": compare,
            });
            serde_json::to_vec_pretty(&doc).unwrap_or_default()
        }
    }
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

/// The report as a web page that opens the browser's print window by itself:
/// from there it goes to a printer or into a PDF file.
pub fn print_page(heading: &str, pages: &[(String, Page)]) -> String {
    html(heading, pages, true)
}

fn html(heading: &str, pages: &[(String, Page)], print: bool) -> String {
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
         @media print{body{background:#fff;color:#000;padding:0}main{max-width:none}section{break-inside:avoid}h2{break-after:avoid}}\n",
    );
    if print {
        // paper is white whatever the theme; the print window opens when the page is ready
        out.push_str("</style>\n<script>window.addEventListener('load',function(){window.print()})</script>\n");
    } else {
        out.push_str(
            "@media (prefers-color-scheme:dark){body{background:#111216;color:#eef0f4}section{background:#1d1f25;border-color:#2c2f38}\
             th,td{border-color:#2c2f38}th{color:#8c93a0}}\n</style>\n",
        );
    }
    out.push_str("</head>\n<body>\n<main>\n");
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

/// What a file window ended with.
pub enum Ask {
    Chosen(PathBuf),
    Cancelled,
    /// This system has no such window to show.
    #[cfg_attr(any(windows, target_os = "macos"), allow(dead_code))]
    NoDialog,
}

/// Kinds of file offered in a file window: (name, extension).
pub const REPORT_KINDS: &[(&str, &str)] = &[("Text", "txt"), ("HTML", "html"), ("JSON", "json")];
pub const PICTURE_KINDS: &[(&str, &str)] = &[("PNG", "png")];
pub const JSON_KINDS: &[(&str, &str)] = &[("JSON", "json")];

/// Asks where to save a file. `name` has no extension: the person picks the
/// kind in the window; a name typed without one gets the first kind's.
#[cfg(any(windows, target_os = "macos"))]
pub fn ask_path(title: &str, name: &str, dir: &Path, kinds: &[(&str, &str)]) -> Ask {
    let mut window = rfd::FileDialog::new().set_title(title).set_directory(dir).set_file_name(name);
    for (label, ext) in kinds {
        window = window.add_filter(format!("{label} (*.{ext})"), &[*ext]);
    }
    match window.save_file() {
        Some(p) => Ask::Chosen(with_extension(p, kinds[0].1)),
        None => Ask::Cancelled,
    }
}

/// Asks which file to open.
#[cfg(any(windows, target_os = "macos"))]
pub fn pick_file(title: &str, dir: &Path, kinds: &[(&str, &str)]) -> Ask {
    let mut window = rfd::FileDialog::new().set_title(title).set_directory(dir);
    for (label, ext) in kinds {
        window = window.add_filter(format!("{label} (*.{ext})"), &[*ext]);
    }
    match window.pick_file() {
        Some(p) => Ask::Chosen(p),
        None => Ask::Cancelled,
    }
}

/// On Linux the desktop's own helper is asked: zenity (GNOME and others) or kdialog (KDE).
#[cfg(not(any(windows, target_os = "macos")))]
pub fn ask_path(title: &str, name: &str, dir: &Path, kinds: &[(&str, &str)]) -> Ask {
    let ext = kinds[0].1;
    let start = dir.join(format!("{name}.{ext}"));
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
    match helper(&tries) {
        Ask::Chosen(p) => Ask::Chosen(with_extension(p, ext)),
        other => other,
    }
}

#[cfg(not(any(windows, target_os = "macos")))]
pub fn pick_file(title: &str, dir: &Path, kinds: &[(&str, &str)]) -> Ask {
    let ext = kinds[0].1;
    let start = format!("{}/", dir.display());
    let tries: [(&str, Vec<String>); 2] = [
        (
            "zenity",
            vec![
                "--file-selection".into(),
                format!("--title={title}"),
                format!("--filename={start}"),
                format!("--file-filter=*.{ext}"),
            ],
        ),
        ("kdialog", vec!["--title".into(), title.into(), "--getopenfilename".into(), start.clone(), format!("*.{ext}")]),
    ];
    helper(&tries)
}

/// Runs the first helper that exists and reads the path it prints.
#[cfg(not(any(windows, target_os = "macos")))]
fn helper(tries: &[(&str, Vec<String>)]) -> Ask {
    for (prog, args) in tries {
        if let Ok(out) = std::process::Command::new(prog).args(args).output() {
            let picked = String::from_utf8_lossy(&out.stdout).trim().to_string();
            return if out.status.success() && !picked.is_empty() { Ask::Chosen(PathBuf::from(picked)) } else { Ask::Cancelled };
        }
    }
    Ask::NoDialog
}

/// Opens a file or a web address with whatever the system uses for it.
pub fn open(target: &str) -> bool {
    use std::process::Command;
    #[cfg(windows)]
    let mut cmd = {
        use std::os::windows::process::CommandExt;
        let mut c = Command::new("rundll32.exe");
        c.args(["url.dll,FileProtocolHandler", target]).creation_flags(0x0800_0000); // CREATE_NO_WINDOW
        c
    };
    #[cfg(not(windows))]
    let mut cmd = {
        let mut c = Command::new(if cfg!(target_os = "macos") { "open" } else { "xdg-open" });
        c.arg(target);
        c
    };
    cmd.spawn().is_ok()
}

/// A name typed without an extension gets `ext`.
fn with_extension(p: PathBuf, ext: &str) -> PathBuf {
    if p.extension().is_some() { p } else { p.with_extension(ext) }
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
        assert_eq!(with_extension(PathBuf::from("report"), "txt"), PathBuf::from("report.txt"));
        assert_eq!(with_extension(PathBuf::from("page"), "png"), PathBuf::from("page.png"));
        assert_eq!(with_extension(PathBuf::from("report.html"), "txt"), PathBuf::from("report.html"));
    }

    #[test]
    fn html_is_escaped() {
        let h = String::from_utf8(render(Format::Html, "Отчёт", &sample(), &Value::Null)).unwrap();
        assert!(!h.contains("window.print"));
        assert!(h.contains("<h2>Страница &lt;1&gt;</h2>"));
        assert!(h.contains("<tr><th>Имя &amp; Ко</th><td>a &quot;b&quot;</td></tr>"));
    }

    #[test]
    fn json_keeps_the_structure() {
        let v: Value = serde_json::from_slice(&render(Format::Json, "Отчёт", &sample(), &json!({ "made": "now" }))).unwrap();
        assert_eq!(v["program"], "SysLens");
        assert_eq!(v["compare"]["made"], "now");
        assert_eq!(v["pages"][0]["sections"][0]["rows"][0]["name"], "Имя & Ко");
        assert_eq!(v["pages"][0]["sections"][0]["rows"][0]["value"], "a \"b\"");
    }

    #[test]
    fn print_page_prints_itself() {
        let h = print_page("Отчёт", &sample());
        assert!(h.contains("window.print()") && h.contains("@media print"));
        assert!(!h.contains("prefers-color-scheme")); // paper stays white
        assert!(h.contains("</style>") && h.ends_with("</html>\n"));
    }

    #[test]
    fn text_starts_with_the_mark() {
        let t = render(Format::Txt, "Отчёт", &sample(), &Value::Null);
        assert_eq!(&t[..3], &[0xEF, 0xBB, 0xBF]);
        assert!(String::from_utf8_lossy(&t).contains("--------[ Страница <1> ]--------"));
    }
}
