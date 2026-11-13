//! The icon next to the clock (Windows): a small meter of processor and
//! memory load, with the numbers in its tooltip. A click brings the window
//! back. It lives on its own thread, so it keeps counting while the window
//! is minimised.

#[cfg(windows)]
mod imp {
    use crate::i18n::t;
    use crate::icon;
    use eframe::egui;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;
    use std::time::{Duration, Instant};
    use tray_icon::{Icon, MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};

    /// One Windows message, as PeekMessageW fills it.
    #[repr(C)]
    struct Msg {
        hwnd: isize,
        message: u32,
        wparam: usize,
        lparam: isize,
        time: u32,
        pt: [i32; 2],
        private: u32,
    }

    #[link(name = "user32")]
    extern "system" {
        fn PeekMessageW(msg: *mut Msg, hwnd: isize, min: u32, max: u32, remove: u32) -> i32;
        fn TranslateMessage(msg: *const Msg) -> i32;
        fn DispatchMessageW(msg: *const Msg) -> isize;
    }

    pub struct Tray {
        stop: Arc<AtomicBool>,
    }

    impl Tray {
        pub fn start(ctx: egui::Context) -> Tray {
            let stop = Arc::new(AtomicBool::new(false));
            let flag = stop.clone();
            std::thread::spawn(move || run(ctx, flag));
            Tray { stop }
        }
    }

    impl Drop for Tray {
        fn drop(&mut self) {
            self.stop.store(true, Ordering::Relaxed);
        }
    }

    fn picture(cpu: f32, memory: f32) -> Option<Icon> {
        Icon::from_rgba(icon::meter(cpu, memory), icon::METER, icon::METER).ok()
    }

    fn run(ctx: egui::Context, stop: Arc<AtomicBool>) {
        let Some(first) = picture(0.0, 0.0) else { return };
        let Ok(tray) = TrayIconBuilder::new().with_tooltip("SysLens").with_icon(first).build() else { return };
        // a click on the icon brings the window back
        TrayIconEvent::set_event_handler(Some(move |e: TrayIconEvent| {
            if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = e {
                ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
                ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
                ctx.request_repaint();
            }
        }));
        let mut sys = sysinfo::System::new();
        sys.refresh_cpu_usage();
        let mut last = Instant::now();
        let mut shown = (u32::MAX, u32::MAX);
        while !stop.load(Ordering::Relaxed) {
            // the icon's clicks arrive as window messages of this thread
            // SAFETY: plain Win32 message loop; `msg` is a valid, writable MSG.
            unsafe {
                let mut msg: Msg = std::mem::zeroed();
                while PeekMessageW(&mut msg, 0, 0, 0, 1) != 0 {
                    TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                }
            }
            if last.elapsed() >= Duration::from_secs(1) {
                last = Instant::now();
                sys.refresh_cpu_usage();
                sys.refresh_memory();
                let cpu = sys.global_cpu_usage().clamp(0.0, 100.0);
                let memory = if sys.total_memory() > 0 { sys.used_memory() as f32 / sys.total_memory() as f32 * 100.0 } else { 0.0 };
                let now = (cpu.round() as u32, memory.round() as u32);
                if now != shown {
                    shown = now;
                    if let Some(p) = picture(cpu / 100.0, memory / 100.0) {
                        let _ = tray.set_icon(Some(p));
                    }
                    let _ = tray.set_tooltip(Some(format!("SysLens\n{}: {} %\n{}: {} %", t("Процессор"), now.0, t("Память"), now.1)));
                }
            }
            std::thread::sleep(Duration::from_millis(40));
        }
        TrayIconEvent::set_event_handler(None::<fn(TrayIconEvent)>);
    }
}

#[cfg(windows)]
pub use imp::Tray;

/// Whether this system has such an icon.
pub const AVAILABLE: bool = cfg!(windows);

/// Elsewhere there is no icon.
#[cfg(not(windows))]
pub struct Tray;

#[cfg(not(windows))]
impl Tray {
    pub fn start(_ctx: eframe::egui::Context) -> Tray {
        Tray
    }
}

#[cfg(all(test, windows))]
mod tests {
    use tray_icon::{Icon, TrayIconBuilder};

    /// Puts a real icon next to the clock for a moment. Run by hand:
    /// `cargo test tray -- --ignored` (it needs a desktop session).
    #[test]
    #[ignore]
    fn icon_can_be_shown_and_changed() {
        let picture = |cpu, memory| Icon::from_rgba(crate::icon::meter(cpu, memory), crate::icon::METER, crate::icon::METER).unwrap();
        let tray = TrayIconBuilder::new().with_tooltip("SysLens").with_icon(picture(0.1, 0.2)).build().unwrap();
        tray.set_icon(Some(picture(0.9, 0.5))).unwrap();
        tray.set_tooltip(Some("SysLens\nCPU: 90 %\nMemory: 50 %")).unwrap();
    }
}
