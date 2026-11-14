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
        thread: Option<std::thread::JoinHandle<()>>,
    }

    impl Tray {
        pub fn start(ctx: egui::Context) -> Tray {
            let stop = Arc::new(AtomicBool::new(false));
            let flag = stop.clone();
            let thread = Some(std::thread::spawn(move || run(ctx, flag)));
            Tray { stop, thread }
        }
    }

    impl Drop for Tray {
        /// Waits for the thread, which takes the icon away as it ends;
        /// left alone at exit it would die first and leave a ghost icon.
        fn drop(&mut self) {
            self.stop.store(true, Ordering::Relaxed);
            if let Some(thread) = self.thread.take() {
                let _ = thread.join();
            }
        }
    }

    fn picture(cpu: f32, memory: f32) -> Option<Icon> {
        Icon::from_rgba(icon::meter(cpu, memory), icon::METER, icon::METER).ok()
    }

    fn run(ctx: egui::Context, stop: Arc<AtomicBool>) {
        let Some(first) = picture(0.0, 0.0) else { return };
        let Ok(tray) = TrayIconBuilder::new().with_tooltip("SysLens").with_icon(first).build() else { return };
        // The clicks are read from the crate's channel in the loop. Its event
        // handler can be set only once per process, so it would not survive
        // the icon being switched off and on again.
        let events = TrayIconEvent::receiver();
        // what nobody read while the icon was off
        while events.try_recv().is_ok() {}
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
            // a click on the icon brings the window back
            while let Ok(event) = events.try_recv() {
                if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = event {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
                    ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
                    ctx.request_repaint();
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

    /// The icon's thread ends when the icon is dropped, also the second time
    /// (off and on again in the settings). Run by hand, as the one above.
    #[test]
    #[ignore]
    fn tray_stops_with_its_thread() {
        for _ in 0..2 {
            let tray = super::Tray::start(eframe::egui::Context::default());
            std::thread::sleep(std::time::Duration::from_millis(1500));
            let begun = std::time::Instant::now();
            drop(tray);
            assert!(begun.elapsed() < std::time::Duration::from_secs(2));
        }
    }
}
