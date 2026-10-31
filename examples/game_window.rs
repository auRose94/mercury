//! A classic desktop window: winit owns the main thread (macOS requires
//! it), the engine pump runs on another thread through the ordinary
//! `WindowSystem` seam, and the wgpu renderer presents straight to the
//! window surface. Same game as game_stream — a ball falls under rapier —
//! but pixels go to your compositor instead of a browser.
//!
//! Run:
//!   cargo run --example game_window --features physics,wgpu,winit

use std::thread;

use mercury::game::physics::Shape;
use mercury::game::render::Quad;
use mercury::game::winit_window::WinitWindow;
use mercury::game::window::WindowSystem;
use mercury::game::RenderBackend;
use mercury::game::{Engine, Entity, PhysicsBody, PhysicsWorld, WgpuRenderer};
use mercury::var::Vec3;
use mercury::Var;

fn main() {
    // the event loop + window: created and run on the main thread
    let window = match WinitWindow::new("mercury — game window", (800, 450)) {
        Ok(w) => w,
        Err(e) => {
            eprintln!("no windowing system: {e}");
            std::process::exit(1);
        }
    };
    println!("waiting for the window…");

    // the pump: engine + renderer on another thread
    // the pump handle doubles as the closer (both share the loop's state)
    let mut pump_window = window.pump_handle();
    let closer = window.pump_handle();
    let pump = thread::spawn(move || {
        // wait for the window (created when the event loop resumes)
        let mut winit_window = None;
        for _ in 0..200 {
            if let Some(w) = closer.window() {
                winit_window = Some(w);
                break;
            }
            thread::sleep(std::time::Duration::from_millis(10));
        }
        let Some(winit_window) = winit_window else {
            eprintln!("window never appeared");
            closer.request_close();
            return;
        };

        let mut renderer = match pollster::block_on(WgpuRenderer::new_for_window(
            winit_window,
        )) {
            Ok(r) => r,
            Err(e) => {
                eprintln!("no wgpu adapter for the window: {e}");
                closer.request_close();
                return;
            }
        };

        let mut engine = Engine::new("acme", "window-demo");
        engine.frame_time(0.0);
        engine.fixed_dt(1.0 / 60.0);
        engine.set_physics(PhysicsWorld::create("rapier", &Var::Null).unwrap());

        // world → clip: x ∈ [-8, 8], y ∈ [-1, 9], +y up on screen
        fn world_quad(pos: Vec3, half_extents: Vec3, color: [f32; 4]) -> Quad {
            Quad {
                pos: [pos.x / 8.0, 2.0 * (pos.y + 1.0) / 10.0 - 1.0],
                size: [half_extents.x / 8.0, 2.0 * half_extents.y / 10.0],
                color,
            }
        }
        const FLOOR_COLOR: [f32; 4] = [0.30, 0.34, 0.40, 1.0];
        const BALL_COLOR: [f32; 4] = [1.0, 0.4, 0.1, 1.0];

        let mut floor = Entity::new("floor");
        floor.add_component(Box::new(
            PhysicsBody::new("floor", Shape::Box(Vec3::new(6.0, 0.5, 6.0))).static_body(),
        ));
        engine.add_entity(floor);

        let mut ball = Entity::new("ball");
        ball.transform_mut().position = Vec3::new(0.0, 8.0, 0.0);
        ball.add_component(Box::new(PhysicsBody::new("ball", Shape::Sphere(0.5))));
        engine.add_entity(ball);

        println!("pumping: watch the ball fall; close the window to quit");
        let mut running = true;
        while running {
            // windowing seam: winit events arrive as Vars
            for event in pump_window.poll() {
                if event.get("type").as_string().as_deref() == Some("quit") {
                    running = false;
                }
            }
            if !running {
                break;
            }

            engine.update(1.0 / 60.0);
            engine.draw();

            // the scene: floor + ball, +y up on screen (matches physics)
            let ball_pos = engine
                .physics_mut()
                .and_then(|w| w.body_pose("ball"))
                .map(|(pos, _)| pos)
                .unwrap_or(Vec3::new(0.0, 8.0, 0.0));
            renderer.set_scene(&[
                world_quad(Vec3::new(0.0, 0.0, 0.0), Vec3::new(6.0, 0.5, 6.0), FLOOR_COLOR),
                world_quad(ball_pos, Vec3::splat(0.5), BALL_COLOR),
            ]);
            renderer.render_frame();
            thread::sleep(std::time::Duration::from_millis(16));
        }
        closer.request_close();
    });

    // blocks until the pump asks to close or the user closes the window
    window.run();
    let _ = pump.join();
    println!("done");
}
