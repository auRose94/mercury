//! The system tray: a ☿ glyph icon with a Quit menu for headless windows
//! (the streaming window has no close button of its own). Behind the `tray`
//! feature; the glyph is SDF-rendered on a transparent background — no
//! asset files. On Linux the menu runs over libappindicator (GTK + DBus),
//! which the tray thread owns.

use tray_icon::menu::{Menu, MenuEvent, MenuItem};
use tray_icon::TrayIconBuilder;

use crate::var::Var;

/// ☿ in a 32×32 RGBA buffer: mercury-silver glyph, fully transparent
/// background. Drawn with anti-aliased signed-distance fields — the ring,
/// the horns arc above it, and the cross below.
fn hydrargyros_glyph() -> Vec<u8> {
    const SIZE: u32 = 32;
    // mercury silver, slightly warm
    let (r, g, b) = (220u8, 224u8, 232u8);

    // signed distance to a ring: |distance from center - radius|
    let ring = |x: f32, y: f32, cx: f32, cy: f32, r: f32, half: f32| -> f32 {
        let d = ((x - cx) * (x - cx) + (y - cy) * (y - cy)).sqrt() - r;
        d.abs() - half
    };
    // signed distance to a capsule (segment with thickness)
    let capsule = |px: f32, py: f32, ax: f32, ay: f32, bx: f32, by: f32, half: f32| -> f32 {
        let (abx, aby) = (bx - ax, by - ay);
        let (apx, apy) = (px - ax, py - ay);
        let t = ((apx * abx + apy * aby) / (abx * abx + aby * aby)).clamp(0.0, 1.0);
        let (cx, cy) = (ax + abx * t, ay + aby * t);
        ((px - cx) * (px - cx) + (py - cy) * (py - cy)).sqrt() - half
    };

    let aa = 0.75; // anti-aliasing band, in pixels
    let coverage = |d: f32| ((0.5 - d / aa).clamp(0.0, 1.0) * 255.0) as u8;

    let mut rgba = Vec::with_capacity((SIZE * SIZE * 4) as usize);
    for y in 0..SIZE {
        for x in 0..SIZE {
            let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
            // ☿ proportions on a 32×32 grid: horns arc (16, 9.5) r 6.5;
            // ring (16, 17.5) r 6; stem down to y=30 with a crossbar at 28
            let mut d = ring(px, py, 16.0, 17.5, 6.0, 1.4).min(
                ring(px, py, 16.0, 9.5, 6.5, 1.3),
            );
            d = d.min(capsule(px, py, 16.0, 24.0, 16.0, 30.0, 1.3));
            d = d.min(capsule(px, py, 12.0, 28.0, 20.0, 28.0, 1.3));
            let a = if d <= 0.0 { 255 } else { coverage(d) };
            rgba.extend_from_slice(&[r, g, b, a]);
        }
    }
    rgba
}

/// Build a tray icon titled `tooltip` with a single `Quit` menu item.
/// `on_quit` runs (on the tray's event thread) whenever Quit is clicked —
/// hand it a closure that reaches the engine, e.g.
/// [`StreamingWindow::inject`](crate::game::StreamingWindow::inject).
///
/// # Errors
/// The tray icon could not be created (no system tray, or the platform's
/// initialization failed).
pub fn spawn_tray_quit<F>(tooltip: &str, on_quit: F) -> Result<(), String>
where
    F: Fn(Var) + Send + 'static,
{
    let tooltip = tooltip.to_string();
    std::thread::Builder::new()
        .name("hydrargyros-tray".to_string())
        .spawn(move || {
            // Linux: the tray lives in GTK, which needs init + a main loop on
            // this thread. Headless machines fail here — degrade gracefully.
            #[cfg(target_os = "linux")]
            if let Err(e) = gtk::init() {
                eprintln!("tray unavailable (gtk init): {e}");
                return;
            }

            match build_tray(&tooltip, on_quit) {
                Ok(tray) => {
                    std::mem::forget(tray); // one tray per process, lives forever
                    #[cfg(target_os = "linux")]
                    gtk::main();
                    // non-Linux: menu events flow through the global receiver
                }
                Err(e) => eprintln!("tray unavailable: {e}"),
            }
        })
        .map_err(|e| format!("tray thread: {e}"))?;
    Ok(())
}

fn build_tray<F>(tooltip: &str, on_quit: F) -> Result<tray_icon::TrayIcon, String>
where
    F: Fn(Var) + Send + 'static,
{
    let icon = tray_icon::Icon::from_rgba(hydrargyros_glyph(), 32, 32)
        .map_err(|e| format!("tray icon: {e}"))?;

    let menu = Menu::new();
    let quit = MenuItem::new("Quit", true, None);
    menu.append_items(&[&quit])
        .map_err(|e| format!("tray append: {e}"))?;

    let tray = TrayIconBuilder::new()
        .with_tooltip(tooltip)
        .with_icon(icon)
        .with_menu(Box::new(menu))
        .build()
        .map_err(|e| format!("tray build: {e}"))?;

    // the menu id → remember which item is Quit
    let quit_id = quit.id().clone();

    // menu events are global; pump them on a thread and forward quits
    std::thread::Builder::new()
        .name("hydrargyros-tray-events".to_string())
        .spawn(move || {
            let receiver = MenuEvent::receiver();
            while let Ok(event) = receiver.recv() {
                if event.id == quit_id {
                    let ev = crate::object::Object::new();
                    ev.set("type", Var::from("quit"));
                    on_quit(Var::from(ev));
                }
            }
        })
        .map_err(|e| format!("tray event thread: {e}"))?;

    Ok(tray)
}
