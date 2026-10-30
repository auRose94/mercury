//! The engine — gold's `engine`: the pump + dispatch loop.
//!
//! gold's frame, ported: poll window events (`quit` stops the loop),
//! physics step (no physics backend yet), sort + initialize components,
//! priority-ordered `update` with the frame's delta time, the draw pass,
//! then the frame-rate cap (`frameTime` config, gold's default 16 ms).

use std::path::PathBuf;
use std::time::{Duration, Instant};

use super::component::Priority;
use super::entity::Entity;
use super::window::{create_window_backend, WindowSystem};
use crate::var::Var;

/// Tree path to a component slot: `[top_entity_slot, child_idx, child_slot, …]`.
type SlotPath = Vec<usize>;

/// The game engine (gold's `engine`): entities, the pump, settings.
pub struct Engine {
    company: String,
    game_name: String,
    entities: Vec<Entity>,
    running: bool,
    /// Frame cap in milliseconds (gold's `frameTime`; 0 = uncapped, used by
    /// headless runs and tests).
    frame_time: f64,
    settings_root: PathBuf,
}

impl Engine {
    /// An engine for `company`/`game_name` — the settings namespace
    /// (gold's `boot(company, gameName)`).
    pub fn new(company: impl Into<String>, game_name: impl Into<String>) -> Engine {
        Engine {
            company: company.into(),
            game_name: game_name.into(),
            entities: Vec::new(),
            running: false,
            frame_time: 16.0,
            settings_root: PathBuf::from("."),
        }
    }

    /// Attach a top-level entity (gold's `engine += [component, entity]`).
    pub fn add_entity(&mut self, entity: Entity) -> &mut Self {
        self.entities.push(entity);
        self
    }

    pub fn entities(&self) -> &[Entity] { &self.entities }

    /// The frame cap in milliseconds; `0` disables it (headless runs).
    pub fn frame_time(&mut self, ms: f64) -> &mut Self {
        self.frame_time = ms;
        self
    }

    /// Where settings files live (default: the working directory).
    pub fn settings_root(&mut self, path: impl Into<PathBuf>) -> &mut Self {
        self.settings_root = path.into();
        self
    }

    pub fn running(&self) -> bool { self.running }

    /// Set `running` false — the pump exits after the current frame (gold's
    /// quit semantics: the frame in flight still completes).
    pub fn stop(&mut self) { self.running = false; }

    /// The settings file path: `{root}/{company}/{gameName}/settings.json`.
    pub fn settings_path(&self) -> PathBuf {
        self.settings_root.join(&self.company).join(&self.game_name).join("settings.json")
    }

    /// Save settings as JSON (gold's `saveSettings`).
    ///
    /// # Errors
    /// Filesystem trouble along the settings path.
    pub fn save_settings(&self, settings: &Var) -> std::io::Result<()> {
        let path = self.settings_path();
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let bytes = serde_json::to_vec_pretty(settings)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        std::fs::write(path, bytes)
    }

    /// Load settings (gold's `loadSettings`); `Null` when absent.
    ///
    /// # Errors
    /// Filesystem trouble, or unparsable JSON.
    pub fn load_settings(&self) -> std::io::Result<Var> {
        match std::fs::read(self.settings_path()) {
            Ok(bytes) => serde_json::from_slice(&bytes)
                .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Var::Null),
            Err(e) => Err(e),
        }
    }

    /// The pump (gold's `start`): creates the named window backend and runs
    /// frames until a `quit` event or [`stop`](Self::stop). Blocks.
    ///
    /// # Panics
    /// On an unknown window backend name (the registry's job to report
    /// earlier) — prefer creating the backend yourself with
    /// [`create_window_backend`] and calling [`start_with`](Self::start_with).
    pub fn start(&mut self, backend_name: &str, config: &Var) {
        let window = create_window_backend(backend_name, config)
            .unwrap_or_else(|e| panic!("cannot start engine: {e}"));
        self.start_with(window);
    }

    /// The pump with a window you made yourself — the headless path.
    pub fn start_with(&mut self, mut window: Box<dyn WindowSystem>) {
        self.running = true;
        let frame_interval = Duration::from_secs_f64(self.frame_time / 1000.0);
        let mut last = Instant::now();

        while self.running {
            // events: quit stops the loop after this frame (gold's semantics)
            for event in window.poll() {
                if event.get("type").as_str() == Some("quit") {
                    self.running = false;
                }
                window.handle_event(&event);
            }

            let dt = last.elapsed().as_secs_f64();
            self.update(dt);
            self.draw();

            if self.frame_time > 0.0 {
                let elapsed = last.elapsed();
                if elapsed < frame_interval {
                    std::thread::sleep(frame_interval - elapsed);
                }
            }
            last = Instant::now();
        }
    }

    /// One frame's update pass: sort components by priority (stable, every
    /// frame — priorities can change), initialize newcomers, then update
    /// every enabled component with the frame's delta time. Public so tests
    /// and headless code can drive frames without a window.
    pub fn update(&mut self, dt: f64) {
        for (_, path) in self.update_order() {
            let slot = self.slot_at_mut(&path);
            if !slot.initialized {
                slot.component.initialize();
                slot.initialized = true;
            }
            slot.component.update(dt);
        }
    }

    /// One frame's draw pass: draw-priority components, after all updates.
    pub fn draw(&mut self) {
        for (priority, path) in self.update_order() {
            if priority == Priority::Draw.value() {
                self.slot_at_mut(&path).component.draw();
            }
        }
    }

    /// The slot at a tree path.
    fn slot_at_mut(&mut self, path: &[usize]) -> &mut super::entity::ComponentSlot {
        let [entity_idx, rest @ ..] = path else {
            unreachable!("non-empty slot paths only")
        };
        self.entities[*entity_idx].slot_at_mut(rest)
    }

    /// Flat slot paths over the whole tree, in priority order — gold's
    /// `sortComponents` across the flat component list.
    fn update_order(&self) -> Vec<(i64, SlotPath)> {
        let mut order: Vec<(i64, SlotPath)> = Vec::new();
        let mut path = Vec::new();
        for (entity_idx, entity) in self.entities.iter().enumerate() {
            path.push(entity_idx);
            entity.collect_order(&mut path, &mut order);
            path.pop();
        }
        order.sort_by_key(|(p, _)| *p);
        order
    }
}
