//! The system tray: a Quit menu for headless windows (the streaming window
//! has no close button of its own). Behind the `tray` feature; the icon is
//! generated — no asset files. On Linux the menu runs over libappindicator
//! (GTK + DBus), which this thread owns.

use tray_icon::menu::{Menu, MenuEvent, MenuItem};
use tray_icon::TrayIconBuilder;

use crate::var::Var;

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
        .name("mercury-tray".to_string())
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
    // the icon: a 32×32 orange ball on the mercury dark background —
    // generated, so there are no asset files
    let mut rgba = Vec::with_capacity(32 * 32 * 4);
    for y in 0..32u32 {
        for x in 0..32u32 {
            let dx = x as f32 - 15.5;
            let dy = y as f32 - 15.5;
            let on_ball = dx * dx + dy * dy <= 12.0 * 12.0;
            if on_ball {
                rgba.extend_from_slice(&[255, 102, 26, 255]);
            } else {
                rgba.extend_from_slice(&[26, 28, 32, 255]);
            }
        }
    }
    let icon =
        tray_icon::Icon::from_rgba(rgba, 32, 32).map_err(|e| format!("tray icon: {e}"))?;

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
        .name("mercury-tray-events".to_string())
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
