#![windows_subsystem = "windows"]

mod diff;
mod export;
mod i18n;
mod info;
mod live;
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
mod linux;
mod rules;

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
        &[("summary", "Суммарная информация"), ("live", "Нагрузка сейчас"), ("changes", "Изменения"), ("battery", "Батарея")],
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
/// they stay out of the report, the search and the comparison.
const SPECIAL: &[&str] = &["live", "changes"];

/// Every ordinary page: (id, title).
fn fact_pages() -> impl Iterator<Item = (&'static str, &'static str)> {
    TREE.iter().flat_map(|c| c.2.iter().copied()).filter(|(id, _)| !SPECIAL.contains(id))
}

/// How pages are shown right now.
struct View<'a> {
    private: bool,
    sort: info::AppSort,
    /// The saved snapshot the "changes" page compares with.
    baseline: Option<&'a Value>,
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
    /// The saved snapshot, if one was ever taken.
    baseline: Option<Arc<Value>>,
    /// Live load; starts sampling when its page is first opened.
    live: Option<live::Live>,
}

fn baseline_file() -> PathBuf {
    config_dir().join("baseline.json")
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

fn theme_file() -> PathBuf {
    config_dir().join("theme.txt")
}

/// Dark unless the person switched to light earlier.
fn load_dark() -> bool {
    std::fs::read_to_string(theme_file()).map(|s| s.trim() != "light").unwrap_or(true)
}

fn save_dark(dark: bool) {
    let f = theme_file();
    if let Some(dir) = f.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let _ = std::fs::write(f, if dark { "dark" } else { "light" });
}

fn lang_file() -> PathBuf {
    theme_file().with_file_name("lang.txt")
}

/// The saved language, or the Windows display language the first time.
fn load_lang() -> Lang {
    std::fs::read_to_string(lang_file()).ok().and_then(|s| Lang::from_code(&s)).unwrap_or_else(i18n::system)
}

impl App {
    fn set_lang(&mut self, ctx: &egui::Context, l: Lang) {
        i18n::set(l);
        let f = lang_file();
        if let Some(dir) = f.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let _ = std::fs::write(f, l.code());
        ctx.send_viewport_cmd(egui::ViewportCommand::Title(t("SysLens — сведения о системе")));
    }

    fn set_dark(&mut self, ctx: &egui::Context, dark: bool) {
        self.dark = dark;
        apply_theme(ctx, dark);
        save_dark(dark);
    }

    fn new(cc: &eframe::CreationContext, shot: Option<PathBuf>) -> Self {
        setup_fonts(&cc.egui_ctx);
        let dark = load_dark();
        apply_theme(&cc.egui_ctx, dark);
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
            baseline: std::fs::read(baseline_file()).ok().and_then(|b| serde_json::from_slice(&b).ok()).map(Arc::new),
            live: None,
        };
        app.refresh(&cc.egui_ctx);
        app
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
        let mut dir = home().join("Documents");
        if !dir.is_dir() {
            dir = home();
        }
        let name = t("SysLens-отчёт");
        // The person picks the place and the kind of file: text, web page or JSON.
        let path = match export::ask_path(&t("Сохранить отчёт"), &name, &dir) {
            export::Ask::Chosen(p) => p,
            export::Ask::Cancelled => return,
            export::Ask::NoDialog => export::free_name(&dir, &name, "txt"),
        };
        let view = View { private: self.private, sort: self.apps_sort, baseline: None };
        let bytes = export::render(export::format_of(&path), &t("Отчёт о системе (SysLens)"), &all_pages(&d, &view));
        self.status = match std::fs::write(&path, bytes) {
            Ok(_) => format!("Отчёт сохранён: {}", path.display()),
            Err(e) => format!("Не удалось сохранить отчёт: {e}"),
        };
    }

    /// Remembers the computer as it is now, to show later what has changed.
    fn save_baseline(&mut self) {
        let Some(d) = self.data.clone() else { return };
        let f = baseline_file();
        if let Some(dir) = f.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        self.status = match std::fs::write(&f, serde_json::to_vec(&*d).unwrap_or_default()) {
            Ok(_) => {
                self.baseline = Some(d);
                "Снимок сохранён".into()
            }
            Err(e) => format!("Не удалось сохранить снимок: {e}"),
        };
    }
}

/// Every ordinary page with its title, as shown: for the report.
fn all_pages(d: &Value, view: &View) -> Vec<(String, info::Page)> {
    fact_pages().map(|(id, title)| (t(title), page_for(id, d, view))).collect()
}

/// Rows from all pages that contain `q` (lower case); a section whose title
/// matches is kept whole. Sections are titled "page  ›  section".
fn search_all(d: &Value, view: &View, q: &str) -> info::Page {
    let mut out = info::Page { sections: Vec::new() };
    for (id, title) in fact_pages() {
        for mut s in page_for(id, d, view).sections {
            if !s.title.to_lowercase().contains(q) {
                s.rows.retain(|(k, v)| k.to_lowercase().contains(q) || v.to_lowercase().contains(q));
            }
            if !s.rows.is_empty() {
                let page = t(title);
                if s.title != page {
                    s.title = format!("{page}  ›  {}", s.title);
                }
                out.sections.push(s);
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

/// Rows that identify this computer or its owner.
const PERSONAL: &[&str] = &[
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

/// A page, with personal values replaced when `private` is on
/// (for screenshots and reports that will be shared).
fn page_for(id: &str, d: &serde_json::Value, view: &View) -> info::Page {
    let mut page = match (id, view.baseline) {
        ("changes", Some(old)) => diff::compare(&fact_pages().collect::<Vec<_>>(), old, d),
        _ => info::page_sorted(id, d, view.sort),
    };
    if view.private {
        let user = std::env::var(if cfg!(windows) { "USERNAME" } else { "USER" }).unwrap_or_default();
        hide_personal(&mut page, &user);
    }
    let l = i18n::get();
    if l != Lang::Ru {
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

/// Replaces everything that identifies the computer or its owner.
fn hide_personal(page: &mut info::Page, user: &str) {
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
fn hide_user(s: &str, user: &str) -> String {
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
fn tiles(ui: &mut egui::Ui, pal: &Palette, page: &info::Page) {
    let find = |k: &str| {
        page.sections.iter().flat_map(|s| s.rows.iter()).find(|r| r.0 == t(k)).map(|r| r.1.clone())
    };
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
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(pal.bg).inner_margin(egui::Margin::symmetric(28, 20)))
            .show(ctx, |ui| {
                // Header: title on the left, the small switches on the right.
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
                        if ui.button("ℹ").on_hover_text(t("О программе")).clicked() {
                            self.show_about = true;
                        }
                        let (icon, tip) = if self.dark { ("☀", "Светлая тема") } else { ("🌙", "Тёмная тема") };
                        if ui.button(icon).on_hover_text(t(tip)).clicked() {
                            let d = !self.dark;
                            self.set_dark(ctx, d);
                        }
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
                if !self.status.is_empty() {
                    ui.label(RichText::new(t(&self.status)).size(12.0).color(pal.muted));
                }
                ui.add_space(10.0);
                // Search on the left, actions on the right. In a narrow window
                // the actions keep only their icons so everything still fits.
                ui.horizontal(|ui| {
                    let avail = ui.available_width();
                    let icons = avail < 640.0;
                    let actions_w = if icons { 150.0 } else { 440.0 };
                    let edit = egui::TextEdit::singleline(&mut self.query)
                        .hint_text(format!("🔍  {}", t("Поиск по всем страницам…")))
                        .desired_width((avail - actions_w - 90.0).clamp(100.0, 360.0))
                        .margin(egui::Margin::symmetric(10, 6));
                    ui.add(edit);
                    if !self.query.is_empty() && ui.button("✖").on_hover_text(t("Очистить поиск")).clicked() {
                        self.query.clear();
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let label = |icon: &str, text: &str| {
                            if icons { icon.to_string() } else { format!("{icon}  {}", t(text)) }
                        };
                        let save = egui::Button::new(RichText::new(label("💾", "Сохранить отчёт")).color(Color32::WHITE))
                            .fill(pal.accent)
                            .stroke(Stroke::NONE);
                        if ui.add(save).on_hover_text(t("Сохранить отчёт")).clicked() {
                            self.save_report();
                        }
                        if ui.button(label("📋", "Копировать")).on_hover_text(t("Копировать")).clicked() {
                            if let Some(d) = &self.data {
                                let view = View { private: self.private, sort: self.apps_sort, baseline: self.baseline.as_deref() };
                                ctx.copy_text(info::report(&page_title(self.selected), &page_for(self.selected, d, &view)));
                                self.status = "Страница скопирована в буфер обмена".into();
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
                ui.add_space(12.0);

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
                let baseline = self.baseline.clone();
                let view = View { private: self.private, sort: self.apps_sort, baseline: baseline.as_deref() };
                let q = self.query.trim().to_lowercase();
                let searching = !q.is_empty();
                // A search looks through every page; otherwise the chosen page is shown.
                let page = if searching { search_all(&d, &view, &q) } else { page_for(self.selected, &d, &view) };
                let col = page_color(self.selected);
                let shown = if searching { "" } else { self.selected };
                if shown == "live" {
                    // a new sample every second while this page is open
                    let wait = self.live.get_or_insert_with(live::Live::new).tick();
                    ctx.request_repaint_after(wait);
                }
                let mut copied: Option<String> = None;
                let mut remember = false;

                egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                    match shown {
                        "summary" => {
                            tiles(ui, &pal, &page);
                            ui.add_space(14.0);
                        }
                        "live" => {
                            if let Some(lv) = &self.live {
                                live_page(ui, &pal, lv);
                            }
                        }
                        "changes" => {
                            card(&pal).show(ui, |ui| {
                                ui.set_width(ui.available_width());
                                let text = match &baseline {
                                    Some(b) => format!("{} {}", t("Сравнение со снимком от"), diff::taken(b)),
                                    None => t("Снимка ещё нет. Запомните текущее состояние — позже здесь будет видно, что изменилось: программы, драйверы, устройства."),
                                };
                                ui.add(egui::Label::new(RichText::new(text).color(pal.soft)).wrap());
                                ui.add_space(8.0);
                                if ui.button(format!("📌  {}", t("Запомнить текущее состояние"))).clicked() {
                                    remember = true;
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
                    for s in &page.sections {
                        card(&pal).show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            let key_w = (ui.available_width() * 0.36).clamp(180.0, 320.0);
                            ui.horizontal(|ui| {
                                let (r, _) = ui.allocate_exact_size(egui::vec2(12.0, 12.0), egui::Sense::hover());
                                ui.painter().circle_filled(r.center(), 5.0, col);
                                ui.add(egui::Label::new(bold(&s.title, 15.0).color(pal.text)).wrap());
                            });
                            ui.add_space(6.0);
                            let n = s.rows.len();
                            for (i, (k, v)) in s.rows.iter().enumerate() {
                                ui.horizontal_top(|ui| {
                                    key_cell(ui, &pal, key_w, k);
                                    // long values wrap inside the card; a click on a value copies it
                                    ui.vertical(|ui| {
                                        ui.set_max_width(ui.available_width());
                                        ui.add_space(2.0);
                                        let value = egui::Label::new(RichText::new(v).color(pal.text)).wrap().sense(egui::Sense::click());
                                        if ui.add(value).on_hover_cursor(egui::CursorIcon::Copy).clicked() {
                                            copied = Some(v.clone());
                                        }
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
                    if page.sections.is_empty() {
                        let msg = match shown {
                            _ if searching => "Ничего не найдено",
                            "live" => "",
                            "changes" if baseline.is_some() => "Изменений нет",
                            "changes" => "",
                            _ => "Нет данных",
                        };
                        if !msg.is_empty() {
                            ui.label(RichText::new(t(msg)).color(pal.muted));
                        }
                    }
                });
                if let Some(text) = copied {
                    ctx.copy_text(text);
                    self.status = "Значение скопировано".into();
                }
                if remember {
                    self.save_baseline();
                }
            });

        if self.show_about {
            egui::Window::new(t("О программе"))
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
                .open(&mut self.show_about)
                .show(ctx, |ui| {
                    ui.label(bold("SysLens", 16.0));
                    ui.label(t("Сведения о процессоре, системной плате, памяти,"));
                    ui.label(t("видеоадаптере, дисках и сети этого компьютера."));
                    ui.label(RichText::new(t("F5 — обновить сведения")).color(pal.muted));
                });
        }

        self.screenshot_step(ctx);
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
fn key_cell(ui: &mut egui::Ui, pal: &Palette, key_w: f32, k: &str) {
    ui.allocate_ui_with_layout(egui::vec2(key_w, 22.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
        ui.set_min_width(key_w);
        ui.add(egui::Label::new(RichText::new(k).color(pal.muted)).truncate());
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
        key_cell(ui, pal, key_w, name);
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
    let of = |used: u64, total: u64| tv(&format!("{} из {}", rules::size(used), rules::size(total)));

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
                    key_cell(ui, pal, key_w, &n.name);
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
    let view = View { private, sort: info::AppSort::Name, baseline: None };
    let bytes = export::render(export::format_of(path), &t("Отчёт о системе (SysLens)"), &all_pages(&d, &view));
    if let Some(dir) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        let _ = std::fs::create_dir_all(dir);
    }
    std::fs::write(path, bytes).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let mut page = info::Page {
            sections: vec![info::Section {
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

fn main() -> eframe::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let shot = args
        .iter()
        .position(|a| a == "--screenshot")
        .and_then(|i| args.get(i + 1))
        .map(PathBuf::from);

    // Language: --lang en|ru|uk, else the saved one, else the Windows language.
    let lang = args
        .iter()
        .position(|a| a == "--lang")
        .and_then(|i| args.get(i + 1))
        .and_then(|c| Lang::from_code(c))
        .unwrap_or_else(load_lang);
    i18n::set(lang);

    let mut vp = egui::ViewportBuilder::default()
        .with_title(t("SysLens — сведения о системе"))
        .with_inner_size([1100.0, 720.0])
        .with_min_inner_size([700.0, 450.0]);
    if shot.is_some() {
        vp = vp.with_active(false);
        #[cfg(windows)]
        {
            vp = vp.with_window_level(egui::WindowLevel::AlwaysOnBottom);
        }
    }
    let private = args.iter().any(|a| a == "--private");
    // --report file: write the report and leave, no window.
    if let Some(path) = args.iter().position(|a| a == "--report").and_then(|i| args.get(i + 1)) {
        match headless_report(std::path::Path::new(path), private) {
            Ok(()) => std::process::exit(0),
            Err(e) => {
                eprintln!("SysLens: {e}");
                std::process::exit(1);
            }
        }
    }
    // Optional start page: --page apps
    let start: Option<&'static str> = args
        .iter()
        .position(|a| a == "--page")
        .and_then(|i| args.get(i + 1))
        .and_then(|want| TREE.iter().flat_map(|t| t.2.iter()).find(|(id, _)| id == want).map(|(id, _)| *id));
    let options = eframe::NativeOptions { viewport: vp, ..Default::default() };
    eframe::run_native(
        "SysLens",
        options,
        Box::new(move |cc| {
            let mut app = App::new(cc, shot);
            if let Some(id) = start {
                app.selected = id;
            }
            app.private = private;
            Ok(Box::new(app))
        }),
    )
}
