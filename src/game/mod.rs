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
//! use hydrargyros::game::component::Component;
//! use hydrargyros::game::{Engine, Entity, HeadlessWindow};
//! use hydrargyros::{o, Var};
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
#[cfg(feature = "physics")]
pub mod physics_rapier;
pub mod physics;
pub mod physics_body;
pub mod render;
#[cfg(feature = "wgpu")]
pub mod render_wgpu;
#[cfg(feature = "stream")]
pub mod stream_window;
#[cfg(feature = "tray")]
pub mod tray;
#[cfg(feature = "tray")]
pub use tray::spawn_tray_quit;
#[cfg(feature = "winit")]
pub mod winit_window;
pub mod transform;
pub mod window;

pub use component::{Component, Priority};
pub use engine::Engine;
pub use entity::Entity;
pub use physics::PhysicsWorld;
pub use physics_body::PhysicsBody;
pub use render::{Quad, RenderBackend};
#[cfg(feature = "wgpu")]
pub use render::FrameSink;
#[cfg(feature = "wgpu")]
pub use render_wgpu::WgpuRenderer;
#[cfg(feature = "stream")]
pub use stream_window::StreamingWindow;
#[cfg(feature = "winit")]
pub use winit_window::{
    key_var, mouse_button_var, mouse_move_var, quit_var, wheel_var, WinitWindow,
};
pub use transform::Transform;
pub use window::{create_window_backend, register_window_backend, HeadlessWindow, WindowSystem};
