//! The game module — gold's engine core, as Rust traits and registries.
//!
//! The shape follows gold's `game` module: an [`Engine`] pumps frames
//! (poll window events → physics → sort/init components → priority-ordered
//! `update` → draw pass → frame cap), [`Component`]s hang off [`Entity`]s
//! with [`Transform`] hierarchies, and subsystems are swappable backends
//! behind `register`/`create` registries — gold's pattern, as Rust traits.
//!
//! Headless first, like gold's `windowSystemHeadless`: the whole loop runs
//! and tests without a window or GPU. The wgpu render backend is the next
//! rung; events already cross the boundary as `Var` (the thesis — a `quit`
//! event is just `o!{ "type": "quit" }`).
//!
//! ```
//! use mercury::game::component::Component;
//! use mercury::game::{Engine, Entity, HeadlessWindow};
//! use mercury::{o, Var};
//!
//! struct Counter(u64);
//! impl Component for Counter {
//!     fn update(&mut self, _dt: f64) { self.0 += 1; }
//! }
//!
//! let mut engine = Engine::new("acme", "demo");
//! engine.frame_time(0.0); // headless: no vsync wait
//! let mut e = Entity::new("counter");
//! e.add_component(Box::new(Counter(0)));
//! engine.add_entity(e);
//!
//! // scripted window: one key event, then an automatic quit
//! engine.start_with(Box::new(HeadlessWindow::with_events([
//!     Var::from(o!{ "type": "key", "key": "w" }),
//! ])));
//! ```

pub mod component;
pub mod engine;
pub mod entity;
pub mod transform;
pub mod window;

pub use component::{Component, Priority};
pub use engine::Engine;
pub use entity::Entity;
pub use transform::Transform;
pub use window::{create_window_backend, register_window_backend, HeadlessWindow, WindowSystem};
