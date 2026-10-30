//! Window backends — gold's registered-backend pattern, as Rust traits and
//! a register/create registry. Events cross the boundary as `Var` objects
//! (`{ "type": "quit" }`, `{ "type": "key", "key": "w" }`, …); the headless
//! backend (gold ships one too) runs the whole loop in tests without a GPU.

use std::collections::HashMap;
use std::collections::VecDeque;
use std::sync::{Arc, OnceLock, RwLock};

use crate::object::Object;
use crate::var::Var;

/// A window/display backend: the engine polls it for events each frame and
/// hands events back for its own handling (gold's `windowSystem`).
pub trait WindowSystem: Send {
    /// All events queued since the last poll, as `Var` objects.
    fn poll(&mut self) -> Vec<Var>;

    /// The window's own reaction to an event (the engine forwards every
    /// polled event here, after checking for `"quit"` itself).
    fn handle_event(&mut self, event: &Var);
}

/// Factory shape for the backend registry: config `Var` in, backend out.
pub type WindowFactory =
    Arc<dyn Fn(&Var) -> Result<Box<dyn WindowSystem>, String> + Send + Sync>;

fn backends() -> &'static RwLock<HashMap<String, WindowFactory>> {
    static BACKENDS: OnceLock<RwLock<HashMap<String, WindowFactory>>> = OnceLock::new();
    BACKENDS.get_or_init(|| {
        let mut map = HashMap::new();
        map.insert("headless".to_string(), headless_factory());
        RwLock::new(map)
    })
}

fn headless_factory() -> WindowFactory {
    Arc::new(|_config| Ok(Box::new(HeadlessWindow::new())))
}

/// Register a window backend under a name (gold's `registerBackend`).
pub fn register_window_backend(name: &str, factory: WindowFactory) {
    backends().write().unwrap().insert(name.to_string(), factory);
}

/// Create a window backend by name (gold's `createBackend`).
///
/// # Errors
/// Unknown backend name, or the backend's own factory failure.
pub fn create_window_backend(name: &str, config: &Var) -> Result<Box<dyn WindowSystem>, String> {
    let factory = backends()
        .read()
        .unwrap()
        .get(name)
        .cloned()
        .ok_or_else(|| format!("unknown window backend {name:?}"))?;
    factory(config)
}

/// The no-window backend (gold's `windowSystemHeadless`): no events unless
/// scripted, and an automatic `quit` once the scripted ones run out, so
/// `Engine::start` terminates. Frame behavior is fully deterministic.
pub struct HeadlessWindow {
    events: VecDeque<Var>,
    quit_on_empty: bool,
    quit_sent: bool,
}

impl HeadlessWindow {
    /// A window that never emits events and never quits — pair with
    /// `Engine::stop` from a component, or drive frames with
    /// [`Engine::update`](crate::game::Engine::update) directly.
    pub fn new() -> Self {
        HeadlessWindow { events: VecDeque::new(), quit_on_empty: false, quit_sent: false }
    }

    /// A scripted window: emits the given events (all on the first poll),
    /// then a `quit` event on the next poll so `Engine::start` terminates.
    pub fn with_events(events: impl IntoIterator<Item = Var>) -> Self {
        HeadlessWindow {
            events: events.into_iter().collect(),
            quit_on_empty: true,
            quit_sent: false,
        }
    }
}

impl Default for HeadlessWindow {
    fn default() -> Self { Self::new() }
}

impl WindowSystem for HeadlessWindow {
    fn poll(&mut self) -> Vec<Var> {
        if !self.events.is_empty() {
            return self.events.drain(..).collect();
        }
        if self.quit_on_empty && !self.quit_sent {
            self.quit_sent = true;
            let ev = Object::new();
            ev.set("type", Var::from("quit"));
            return vec![Var::from(ev)];
        }
        Vec::new()
    }

    fn handle_event(&mut self, _event: &Var) {}
}
