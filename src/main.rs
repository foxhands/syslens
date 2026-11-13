#![windows_subsystem = "windows"]

mod advice;
mod diff;
mod export;
mod i18n;
mod icon;
mod info;
mod live;
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
mod linux;
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
mod mac;
mod privacy;
mod rules;
mod tray;
mod update;

use eframe::egui::{self, Color32, RichText, Stroke};
use i18n::{t, Lang};
use serde_json::Value;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

/// Menu tree: category, icon colour, pages (id, title).
const TREE: &[(&str, [u8; 3], &[(&str, &str)])] = &[
    (
        "Компьютер",
        [70, 120, 200],
        &[
            ("summary", "Суммарная информация"),
            ("live", "Нагрузка сейчас"),
            ("advice", "Оценки и подсказки"),
            ("compare", "Сравнение"),
            ("battery", "Батарея"),
        ],
    ),
    (
        "Системная плата",
        [60, 150, 90],
        &[("cpu", "ЦП"), ("board", "Системная плата"), ("memory", "Память"), ("spd", "SPD"), ("bios", "BIOS")],
    ),
    ("Операционная система", [40, 140, 210], &[("os", "Операционная система"), ("updates", "Обновления системы")]),
    ("Отображение", [200, 110, 40], &[("gpu", "Видеоадаптер"), ("monitor", "Монитор")]),
    ("Мультимедиа", [170, 70, 170], &[("audio", "Звуковые устройства")]),
    ("Хранение данных", [120, 120, 130], &[("pdisk", "Физические диски"), ("ldisk", "Логические диски")]),
    ("Сеть", [40, 160, 160], &[("net", "Сетевые адаптеры")]),
    ("Устройства", [150, 110, 70], &[("drivers", "Драйверы"), ("usb", "Устройства USB"), ("printers", "Принтеры")]),
    (
        "Программы",
        [220, 170, 40],
        &[("apps", "Установленные программы"), ("startup", "Автозагрузка"), ("services", "Службы")],
    ),
];

/// Pages that show something other than a list of facts about the computer;
/// they stay out of the report and the search.
const SPECIAL: &[&str] = &["live", "compare"];

/// Every ordinary page: (id, title).
fn fact_pages() -> impl Iterator<Item = (&'static str, &'static str)> {
    TREE.iter().flat_map(|c| c.2.iter().copied()).filter(|(id, _)| !SPECIAL.contains(id))
}

/// The pages two computers are compared by: the facts, without the judgements.
fn compared_pages() -> Vec<(&'static str, &'static str)> {
    fact_pages().filter(|(id, _)| *id != "advice").collect()
}

/// How pages are shown right now.
struct View<'a> {
    private: bool,
    sort: info::AppSort,
    /// Another computer, read from its report, for the comparison page.
    other: Option<&'a diff::Other>,
}

fn page_title(id: &str) -> String {
    for (_, _, pages) in TREE {
        for (pid, title) in *pages {
            if *pid == id {
                return t(title);
            }
        }
    }
    String::new()
}

fn page_color(id: &str) -> Color32 {
    for (_, c, pages) in TREE {
        if pages.iter().any(|(p, _)| *p == id) {
            return Color32::from_rgb(c[0], c[1], c[2]);
        }
    }
    Color32::GRAY
}

enum Load {
    Loading,
    Ready(Arc<Value>),
    Failed(String),
}

/// Saving the page as a picture takes a few frames.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Snap {
    Idle,
    /// Asked for: the next frame is drawn with personal data hidden and photographed.
    Armed,
    /// Waiting for the photograph.
    Waiting,
}

struct App {
    load: Arc<Mutex<Load>>,
    data: Option<Arc<Value>>,
    selected: &'static str,
    status: String,
    shot: Option<PathBuf>,
    frames: u64,
    ready_frames: u64,
    shot_sent: bool,
    show_about: bool,
    dark: bool,
    query: String,
    private: bool,
    /// A collection is running; another one is not started until it ends.
    loading: bool,
    /// The last collection failed.
    failed: bool,
    apps_sort: info::AppSort,
    /// Live load; starts sampling when its page is first opened.
    live: Option<live::Live>,
    /// Rows pinned to the summary page: "page\tsection\trow".
    favorites: Vec<String>,
    /// Another computer's report (file name, facts) opened for comparison.
    other: Option<(String, diff::Other)>,
    /// The icon next to the clock, while it is switched on.
    tray: Option<tray::Tray>,
    updates: Arc<Mutex<update::State>>,
    snap: Snap,
    snap_frames: u32,
    /// Where the page is on the screen: the part a picture of it shows.
    content_rect: egui::Rect,
}

/// Colours of the modern look for the current theme.
#[derive(Clone, Copy)]
struct Palette {
    bg: Color32,
    side: Color32,
    card: Color32,
    border: Color32,
    hover: Color32,
    text: Color32,
    soft: Color32,
    muted: Color32,
    accent: Color32,
}

fn palette(dark: bool) -> Palette {
    if dark {
        Palette {
            bg: Color32::from_rgb(17, 18, 22),
            side: Color32::from_rgb(23, 24, 29),
            card: Color32::from_rgb(29, 31, 37),
            border: Color32::from_rgb(44, 47, 56),
            hover: Color32::from_rgb(36, 38, 46),
            text: Color32::from_rgb(238, 240, 244),
            soft: Color32::from_rgb(196, 200, 210),
            muted: Color32::from_rgb(140, 147, 160),
            accent: Color32::from_rgb(108, 140, 255),
        }
    } else {
        Palette {
            bg: Color32::from_rgb(243, 245, 249),
            side: Color32::from_rgb(255, 255, 255),
            card: Color32::from_rgb(255, 255, 255),
            border: Color32::from_rgb(226, 230, 237),
            hover: Color32::from_rgb(238, 241, 247),
            text: Color32::from_rgb(20, 24, 33),
            soft: Color32::from_rgb(55, 62, 75),
            muted: Color32::from_rgb(110, 118, 132),
            accent: Color32::from_rgb(64, 100, 235),
        }
    }
}

fn apply_theme(ctx: &egui::Context, dark: bool) {
    let pal = palette(dark);
    let mut v = if dark { egui::Visuals::dark() } else { egui::Visuals::light() };
    v.panel_fill = pal.bg;
    v.window_fill = pal.card;
    v.extreme_bg_color = pal.bg;
    v.window_stroke = Stroke::new(1.0_f32, pal.border);
    v.window_corner_radius = egui::CornerRadius::same(12);
    v.menu_corner_radius = egui::CornerRadius::same(10);
    v.selection.bg_fill = pal.accent.gamma_multiply(0.35);
    v.selection.stroke = Stroke::new(1.0_f32, pal.text);
    v.hyperlink_color = pal.accent;
    for w in [
        &mut v.widgets.noninteractive,
        &mut v.widgets.inactive,
        &mut v.widgets.hovered,
        &mut v.widgets.active,
        &mut v.widgets.open,
    ] {
        w.corner_radius = egui::CornerRadius::same(8);
    }
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0_f32, pal.border);
    v.widgets.inactive.weak_bg_fill = pal.card;
    v.widgets.inactive.bg_stroke = Stroke::new(1.0_f32, pal.border);
    v.widgets.inactive.fg_stroke = Stroke::new(1.0_f32, pal.soft);
    v.widgets.hovered.weak_bg_fill = pal.hover;
    v.widgets.hovered.bg_stroke = Stroke::new(1.0_f32, pal.accent.gamma_multiply(0.6));
    v.widgets.active.weak_bg_fill = pal.hover;
    ctx.set_visuals(v);
    ctx.style_mut(|s| {
        s.spacing.button_padding = egui::vec2(12.0, 6.0);
        s.spacing.item_spacing = egui::vec2(8.0, 4.0);
    });
}

fn home() -> PathBuf {
    std::env::var(if cfg!(windows) { "USERPROFILE" } else { "HOME" }).map(PathBuf::from).unwrap_or_default()
}

/// Where settings live: %LOCALAPPDATA% on Windows, ~/.config elsewhere.
fn config_dir() -> PathBuf {
    let base = if cfg!(windows) {
        std::env::var("LOCALAPPDATA").map(PathBuf::from).unwrap_or_default()
    } else {
        std::env::var("XDG_CONFIG_HOME")
            .ok()
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(|| home().join(".config"))
    };
    base.join("SysLens")
}

/// A small setting, kept as a text file of its own.
fn setting(name: &str) -> Option<String> {
    std::fs::read_to_string(config_dir().join(name)).ok().map(|s| s.trim().to_string())
}

fn save_setting(name: &str, text: &str) {
    let dir = config_dir();
    let _ = std::fs::create_dir_all(&dir);
    let _ = std::fs::write(dir.join(name), text);
}

/// Dark unless the person switched to light earlier.
fn load_dark() -> bool {
    setting("theme.txt").map_or(true, |s| s != "light")
}

/// The saved language, or the system's display language the first time.
fn load_lang() -> Lang {
    setting("lang.txt").and_then(|s| Lang::from_code(&s)).unwrap_or_else(i18n::system)
}

/// Where the person keeps documents: the place file windows open in.
fn docs_dir() -> PathBuf {
    let docs = home().join("Documents");
    if docs.is_dir() { docs } else { home() }
}

impl App {
    fn set_lang(&mut self, ctx: &egui::Context, l: Lang) {
        i18n::set(l);
        save_setting("lang.txt", l.code());
        ctx.send_viewport_cmd(egui::ViewportCommand::Title(t("SysLens — сведения о системе")));
    }

    fn set_dark(&mut self, ctx: &egui::Context, dark: bool) {
        self.dark = dark;
        apply_theme(ctx, dark);
        save_setting("theme.txt", if dark { "dark" } else { "light" });
    }

    fn set_units(&mut self, decimal: bool) {
        rules::set_decimal(decimal);
        save_setting("units.txt", if decimal { "decimal" } else { "binary" });
    }

    fn set_tray(&mut self, ctx: &egui::Context, on: bool) {
        self.tray = on.then(|| tray::Tray::start(ctx.clone()));
        save_setting("tray.txt", if on { "on" } else { "off" });
    }

    /// Pins a row to the summary page, or unpins it.
    fn toggle_pin(&mut self, pin: String) {
        match self.favorites.iter().position(|f| *f == pin) {
            Some(i) => {
                self.favorites.remove(i);
                self.status = "Строка откреплена".into();
            }
            None => {
                self.favorites.push(pin);
                self.status = "Строка закреплена на сводной странице".into();
            }
        }
        save_setting("favorites.txt", &self.favorites.join("\n"));
    }

    fn new(cc: &eframe::CreationContext, shot: Option<PathBuf>) -> Self {
        setup_fonts(&cc.egui_ctx);
        let dark = load_dark();
        apply_theme(&cc.egui_ctx, dark);
        // no icon next to the clock while a picture is made for the documentation
        let tray = (tray::AVAILABLE && shot.is_none() && setting("tray.txt").as_deref() != Some("off"))
            .then(|| tray::Tray::start(cc.egui_ctx.clone()));
        let mut app = App {
            load: Arc::new(Mutex::new(Load::Loading)),
            data: None,
            selected: "summary",
            status: String::new(),
            shot,
            frames: 0,
            ready_frames: 0,
            shot_sent: false,
            show_about: false,
            dark,
            query: String::new(),
            private: false,
            loading: false,
            failed: false,
            apps_sort: info::AppSort::Name,
            live: None,
            favorites: setting("favorites.txt")
                .map(|s| s.lines().filter(|l| !l.is_empty()).map(str::to_string).collect())
                .unwrap_or_default(),
            other: None,
            tray,
            updates: Arc::new(Mutex::new(update::State::Idle)),
            snap: Snap::Idle,
            snap_frames: 0,
            content_rect: egui::Rect::NOTHING,
        };
        app.refresh(&cc.egui_ctx);
        app
    }

    fn view(&self, private: bool) -> View<'_> {
        View { private, sort: self.apps_sort, other: self.other.as_ref().map(|o| &o.1) }
    }

    fn refresh(&mut self, ctx: &egui::Context) {
        if self.loading {
            return; // one collection at a time (F5 held down, impatient clicks)
        }
        self.loading = true;
        self.failed = false;
        *self.load.lock().unwrap() = Load::Loading;
        self.status = "Сбор сведений о системе…".into();
        let slot = self.load.clone();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let res = info::collect();
            *slot.lock().unwrap() = match res {
                Ok(v) => Load::Ready(Arc::new(v)),
                Err(e) => Load::Failed(e),
            };
            ctx.request_repaint();
        });
    }

    fn save_report(&mut self) {
        let Some(d) = self.data.clone() else { return };
        let dir = docs_dir();
        let name = t("SysLens-отчёт");
        // The person picks the place and the kind of file: text, web page or JSON.
        let path = match export::ask_path(&t("Сохранить отчёт"), &name, &dir, export::REPORT_KINDS) {
            export::Ask::Chosen(p) => p,
            export::Ask::Cancelled => return,
            export::Ask::NoDialog => export::free_name(&dir, &name, "txt"),
        };
        let bytes = report_bytes(&path, &d, self.private, self.apps_sort);
        self.status = match std::fs::write(&path, bytes) {
            Ok(_) => format!("Отчёт сохранён: {}", path.display()),
            Err(e) => format!("Не удалось сохранить отчёт: {e}"),
        };
    }

    /// Opens the report in the browser with its print window: from there it
    /// goes to a printer or into a PDF file.
    fn print_report(&mut self) {
        let Some(d) = self.data.clone() else { return };
        let view = View { private: self.private, sort: self.apps_sort, other: None };
        let page = export::print_page(&t("Отчёт о системе (SysLens)"), &all_pages(&d, &view));
        let path = std::env::temp_dir().join("SysLens-print.html");
        self.status = match std::fs::write(&path, page) {
            Ok(_) if export::open(&path.display().to_string()) => {
                "Отчёт открыт в браузере: выберите принтер или «Сохранить как PDF»".into()
            }
            Ok(_) => "Не удалось открыть браузер".into(),
            Err(e) => format!("Не удалось сохранить отчёт: {e}"),
        };
    }

    /// Opens another computer's JSON report to compare with; asks for the
    /// file when none is given.
    fn open_other(&mut self, path: Option<PathBuf>) {
        let path = match path {
            Some(p) => p,
            None => match export::pick_file(&t("Открыть отчёт другого компьютера"), &docs_dir(), export::JSON_KINDS) {
                export::Ask::Chosen(p) => p,
                export::Ask::Cancelled => return,
                export::Ask::NoDialog => {
                    self.status = "Окно выбора файла недоступно. Запустите программу так: syslens --compare отчёт.json".into();
                    return;
                }
            },
        };
        let parsed = std::fs::read(&path)
            .map_err(|e| format!("Не удалось открыть файл: {e}"))
            .and_then(|bytes| {
                let text = String::from_utf8_lossy(&bytes);
                serde_json::from_str::<Value>(text.trim_start_matches('\u{feff}'))
                    .map_err(|_| "Это не отчёт SysLens в формате JSON".to_string())
            })
            .and_then(|v| diff::parse(&v));
        match parsed {
            Ok(other) => {
                let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                self.other = Some((name, other));
                self.selected = "compare";
                self.status = "Отчёт другого компьютера открыт".into();
            }
            Err(e) => self.status = e,
        }
    }

    /// Asks GitHub whether a newer version is out. Only when the person asks.
    fn check_updates(&mut self, ctx: &egui::Context) {
        let mut state = self.updates.lock().unwrap();
        if *state == update::State::Checking {
            return;
        }
        *state = update::State::Checking;
        let slot = self.updates.clone();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let found = update::check();
            *slot.lock().unwrap() = found;
            ctx.request_repaint();
        });
    }
}

/// The report file for `path`: its kind follows the file name.
fn report_bytes(path: &std::path::Path, d: &Value, private: bool, sort: info::AppSort) -> Vec<u8> {
    let view = View { private, sort, other: None };
    let compare = diff::to_json(&compared_pages(), d, &privacy::account());
    export::render(export::format_of(path), &t("Отчёт о системе (SysLens)"), &all_pages(d, &view), &compare)
}

/// Every ordinary page with its title, as shown: for the report.
fn all_pages(d: &Value, view: &View) -> Vec<(String, info::Page)> {
    fact_pages().map(|(id, title)| (t(title), page_for(id, d, view))).collect()
}

/// A row as shown, with the name it is pinned by (when it can be pinned).
#[derive(Clone)]
struct Row {
    key: String,
    value: String,
    pin: Option<String>,
}

struct Sec {
    title: String,
    rows: Vec<Row>,
}

/// A page before privacy and translation: the facts as the program writes them.
fn plain_page(id: &str, d: &Value, view: &View) -> info::Page {
    match (id, view.other) {
        ("compare", Some(other)) => diff::compare(&compared_pages(), other, d, &privacy::account()),
        ("compare", None) => info::Page { sections: Vec::new() },
        ("advice", _) => advice::page(d, i18n::get()),
        _ => info::page_sorted(id, d, view.sort),
    }
}

/// A page as shown: personal values replaced when `private` is on (for
/// screenshots and reports that will be shared), texts in the chosen language.
fn page_for(id: &str, d: &Value, view: &View) -> info::Page {
    finish(id, plain_page(id, d, view), view)
}

fn finish(id: &str, mut page: info::Page, view: &View) -> info::Page {
    if view.private {
        privacy::hide_personal(&mut page, &privacy::account());
    }
    let l = i18n::get();
    // the advice page is written in the chosen language from the start
    if l != Lang::Ru && id != "advice" {
        for s in &mut page.sections {
            // "page  ›  section": each part on its own
            s.title = s.title.split("  ›  ").map(|part| i18n::tr(part, l)).collect::<Vec<_>>().join("  ›  ");
            for (k, v) in &mut s.rows {
                *v = i18n::value(k, v, l);
                // on list pages the "field" is the item's own name, not a label
                if !OWN_NAMES.contains(&id) {
                    *k = i18n::tr(k, l);
                }
            }
        }
    }
    page
}

/// Pages whose row names are names of programs, devices and the like.
const OWN_NAMES: &[&str] = &["apps", "startup", "services", "drivers", "updates", "usb", "printers", "audio"];

/// The name a row is pinned by: page, section (without its count) and row,
/// as the program writes them — so a pin survives a change of language.
fn pin_name(id: &str, section: &str, key: &str) -> String {
    format!("{id}\t{}\t{key}", diff::plain_title(section))
}

/// A page as shown, each row with its pin name. Judgements and differences
/// are not pinned: they are not facts with a fixed place.
fn shown(id: &str, d: &Value, view: &View) -> Vec<Sec> {
    let plain = plain_page(id, d, view);
    let can_pin = id != "advice" && id != "compare";
    let pins: Vec<Vec<Option<String>>> = plain
        .sections
        .iter()
        .map(|s| s.rows.iter().map(|(k, _)| can_pin.then(|| pin_name(id, &s.title, k))).collect())
        .collect();
    finish(id, plain, view)
        .sections
        .into_iter()
        .zip(pins)
        .map(|(s, pins)| Sec {
            title: s.title,
            rows: s.rows.into_iter().zip(pins).map(|((key, value), pin)| Row { key, value, pin }).collect(),
        })
        .collect()
}

/// The pinned rows, in the order they were pinned. A row that is gone
/// (a program removed, a device unplugged) is simply not shown.
fn favorite_rows(favorites: &[String], d: &Value, view: &View) -> Vec<Row> {
    let mut pages: Vec<(&str, Vec<Sec>)> = Vec::new();
    let mut out = Vec::new();
    for f in favorites {
        let id = f.split('\t').next().unwrap_or_default();
        let Some((id, _)) = fact_pages().find(|(p, _)| *p == id) else { continue };
        if !pages.iter().any(|p| p.0 == id) {
            pages.push((id, shown(id, d, view)));
        }
        let secs = &pages.iter().find(|p| p.0 == id).unwrap().1;
        if let Some(row) = secs.iter().flat_map(|s| s.rows.iter()).find(|r| r.pin.as_deref() == Some(f.as_str())) {
            out.push(row.clone());
        }
    }
    out
}

/// Rows from all pages that contain `q` (lower case); a section whose title
/// matches is kept whole. Sections are titled "page  ›  section".
fn search_all(d: &Value, view: &View, q: &str) -> Vec<Sec> {
    let mut out = Vec::new();
    for (id, title) in fact_pages() {
        for mut s in shown(id, d, view) {
            if !s.title.to_lowercase().contains(q) {
                s.rows.retain(|r| r.key.to_lowercase().contains(q) || r.value.to_lowercase().contains(q));
            }
            if !s.rows.is_empty() {
                let page = t(title);
                if s.title != page {
                    s.title = format!("{page}  ›  {}", s.title);
                }
                out.push(s);
            }
        }
    }
    out
}

fn setup_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    // The system's own interface font: Segoe UI on Windows; on Linux the first
    // of Noto Sans, DejaVu Sans, Liberation Sans or Ubuntu that is installed.
    let candidates: [(&str, &[&str]); 2] = [
        ("segoe", &[
            "C:\\Windows\\Fonts\\segoeui.ttf",
            "/usr/share/fonts/truetype/noto/NotoSans-Regular.ttf",
            "/usr/share/fonts/noto/NotoSans-Regular.ttf",
            "/usr/share/fonts/google-noto/NotoSans-Regular.ttf",
            "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
            "/usr/share/fonts/TTF/DejaVuSans.ttf",
            "/usr/share/fonts/dejavu-sans-fonts/DejaVuSans.ttf",
            "/usr/share/fonts/truetype/liberation/LiberationSans-Regular.ttf",
            "/usr/share/fonts/truetype/ubuntu/Ubuntu-R.ttf",
        ]),
        ("segoeb", &[
            "C:\\Windows\\Fonts\\segoeuib.ttf",
            "/usr/share/fonts/truetype/noto/NotoSans-Bold.ttf",
            "/usr/share/fonts/noto/NotoSans-Bold.ttf",
            "/usr/share/fonts/google-noto/NotoSans-Bold.ttf",
            "/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf",
            "/usr/share/fonts/TTF/DejaVuSans-Bold.ttf",
            "/usr/share/fonts/dejavu-sans-fonts/DejaVuSans-Bold.ttf",
            "/usr/share/fonts/truetype/liberation/LiberationSans-Bold.ttf",
            "/usr/share/fonts/truetype/ubuntu/Ubuntu-B.ttf",
        ]),
    ];
    for (name, paths) in candidates {
        if let Some(bytes) = paths.iter().find_map(|p| std::fs::read(p).ok()) {
            fonts.font_data.insert(name.into(), Arc::new(egui::FontData::from_owned(bytes)));
        }
    }
    if fonts.font_data.contains_key("segoe") {
        fonts.families.get_mut(&egui::FontFamily::Proportional).unwrap().insert(0, "segoe".into());
    }
    if fonts.font_data.contains_key("segoeb") {
        fonts
            .families
            .insert(egui::FontFamily::Name("bold".into()), vec!["segoeb".into(), "segoe".into()]);
    } else {
        let p = fonts.families[&egui::FontFamily::Proportional].clone();
        fonts.families.insert(egui::FontFamily::Name("bold".into()), p);
    }
    // Last resort for Chinese, Japanese and Korean program and device names.
    let cjk = [
        "C:\\Windows\\Fonts\\msyh.ttc",
        "C:\\Windows\\Fonts\\msgothic.ttc",
        "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
        "/usr/share/fonts/noto-cjk/NotoSansCJK-Regular.ttc",
        "/usr/share/fonts/google-noto-cjk/NotoSansCJK-Regular.ttc",
    ];
    if let Some(bytes) = cjk.iter().find_map(|p| std::fs::read(p).ok()) {
        fonts.font_data.insert("cjk".into(), Arc::new(egui::FontData::from_owned(bytes)));
        for family in fonts.families.values_mut() {
            family.push("cjk".into());
        }
    }
    ctx.set_fonts(fonts);
    ctx.style_mut(|s| {
        s.text_styles.insert(egui::TextStyle::Body, egui::FontId::proportional(13.5));
        s.text_styles.insert(egui::TextStyle::Button, egui::FontId::proportional(13.5));
        s.spacing.item_spacing = egui::vec2(6.0, 3.0);
    });
}

fn bold(t: &str, size: f32) -> RichText {
    RichText::new(t).font(egui::FontId::new(size, egui::FontFamily::Name("bold".into())))
}

fn category_of(id: &str) -> String {
    TREE.iter().find(|(_, _, pages)| pages.iter().any(|(p, _)| *p == id)).map(|c| t(c.0)).unwrap_or_default()
}

/// Sidebar entry: rounded highlight, accent bar and a coloured dot.
fn nav_item(ui: &mut egui::Ui, pal: &Palette, selected: bool, label: &str, dot: Color32) -> bool {
    let w = ui.available_width();
    let (r, resp) = ui.allocate_exact_size(egui::vec2(w, 25.0), egui::Sense::click());
    let p = ui.painter();
    if selected {
        p.rect_filled(r, 8.0, pal.accent.gamma_multiply(0.16));
        p.rect_filled(
            egui::Rect::from_min_size(egui::pos2(r.left(), r.center().y - 8.0), egui::vec2(3.0, 16.0)),
            2.0,
            pal.accent,
        );
    } else if resp.hovered() {
        p.rect_filled(r, 8.0, pal.hover);
    }
    p.circle_filled(egui::pos2(r.left() + 18.0, r.center().y), 4.0, dot);
    let font = if selected {
        egui::FontId::new(13.5, egui::FontFamily::Name("bold".into()))
    } else {
        egui::FontId::proportional(13.5)
    };
    p.text(
        egui::pos2(r.left() + 32.0, r.center().y),
        egui::Align2::LEFT_CENTER,
        label,
        font,
        if selected { pal.text } else { pal.soft },
    );
    resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked()
}

fn card(pal: &Palette) -> egui::Frame {
    egui::Frame::new()
        .fill(pal.card)
        .stroke(Stroke::new(1.0_f32, pal.border))
        .corner_radius(12.0)
        .inner_margin(egui::Margin::symmetric(18, 14))
}

/// Summary tiles: the four parts people ask about first.
fn tiles(ui: &mut egui::Ui, pal: &Palette, page: &[Sec]) {
    let find = |k: &str| page.iter().flat_map(|s| s.rows.iter()).find(|r| r.key == t(k)).map(|r| r.value.clone());
    let items = [
        ("Процессор", find("Тип ЦП"), page_color("cpu")),
        ("Видеокарта", find("Видеоадаптер"), page_color("gpu")),
        ("Память", find("Системная память"), page_color("memory")),
        ("Диск", find("Дисковый накопитель"), page_color("pdisk")),
    ];
    ui.columns(4, |cols| {
        for (col, (label, value, c)) in cols.iter_mut().zip(items) {
            card(pal).show(col, |ui| {
                ui.set_width(ui.available_width());
                ui.set_min_height(64.0);
                ui.horizontal(|ui| {
                    let (r, _) = ui.allocate_exact_size(egui::vec2(10.0, 10.0), egui::Sense::hover());
                    ui.painter().circle_filled(r.center(), 4.0, c);
                    ui.label(RichText::new(t(label)).size(12.0).color(pal.muted));
                });
                ui.add_space(2.0);
                ui.with_layout(egui::Layout::top_down(egui::Align::Min), |ui| {
                    ui.add(egui::Label::new(bold(&value.unwrap_or_else(|| "—".into()), 14.0).color(pal.text)).wrap());
                });
            });
        }
    });
}

/// What the person clicked in the rows this frame.
#[derive(Default)]
struct Clicks {
    copied: Option<String>,
    pin: Option<String>,
}

/// One section as a card of rows. A click on a value copies it; the right
/// button offers to pin the row to the summary page.
fn section_card(ui: &mut egui::Ui, pal: &Palette, dot: Color32, sec: &Sec, favorites: &[String], mark: bool, clicks: &mut Clicks) {
    card(pal).show(ui, |ui| {
        ui.set_width(ui.available_width());
        let key_w = (ui.available_width() * 0.36).clamp(180.0, 320.0);
        ui.horizontal(|ui| {
            let (r, _) = ui.allocate_exact_size(egui::vec2(12.0, 12.0), egui::Sense::hover());
            ui.painter().circle_filled(r.center(), 5.0, dot);
            ui.add(egui::Label::new(bold(&sec.title, 15.0).color(pal.text)).wrap());
        });
        ui.add_space(6.0);
        let n = sec.rows.len();
        for (i, row) in sec.rows.iter().enumerate() {
            let pinned = row.pin.as_ref().map_or(false, |p| favorites.contains(p));
            ui.horizontal_top(|ui| {
                key_cell(ui, pal, key_w, &row.key, mark && pinned);
                // long values wrap inside the card
                ui.vertical(|ui| {
                    ui.set_max_width(ui.available_width());
                    ui.add_space(2.0);
                    // the marks of the advice page: met, not met
                    let colour = match row.value.chars().next() {
                        Some('✔') => Color32::from_rgb(70, 170, 100),
                        Some('✖') => Color32::from_rgb(225, 85, 85),
                        _ => pal.text,
                    };
                    let value = egui::Label::new(RichText::new(&row.value).color(colour)).wrap().sense(egui::Sense::click());
                    let resp = ui.add(value).on_hover_cursor(egui::CursorIcon::Copy);
                    if resp.clicked() {
                        clicks.copied = Some(row.value.clone());
                    }
                    resp.context_menu(|ui| {
                        if ui.button(format!("📋  {}", t("Копировать"))).clicked() {
                            clicks.copied = Some(row.value.clone());
                            ui.close_menu();
                        }
                        if let Some(pin) = &row.pin {
                            let text = if pinned { "Открепить от сводной страницы" } else { "Закрепить на сводной странице" };
                            if ui.button(format!("📌  {}", t(text))).clicked() {
                                clicks.pin = Some(pin.clone());
                                ui.close_menu();
                            }
                        }
                    });
                });
            });
            if i + 1 < n {
                ui.add_space(3.0);
                let y = ui.cursor().top();
                ui.painter().hline(ui.max_rect().x_range(), y, Stroke::new(1.0_f32, pal.border));
                ui.add_space(4.0);
            }
        }
    });
    ui.add_space(12.0);
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.frames += 1;
        // Pick up finished collection.
        let finished = std::mem::replace(&mut *self.load.lock().unwrap(), Load::Loading);
        match finished {
            Load::Ready(v) => {
                self.data = Some(v);
                self.status = "Сведения обновлены".into();
                self.loading = false;
            }
            Load::Failed(e) => {
                self.status = e;
                self.loading = false;
                self.failed = true;
            }
            Load::Loading => {}
        }

        if ctx.input(|i| i.key_pressed(egui::Key::F5)) {
            self.refresh(ctx);
        }
        // A picture of the page: this frame is drawn without personal data and photographed.
        if self.snap == Snap::Armed {
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::default()));
            self.snap = Snap::Waiting;
            self.snap_frames = 0;
        }
        let private = self.private || self.snap != Snap::Idle;
        let pal = palette(self.dark);

        // Sidebar
        egui::SidePanel::left("nav")
            .exact_width(240.0)
            .resizable(false)
            .frame(
                egui::Frame::new()
                    .fill(pal.side)
                    .stroke(Stroke::new(1.0_f32, pal.border))
                    .inner_margin(egui::Margin::symmetric(12, 16)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.add_space(6.0);
                    let (r, _) = ui.allocate_exact_size(egui::vec2(26.0, 26.0), egui::Sense::hover());
                    ui.painter().rect_filled(r, 7.0, pal.accent);
                    ui.painter().rect_filled(r.shrink(8.0), 2.0, Color32::from_white_alpha(220));
                    ui.vertical(|ui| {
                        ui.label(bold("SysLens", 16.0).color(pal.text));
                        ui.label(RichText::new(t("Сведения о системе")).size(11.5).color(pal.muted));
                    });
                });
                ui.add_space(12.0);

                egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                    for (cat, col, pages) in TREE {
                        let c = Color32::from_rgb(col[0], col[1], col[2]);
                        ui.add_space(3.0);
                        ui.horizontal(|ui| {
                            ui.add_space(10.0);
                            ui.label(RichText::new(t(cat).to_uppercase()).size(10.5).color(pal.muted));
                        });
                        for (id, title) in *pages {
                            if nav_item(ui, &pal, self.selected == *id, &t(title), c) {
                                self.selected = id;
                            }
                        }
                    }
                });
            });

        // Content
        let panel = egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(pal.bg).inner_margin(egui::Margin::symmetric(28, 20)))
            .show(ctx, |ui| {
                self.header(ctx, ui, &pal);
                if !self.status.is_empty() {
                    ui.label(RichText::new(t(&self.status)).size(12.0).color(pal.muted));
                }
                ui.add_space(10.0);
                self.toolbar(ctx, ui, &pal, private);
                ui.add_space(12.0);
                self.body(ctx, ui, &pal, private);
            });
        self.content_rect = panel.response.rect;

        self.about_window(ctx, &pal);
        self.snap_step(ctx);
        self.screenshot_step(ctx);
    }
}

impl App {
    /// Title on the left, the small switches on the right.
    fn header(&mut self, ctx: &egui::Context, ui: &mut egui::Ui, pal: &Palette) {
        ui.horizontal(|ui| {
            let title_w = (ui.available_width() - 230.0).max(120.0);
            ui.vertical(|ui| {
                ui.set_max_width(title_w);
                ui.label(RichText::new(category_of(self.selected)).size(12.0).color(pal.muted));
                let title = page_title(self.selected);
                let size = match title.chars().count() {
                    0..=14 => 24.0,
                    15..=20 => 21.0,
                    _ => 19.0,
                };
                ui.add(egui::Label::new(bold(&title, size).color(pal.text)).truncate());
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let lock = if self.private { "🔒" } else { "🔓" };
                let tip = if self.private {
                    "Личные данные скрыты (имя ПК, пользователь, серийные номера, адреса). Нажмите, чтобы показать."
                } else {
                    "Скрыть личные данные — удобно перед тем, как делиться снимком или отчётом"
                };
                if ui.selectable_label(self.private, lock).on_hover_text(t(tip)).clicked() {
                    self.private = !self.private;
                }
                let tip = "Сохранить страницу как картинку — личные данные на ней будут скрыты";
                if ui.button("📷").on_hover_text(t(tip)).clicked() && self.data.is_some() {
                    self.snap = Snap::Armed;
                    ctx.request_repaint();
                }
                ui.menu_button("⚙", |ui| self.settings_menu(ctx, ui)).response.on_hover_text(t("Настройки"));
                let cur = i18n::get();
                ui.menu_button(format!("🌐 {}", cur.short()), |ui| {
                    for l in Lang::ALL {
                        if ui.radio(cur == l, l.native()).clicked() {
                            self.set_lang(ctx, l);
                            ui.close_menu();
                        }
                    }
                })
                .response
                .on_hover_text(t("Язык"));
            });
        });
    }

    fn settings_menu(&mut self, ctx: &egui::Context, ui: &mut egui::Ui) {
        let (icon, text) = if self.dark { ("☀", "Светлая тема") } else { ("🌙", "Тёмная тема") };
        if ui.button(format!("{icon}  {}", t(text))).clicked() {
            let dark = !self.dark;
            self.set_dark(ctx, dark);
            ui.close_menu();
        }
        ui.separator();
        ui.label(RichText::new(t("Размеры дисков")).size(12.0).weak());
        let decimal = rules::decimal();
        if ui.radio(!decimal, t("как в Windows: 1 ГБ = 1024 МБ")).clicked() {
            self.set_units(false);
            ui.close_menu();
        }
        if ui.radio(decimal, t("как на коробке: 1 ГБ = 1000 МБ")).clicked() {
            self.set_units(true);
            ui.close_menu();
        }
        if tray::AVAILABLE {
            ui.separator();
            let mut on = self.tray.is_some();
            if ui.checkbox(&mut on, t("Значок возле часов с нагрузкой")).clicked() {
                self.set_tray(ctx, on);
                ui.close_menu();
            }
        }
        ui.separator();
        if ui.button(t("Проверить обновления")).clicked() {
            self.show_about = true;
            self.check_updates(ctx);
            ui.close_menu();
        }
        if ui.button(t("О программе")).clicked() {
            self.show_about = true;
            ui.close_menu();
        }
    }

    /// Search on the left, actions on the right. In a narrow window the
    /// actions keep only their icons so everything still fits.
    fn toolbar(&mut self, ctx: &egui::Context, ui: &mut egui::Ui, pal: &Palette, private: bool) {
        ui.horizontal(|ui| {
            let avail = ui.available_width();
            let icons = avail < 680.0;
            let actions_w = if icons { 200.0 } else { 490.0 };
            let edit = egui::TextEdit::singleline(&mut self.query)
                .hint_text(format!("🔍  {}", t("Поиск по всем страницам…")))
                .desired_width((avail - actions_w - 90.0).clamp(100.0, 360.0))
                .margin(egui::Margin::symmetric(10, 6));
            ui.add(edit);
            if !self.query.is_empty() && ui.button("✖").on_hover_text(t("Очистить поиск")).clicked() {
                self.query.clear();
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let label = |icon: &str, text: &str| if icons { icon.to_string() } else { format!("{icon}  {}", t(text)) };
                let save = egui::Button::new(RichText::new(label("💾", "Сохранить отчёт")).color(Color32::WHITE))
                    .fill(pal.accent)
                    .stroke(Stroke::NONE);
                if ui.add(save).on_hover_text(t("Сохранить отчёт")).clicked() {
                    self.save_report();
                }
                if ui.button("🖶").on_hover_text(t("Печать или сохранение в PDF")).clicked() {
                    self.print_report();
                }
                let mut copy: Option<bool> = None; // Some(true): the page, Some(false): the short summary
                ui.menu_button(label("📋", "Копировать"), |ui| {
                    if ui.button(t("Эту страницу")).clicked() {
                        copy = Some(true);
                        ui.close_menu();
                    }
                    if ui.button(t("Короткую сводку одной строкой")).clicked() {
                        copy = Some(false);
                        ui.close_menu();
                    }
                })
                .response
                .on_hover_text(t("Копировать"));
                if let (Some(page), Some(d)) = (copy, self.data.clone()) {
                    if page {
                        let text = info::report(&page_title(self.selected), &page_for(self.selected, &d, &self.view(private)));
                        ctx.copy_text(text);
                        self.status = "Страница скопирована в буфер обмена".into();
                    } else {
                        ctx.copy_text(tv(&info::summary_line(&d)));
                        self.status = "Короткая сводка скопирована в буфер обмена".into();
                    }
                }
                let refresh = ui
                    .add_enabled(!self.loading, egui::Button::new(label("🔄", "Обновить")))
                    .on_hover_text(format!("{} — F5", t("Обновить")));
                if refresh.clicked() {
                    self.refresh(ctx);
                }
            });
        });
    }

    /// The chosen page, or what the search found on all pages.
    fn body(&mut self, ctx: &egui::Context, ui: &mut egui::Ui, pal: &Palette, private: bool) {
        let Some(d) = self.data.clone() else {
            if self.failed {
                // Nothing to show and nothing running: say why and offer another try.
                ui.add_space(24.0);
                ui.label(RichText::new(t(&self.status)).color(pal.soft));
                ui.add_space(8.0);
                if ui.button(format!("🔄  {}", t("Повторить"))).clicked() {
                    self.refresh(ctx);
                }
            } else {
                ui.centered_and_justified(|ui| {
                    ui.horizontal_centered(|ui| {
                        ui.spinner();
                        ui.label(RichText::new(t(&self.status)).color(pal.soft));
                    });
                });
            }
            return;
        };
        let q = self.query.trim().to_lowercase();
        let searching = !q.is_empty();
        let shown_id = if searching { "" } else { self.selected };
        let favorites = self.favorites.clone();
        // A search looks through every page; otherwise the chosen page is shown.
        let (secs, pinned) = {
            let view = self.view(private);
            let secs = if searching { search_all(&d, &view, &q) } else { shown(self.selected, &d, &view) };
            let pinned = if shown_id == "summary" { favorite_rows(&favorites, &d, &view) } else { Vec::new() };
            (secs, pinned)
        };
        // the other computer's report: its file name (not shown while personal data is hidden) and date
        let other = self.other.as_ref().map(|(name, o)| (if private { String::new() } else { name.clone() }, o.made.clone()));
        let col = page_color(self.selected);
        if shown_id == "live" {
            // a new sample every second while this page is open
            let wait = self.live.get_or_insert_with(live::Live::new).tick();
            ctx.request_repaint_after(wait);
        }
        let mut clicks = Clicks::default();
        let (mut open_other, mut close_other) = (false, false);

        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            match shown_id {
                "summary" => {
                    tiles(ui, pal, &secs);
                    ui.add_space(14.0);
                    if pinned.is_empty() {
                        let hint = "Важные строки можно закрепить здесь: щёлкните правой кнопкой по значению на любой странице.";
                        ui.label(RichText::new(t(hint)).size(12.0).color(pal.muted));
                        ui.add_space(10.0);
                    } else {
                        let sec = Sec { title: t("Избранное"), rows: pinned };
                        section_card(ui, pal, pal.accent, &sec, &favorites, false, &mut clicks);
                    }
                }
                "live" => {
                    if let Some(lv) = &self.live {
                        live_page(ui, pal, lv);
                    }
                }
                "compare" => {
                    card(pal).show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        match &other {
                            Some((name, made)) => {
                                let what = [name.as_str(), made.as_str()].iter().filter(|x| !x.is_empty()).cloned().collect::<Vec<_>>().join(", ");
                                ui.add(egui::Label::new(bold(&format!("{} {what}", t("Сравнение с отчётом:")), 14.0).color(pal.text)).wrap());
                                ui.add_space(4.0);
                                let legend = "«Другой» — компьютер из отчёта, «этот» — компьютер, на котором запущена программа. Имена, серийные номера и адреса не сравниваются.";
                                ui.add(egui::Label::new(RichText::new(t(legend)).color(pal.soft)).wrap());
                                ui.add_space(8.0);
                                ui.horizontal(|ui| {
                                    if ui.button(format!("📂  {}", t("Открыть другой отчёт"))).clicked() {
                                        open_other = true;
                                    }
                                    if ui.button(format!("✖  {}", t("Закрыть сравнение"))).clicked() {
                                        close_other = true;
                                    }
                                });
                            }
                            None => {
                                let how = "Сохраните на другом компьютере отчёт в формате JSON («Сохранить отчёт» → JSON), перенесите файл сюда и откройте его — здесь будут видны различия в железе и программах.";
                                ui.add(egui::Label::new(RichText::new(t(how)).color(pal.soft)).wrap());
                                ui.add_space(8.0);
                                if ui.button(format!("📂  {}", t("Открыть отчёт другого компьютера"))).clicked() {
                                    open_other = true;
                                }
                            }
                        }
                    });
                    ui.add_space(12.0);
                }
                "apps" => {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(t("Сортировка:")).color(pal.muted));
                        for (sort, name) in [
                            (info::AppSort::Name, "по имени"),
                            (info::AppSort::Size, "по размеру"),
                            (info::AppSort::Date, "по дате"),
                            (info::AppSort::Publisher, "по издателю"),
                        ] {
                            if ui.selectable_label(self.apps_sort == sort, t(name)).clicked() {
                                self.apps_sort = sort;
                            }
                        }
                    });
                    ui.add_space(8.0);
                }
                _ => {}
            }
            for s in &secs {
                section_card(ui, pal, col, s, &favorites, true, &mut clicks);
            }
            if secs.is_empty() {
                let msg = match shown_id {
                    _ if searching => "Ничего не найдено",
                    "live" => "",
                    "compare" if other.is_some() => "Различий нет",
                    "compare" => "",
                    _ => "Нет данных",
                };
                if !msg.is_empty() {
                    ui.label(RichText::new(t(msg)).color(pal.muted));
                }
            }
        });
        if let Some(text) = clicks.copied {
            ctx.copy_text(text);
            self.status = "Значение скопировано".into();
        }
        if let Some(pin) = clicks.pin {
            self.toggle_pin(pin);
        }
        if close_other {
            self.other = None;
        }
        if open_other {
            self.open_other(None);
        }
    }

    fn about_window(&mut self, ctx: &egui::Context, pal: &Palette) {
        if !self.show_about {
            return;
        }
        let state = self.updates.lock().unwrap().clone();
        let mut open = true;
        let mut check = false;
        egui::Window::new(t("О программе"))
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
            .open(&mut open)
            .show(ctx, |ui| {
                ui.label(bold(&format!("SysLens {}", update::CURRENT), 16.0));
                ui.label(format!("{} Foxhand", t("Автор:")));
                if ui.link("github.com/foxhands/syslens").clicked() {
                    export::open("https://github.com/foxhands/syslens");
                }
                ui.add_space(6.0);
                ui.label(t("Сведения о процессоре, системной плате, памяти,"));
                ui.label(t("видеоадаптере, дисках и сети этого компьютера."));
                ui.label(RichText::new(t("F5 — обновить сведения")).color(pal.muted));
                ui.add_space(6.0);
                ui.separator();
                ui.add_space(4.0);
                match &state {
                    update::State::Idle => {}
                    update::State::Checking => {
                        ui.horizontal(|ui| {
                            ui.spinner();
                            ui.label(t("Проверяю, нет ли новой версии…"));
                        });
                    }
                    update::State::Latest => {
                        ui.label(t("У вас последняя версия."));
                    }
                    update::State::Newer(v) => {
                        ui.label(bold(&format!("{} {v}", t("Вышла новая версия:")), 14.0).color(pal.accent));
                        ui.add_space(4.0);
                        let get = egui::Button::new(RichText::new(t("Скачать новую версию")).color(Color32::WHITE))
                            .fill(pal.accent)
                            .stroke(Stroke::NONE);
                        if ui.add(get).clicked() {
                            export::open(update::RELEASES_PAGE);
                        }
                    }
                    update::State::Failed => {
                        ui.label(t("Не удалось проверить: нет связи с GitHub."));
                    }
                }
                if !matches!(state, update::State::Checking | update::State::Newer(_)) && ui.button(t("Проверить обновления")).clicked() {
                    check = true;
                }
            });
        self.show_about = open;
        if check {
            self.check_updates(ctx);
        }
    }

    /// Saves the photograph of the page once it arrives.
    fn snap_step(&mut self, ctx: &egui::Context) {
        if self.snap != Snap::Waiting {
            return;
        }
        ctx.request_repaint();
        let img = ctx.input(|i| {
            i.raw.events.iter().find_map(|e| match e {
                egui::Event::Screenshot { image, .. } => Some(image.clone()),
                _ => None,
            })
        });
        let Some(img) = img else {
            self.snap_frames += 1;
            if self.snap_frames > 300 {
                self.snap = Snap::Idle; // the system never sent it
                self.status = "Не удалось сохранить картинку".into();
            }
            return;
        };
        self.snap = Snap::Idle;
        // only the page, without the menu on the left
        let part = img.region(&self.content_rect, Some(ctx.pixels_per_point()));
        let dir = docs_dir();
        let name = format!("SysLens-{}", self.selected);
        let path = match export::ask_path(&t("Сохранить страницу как картинку"), &name, &dir, export::PICTURE_KINDS) {
            export::Ask::Chosen(p) => p,
            export::Ask::Cancelled => return,
            export::Ask::NoDialog => export::free_name(&dir, &name, "png"),
        };
        let raw: Vec<u8> = part.pixels.iter().flat_map(|c| c.to_array()).collect();
        let [w, h] = part.size;
        self.status = match image::save_buffer(&path, &raw, w as u32, h as u32, image::ExtendedColorType::Rgba8) {
            Ok(_) => format!("Картинка сохранена: {}", path.display()),
            Err(e) => format!("Не удалось сохранить картинку: {e}"),
        };
    }
}

impl App {
    fn screenshot_step(&mut self, ctx: &egui::Context) {
        let Some(path) = self.shot.clone() else { return };
        ctx.request_repaint();
        if self.data.is_some() || self.frames > 3000 {
            self.ready_frames += 1;
        }
        // A window with no area (still opening, or minimised by the system)
        // can't be read back; wait until it has a real size.
        let area = ctx.input(|i| i.screen_rect());
        let minimized = ctx.input(|i| i.viewport().minimized).unwrap_or(false);
        if area.width() < 50.0 || area.height() < 50.0 || minimized {
            if self.ready_frames % 60 == 59 {
                ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
                ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(1100.0, 720.0)));
            }
            return;
        }
        // The load page needs a few seconds of samples before it has a chart to show.
        let sampled = self.selected != "live" || self.live.as_ref().map_or(false, |l| l.history.len() >= 4);
        if !self.shot_sent && self.ready_frames >= 120 && sampled {
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::default()));
            self.shot_sent = true;
        }
        let img = ctx.input(|i| {
            i.raw.events.iter().find_map(|e| match e {
                egui::Event::Screenshot { image, .. } => Some(image.clone()),
                _ => None,
            })
        });
        if let Some(img) = img {
            if let Some(dir) = path.parent() {
                let _ = std::fs::create_dir_all(dir);
            }
            let [w, h] = img.size;
            let raw: Vec<u8> = img.pixels.iter().flat_map(|c| c.to_array()).collect();
            let _ = image::save_buffer(&path, &raw, w as u32, h as u32, image::ExtendedColorType::Rgba8);
            std::process::exit(0);
        }
    }
}

/// The name column of a row: fixed width, cut with "…" when too long.
/// A pinned row carries a pin.
fn key_cell(ui: &mut egui::Ui, pal: &Palette, key_w: f32, k: &str, pinned: bool) {
    ui.allocate_ui_with_layout(egui::vec2(key_w, 22.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
        ui.set_min_width(key_w);
        let text = if pinned { format!("📌 {k}") } else { k.to_string() };
        ui.add(egui::Label::new(RichText::new(text).color(pal.muted)).truncate());
    });
    ui.add_space(12.0);
}

/// A text of the live page in the current language ("9.1 ГБ из 16 ГБ").
fn tv(s: &str) -> String {
    i18n::value("", s, i18n::get())
}

/// A filled bar: how much of something is used. Nearly full turns red.
fn bar(ui: &mut egui::Ui, pal: &Palette, width: f32, share: f32) {
    let share = if share.is_finite() { share.clamp(0.0, 1.0) } else { 0.0 };
    let (r, _) = ui.allocate_exact_size(egui::vec2(width, 10.0), egui::Sense::hover());
    ui.painter().rect_filled(r, 5.0, pal.border);
    if share > 0.0 {
        let fill = if share >= 0.9 { Color32::from_rgb(225, 85, 85) } else { pal.accent };
        let w = (r.width() * share).max(6.0);
        ui.painter().rect_filled(egui::Rect::from_min_size(r.min, egui::vec2(w, r.height())), 5.0, fill);
    }
}

/// A row of the live page: name, bar, text.
fn bar_row(ui: &mut egui::Ui, pal: &Palette, key_w: f32, name: &str, share: f32, text: &str) {
    ui.horizontal(|ui| {
        key_cell(ui, pal, key_w, name, false);
        let w = (ui.available_width() * 0.4).clamp(80.0, 280.0);
        bar(ui, pal, w, share);
        ui.add_space(8.0);
        ui.label(RichText::new(text).color(pal.text));
    });
    ui.add_space(4.0);
}

fn card_title(ui: &mut egui::Ui, pal: &Palette, dot: Color32, title: &str, right: &str) {
    ui.horizontal(|ui| {
        let (r, _) = ui.allocate_exact_size(egui::vec2(12.0, 12.0), egui::Sense::hover());
        ui.painter().circle_filled(r.center(), 5.0, dot);
        ui.label(bold(title, 15.0).color(pal.text));
        if !right.is_empty() {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(bold(right, 18.0).color(pal.text));
            });
        }
    });
    ui.add_space(8.0);
}

/// The "load now" page: processor with a one-minute chart and its cores,
/// memory, network speeds and disk space.
fn live_page(ui: &mut egui::Ui, pal: &Palette, lv: &live::Live) {
    let share = |used: u64, total: u64| if total > 0 { used as f32 / total as f32 } else { 0.0 };
    let of = |used: u64, total: u64| tv(&format!("{} из {}", rules::mem_size(used), rules::mem_size(total)));

    card(pal).show(ui, |ui| {
        ui.set_width(ui.available_width());
        card_title(ui, pal, page_color("cpu"), &t("Процессор"), &format!("{} %", lv.cpu.round() as u32));
        // the last minute, newest on the right
        let (r, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 70.0), egui::Sense::hover());
        let p = ui.painter();
        p.rect_filled(r, 6.0, pal.bg);
        for line in 1..4 {
            let y = r.top() + r.height() * line as f32 / 4.0;
            p.hline(r.x_range(), y, Stroke::new(1.0_f32, pal.border));
        }
        let step = r.width() / (live::HISTORY - 1) as f32;
        let start = live::HISTORY - lv.history.len();
        let points: Vec<egui::Pos2> = lv
            .history
            .iter()
            .enumerate()
            .map(|(i, v)| egui::pos2(r.left() + step * (start + i) as f32, r.bottom() - 2.0 - (r.height() - 4.0) * (v / 100.0).clamp(0.0, 1.0)))
            .collect();
        if points.len() > 1 {
            p.add(egui::Shape::line(points, Stroke::new(2.0_f32, pal.accent)));
        }
        ui.add_space(10.0);
        ui.label(RichText::new(t("Загрузка по ядрам")).size(12.0).color(pal.muted));
        ui.add_space(2.0);
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = egui::vec2(6.0, 6.0);
            for (i, c) in lv.cores.iter().enumerate() {
                let (r, resp) = ui.allocate_exact_size(egui::vec2(46.0, 10.0), egui::Sense::hover());
                ui.painter().rect_filled(r, 5.0, pal.border);
                let w = r.width() * (c / 100.0).clamp(0.0, 1.0);
                if w > 0.5 {
                    ui.painter().rect_filled(egui::Rect::from_min_size(r.min, egui::vec2(w.max(4.0), r.height())), 5.0, pal.accent);
                }
                resp.on_hover_text(format!("{}: {} %", i + 1, c.round() as u32));
            }
        });
    });
    ui.add_space(12.0);

    card(pal).show(ui, |ui| {
        ui.set_width(ui.available_width());
        let key_w = (ui.available_width() * 0.36).clamp(180.0, 320.0);
        card_title(ui, pal, page_color("memory"), &t("Память"), "");
        bar_row(ui, pal, key_w, &t("Оперативная память"), share(lv.mem_used, lv.mem_total), &of(lv.mem_used, lv.mem_total));
        if lv.swap_total > 0 {
            bar_row(ui, pal, key_w, &t("Файл подкачки"), share(lv.swap_used, lv.swap_total), &of(lv.swap_used, lv.swap_total));
        }
    });
    ui.add_space(12.0);

    if !lv.nets_now.is_empty() {
        card(pal).show(ui, |ui| {
            ui.set_width(ui.available_width());
            let key_w = (ui.available_width() * 0.36).clamp(180.0, 320.0);
            card_title(ui, pal, page_color("net"), &t("Сеть"), "");
            for n in &lv.nets_now {
                ui.horizontal(|ui| {
                    key_cell(ui, pal, key_w, &n.name, false);
                    let text = tv(&format!("↓ {}    ↑ {}", rules::rate(n.down), rules::rate(n.up)));
                    ui.label(RichText::new(text).color(pal.text));
                });
                ui.add_space(4.0);
            }
        });
        ui.add_space(12.0);
    }

    if !lv.disks_now.is_empty() {
        card(pal).show(ui, |ui| {
            ui.set_width(ui.available_width());
            let key_w = (ui.available_width() * 0.36).clamp(180.0, 320.0);
            card_title(ui, pal, page_color("pdisk"), &t("Диски"), "");
            for d in &lv.disks_now {
                let used = d.total.saturating_sub(d.free);
                bar_row(ui, pal, key_w, &d.name, share(used, d.total), &tv(&rules::space_line(d.total, d.free)));
            }
        });
        ui.add_space(12.0);
    }
}

/// `--report file`: collects the information and writes the report without
/// opening a window. The kind of file follows its name (.txt, .html, .json).
fn headless_report(path: &std::path::Path, private: bool) -> Result<(), String> {
    let d = info::collect().map_err(|e| t(&e))?;
    let bytes = report_bytes(path, &d, private, info::AppSort::Name);
    if let Some(dir) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        let _ = std::fs::create_dir_all(dir);
    }
    std::fs::write(path, bytes).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn sample() -> Value {
        json!({
            "cs": { "Name": "ANNA-PC" },
            "cpu": [{ "Name": "X", "NumberOfCores": 4 }],
            "apps": [{ "Name": "A", "Version": "1" }, { "Name": "B", "Version": "2" }],
        })
    }

    #[test]
    fn pinned_rows_are_found_again() {
        let d = sample();
        let view = View { private: false, sort: info::AppSort::Name, other: None };
        let cpu = shown("cpu", &d, &view);
        assert_eq!(cpu[0].rows[0].pin.as_deref(), Some("cpu\tСвойства ЦП\tТип ЦП"));
        // the count in "Установленные программы (2)" is not part of a pin: it changes
        let favorites =
            ["apps\tУстановленные программы\tB".to_string(), "cpu\tСвойства ЦП\tТип ЦП".to_string(), "cpu\tНет\tНет".to_string()];
        let rows = favorite_rows(&favorites, &d, &view);
        assert_eq!(rows.iter().map(|r| (r.key.as_str(), r.value.as_str())).collect::<Vec<_>>(), [("B", "версия 2"), ("Тип ЦП", "X")]);
        // judgements and differences have no fixed place to pin
        assert!(shown("advice", &d, &view).iter().flat_map(|s| s.rows.iter()).all(|r| r.pin.is_none()));
    }

    #[test]
    fn search_looks_through_all_pages() {
        let d = sample();
        let view = View { private: true, sort: info::AppSort::Name, other: None };
        let found = search_all(&d, &view, "версия 2");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].rows[0].key, "B");
        // hidden values are not found by what they hide
        assert!(search_all(&d, &view, "anna-pc").is_empty());
    }

    #[test]
    fn json_report_can_be_compared_with() {
        let d = sample();
        let bytes = report_bytes(std::path::Path::new("r.json"), &d, true, info::AppSort::Name);
        let other = diff::parse(&serde_json::from_slice(&bytes).unwrap()).unwrap();
        let view = View { private: false, sort: info::AppSort::Name, other: Some(&other) };
        assert!(shown("compare", &d, &view).is_empty()); // the same computer: no differences
        assert!(!String::from_utf8_lossy(&bytes).contains("ANNA-PC"));
    }

    #[test]
    fn every_page_has_a_place_in_the_menu() {
        for id in SPECIAL {
            assert!(TREE.iter().any(|c| c.2.iter().any(|p| p.0 == *id)), "{id}");
        }
        assert!(compared_pages().iter().all(|p| p.0 != "advice" && p.0 != "live"));
    }
}

fn arg<'a>(args: &'a [String], name: &str) -> Option<&'a String> {
    args.iter().position(|a| a == name).and_then(|i| args.get(i + 1))
}

fn main() -> eframe::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let shot = arg(&args, "--screenshot").map(PathBuf::from);

    // Language: --lang en|ru|uk|de|es|pl, else the saved one, else the system's.
    let lang = arg(&args, "--lang").and_then(|c| Lang::from_code(c)).unwrap_or_else(load_lang);
    i18n::set(lang);
    // Disk sizes: --units decimal|binary, else the saved choice.
    let units = arg(&args, "--units").cloned().or_else(|| setting("units.txt"));
    rules::set_decimal(units.as_deref() == Some("decimal"));

    // --icon file.png: the program's icon as a picture, for packaging.
    if let Some(path) = arg(&args, "--icon") {
        let ok = image::save_buffer(path, &icon::app(256), 256, 256, image::ExtendedColorType::Rgba8).is_ok();
        std::process::exit(if ok { 0 } else { 1 });
    }

    let mut vp = egui::ViewportBuilder::default()
        .with_title(t("SysLens — сведения о системе"))
        .with_inner_size([1100.0, 720.0])
        .with_min_inner_size([700.0, 450.0])
        .with_icon(egui::IconData { rgba: icon::app(64), width: 64, height: 64 });
    if shot.is_some() {
        vp = vp.with_active(false);
        #[cfg(windows)]
        {
            vp = vp.with_window_level(egui::WindowLevel::AlwaysOnBottom);
        }
    }
    let private = args.iter().any(|a| a == "--private");
    // --report file: write the report and leave, no window.
    if let Some(path) = arg(&args, "--report") {
        match headless_report(std::path::Path::new(path), private) {
            Ok(()) => std::process::exit(0),
            Err(e) => {
                eprintln!("SysLens: {e}");
                std::process::exit(1);
            }
        }
    }
    // Optional start page: --page apps
    let start: Option<&'static str> =
        arg(&args, "--page").and_then(|want| TREE.iter().flat_map(|t| t.2.iter()).find(|(id, _)| id == want).map(|(id, _)| *id));
    // --compare file.json: open another computer's report straight away.
    let compare = arg(&args, "--compare").map(PathBuf::from);
    let options = eframe::NativeOptions { viewport: vp, ..Default::default() };
    eframe::run_native(
        "SysLens",
        options,
        Box::new(move |cc| {
            let mut app = App::new(cc, shot);
            if let Some(path) = compare {
                app.open_other(Some(path));
            }
            if let Some(id) = start {
                app.selected = id;
            }
            app.private = private;
            Ok(Box::new(app))
        }),
    )
}
