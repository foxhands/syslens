#![windows_subsystem = "windows"]

mod i18n;
mod info;
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
    ("Компьютер", [70, 120, 200], &[("summary", "Суммарная информация")]),
    (
        "Системная плата",
        [60, 150, 90],
        &[("cpu", "ЦП"), ("board", "Системная плата"), ("memory", "Память"), ("spd", "SPD"), ("bios", "BIOS")],
    ),
    ("Операционная система", [40, 140, 210], &[("os", "Операционная система")]),
    ("Отображение", [200, 110, 40], &[("gpu", "Видеоадаптер"), ("monitor", "Монитор")]),
    ("Мультимедиа", [170, 70, 170], &[("audio", "Звуковые устройства")]),
    ("Хранение данных", [120, 120, 130], &[("pdisk", "Физические диски"), ("ldisk", "Логические диски")]),
    ("Сеть", [40, 160, 160], &[("net", "Сетевые адаптеры")]),
    ("Программы", [220, 170, 40], &[("apps", "Установленные программы")]),
];

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
        let Some(d) = &self.data else { return };
        let mut text = format!("{}\r\n\r\n", t("Отчёт о системе (SysLens)"));
        for (_, _, pages) in TREE {
            for (id, title) in *pages {
                text.push_str(&info::report(&t(title), &page_for(id, d, self.private)));
            }
        }
        let mut dir = home().join("Documents");
        if !dir.is_dir() {
            dir = home();
        }
        let path = dir.join(t("SysLens-отчёт.txt"));
        let mut bytes = vec![0xEF, 0xBB, 0xBF];
        bytes.extend_from_slice(text.as_bytes());
        self.status = match std::fs::write(&path, bytes) {
            Ok(_) => format!("Отчёт сохранён: {}", path.display()),
            Err(e) => format!("Не удалось сохранить отчёт: {e}"),
        };
    }
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
fn page_for(id: &str, d: &serde_json::Value, private: bool) -> info::Page {
    let mut page = info::page(id, d);
    if private {
        let user = std::env::var(if cfg!(windows) { "USERNAME" } else { "USER" }).unwrap_or_default();
        hide_personal(&mut page, &user);
    }
    let l = i18n::get();
    if l != Lang::Ru {
        for s in &mut page.sections {
            s.title = i18n::tr(&s.title, l);
            for (k, v) in &mut s.rows {
                *v = i18n::value(k, v, l);
                // on the programs page the "field" is a program's own name
                if id != "apps" {
                    *k = i18n::tr(k, l);
                }
            }
        }
    }
    page
}

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
                        .hint_text(format!("🔍  {}", t("Поиск на этой странице…")))
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
                                ctx.copy_text(info::report(&page_title(self.selected), &page_for(self.selected, d, self.private)));
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
                let mut page = page_for(self.selected, &d, self.private);
                let q = self.query.trim().to_lowercase();
                let searching = !q.is_empty();
                if searching {
                    for s in &mut page.sections {
                        let title_hit = s.title.to_lowercase().contains(&q);
                        if !title_hit {
                            s.rows.retain(|(k, v)| k.to_lowercase().contains(&q) || v.to_lowercase().contains(&q));
                        }
                    }
                    page.sections.retain(|s| !s.rows.is_empty());
                }
                let col = page_color(self.selected);

                egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                    if self.selected == "summary" && !searching {
                        tiles(ui, &pal, &page);
                        ui.add_space(14.0);
                    }
                    for s in &page.sections {
                        card(&pal).show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            let key_w = (ui.available_width() * 0.36).clamp(180.0, 320.0);
                            ui.horizontal(|ui| {
                                let (r, _) = ui.allocate_exact_size(egui::vec2(12.0, 12.0), egui::Sense::hover());
                                ui.painter().circle_filled(r.center(), 5.0, col);
                                ui.label(bold(&s.title, 15.0).color(pal.text));
                            });
                            ui.add_space(6.0);
                            let n = s.rows.len();
                            for (i, (k, v)) in s.rows.iter().enumerate() {
                                ui.horizontal(|ui| {
                                    ui.allocate_ui_with_layout(
                                        egui::vec2(key_w, 22.0),
                                        egui::Layout::left_to_right(egui::Align::Center),
                                        |ui| {
                                            ui.set_min_width(key_w);
                                            ui.add(egui::Label::new(RichText::new(k).color(pal.muted)).truncate());
                                        },
                                    );
                                    ui.add_space(12.0);
                                    ui.add(egui::Label::new(RichText::new(v).color(pal.text)).selectable(true));
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
                        let msg = if searching { "Ничего не найдено" } else { "Нет данных" };
                        ui.label(RichText::new(t(msg)).color(pal.muted));
                    }
                });
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
        if !self.shot_sent && self.ready_frames >= 120 {
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
