//! The streaming window: the headless render buffer becomes a browser
//! page. A ball falls under rapier physics, the wgpu renderer draws it
//! offscreen, and a tiny web app streams the frames while browser keys and
//! mouse events flow back into the engine through the normal
//! `WindowSystem::poll` seam — the engine can't tell it apart from SDL.
//!
//! Run:
//!   cargo run --example game_stream --features physics,wgpu,stream
//!
//! The page opens automatically (http://127.0.0.1:8400). Watch the ball
//! fall; press keys, click around — events print in the console; hit the
//! page's quit button to end the pump.

use mercury::game::physics::Shape as PhysShape;
use mercury::game::render::{FrameSink, Quad};
use mercury::game::RenderBackend;
use mercury::game::{Engine, Entity, PhysicsBody, PhysicsWorld, WgpuRenderer};
use mercury::var::Vec3;
use mercury::Var;

/// World → clip mapping shared by the whole scene: x ∈ [-8, 8], y ∈ [-1, 9],
/// with +y UP on the screen (world and screen agree, so gravity reads
/// as "down").
fn world_quad(pos: Vec3, half_extents: Vec3, color: [f32; 4]) -> Quad {
    Quad {
        pos: [pos.x / 8.0, 2.0 * (pos.y + 1.0) / 10.0 - 1.0],
        size: [half_extents.x / 8.0, 2.0 * half_extents.y / 10.0],
        color,
    }
}

const FLOOR_COLOR: [f32; 4] = [0.30, 0.34, 0.40, 1.0];
const BALL_COLOR: [f32; 4] = [1.0, 0.4, 0.1, 1.0];
const FLOOR_HALF: Vec3 = Vec3 { x: 6.0, y: 0.5, z: 6.0 };
const BALL_HALF: Vec3 = Vec3 { x: 0.5, y: 0.5, z: 0.5 };


fn main() {
    let mut engine = Engine::new("acme", "stream-demo");
    engine.frame_time(0.0);
    engine.fixed_dt(1.0 / 60.0); // 60 fps of simulated time, however fast we render
    engine.set_physics(PhysicsWorld::create("rapier", &Var::Null).unwrap());

    let mut floor = Entity::new("floor");
    floor.add_component(Box::new(
        PhysicsBody::new("floor", PhysShape::Box(FLOOR_HALF)).static_body(),
    ));
    engine.add_entity(floor);

    let mut ball = Entity::new("ball");
    ball.transform_mut().position = Vec3::new(0.0, 8.0, 0.0);
    ball.add_component(Box::new(PhysicsBody::new("ball", PhysShape::Sphere(BALL_HALF.x))));
    engine.add_entity(ball);

    // the offscreen renderer publishes every frame into the sink
    let sink = FrameSink::new();
    let mut renderer = pollster::block_on(WgpuRenderer::new_offscreen(480, 270))
        .expect("no wgpu adapter on this machine");
    renderer.attach_sink(sink.clone());

    #[cfg(not(feature = "tray"))]
    println!("(build with --features tray for a system-tray quit button)");

    // the streaming window: browser events come back as Var.
    // --no-open skips the auto-launch (paste the URL into any browser —
    // including an embedded one like ZCode's).
    let open_browser = !std::env::args().any(|a| a == "--no-open");
    let window =
        mercury::game::StreamingWindow::new("127.0.0.1:8400", sink, open_browser)
            .expect("stream server");
    let window = std::sync::Arc::new(window);


    // tray icon: Quit reaches the engine even with no visible window
    #[cfg(feature = "tray")]
    {
        let window = window.clone();
        match mercury::game::spawn_tray_quit("mercury (streaming)", move |quit| {
            window.inject(quit);
        }) {
            Ok(()) => println!("tray icon up (Quit closes the game)"),
            Err(e) => println!("tray unavailable: {e}"),
        }
    }

    println!("streaming at {}{}", window.url(),
        if open_browser { " (opened in your browser)" } else { " (paste into a browser)" });

    let mut running = true;
    let mut events_seen = 0u32;
    while running {
        // browser events arrive through the normal windowing seam
        for event in window.drain() {
            events_seen += 1;
            let kind = event.get("type").as_string().unwrap_or_default();
            match kind.as_str() {
                "quit" => {
                    println!("quit requested after {events_seen} events");
                    running = false;
                }
                "key" => println!(
                    "key {:?} {}",
                    event.get("key").as_string().unwrap_or_default(),
                    if event.get("pressed").as_bool() == Some(true) { "down" } else { "up" }
                ),
                "mouse" => println!(
                    "mouse {} at ({}, {})",
                    event.get("action").as_string().unwrap_or_default(),
                    event.get("x").as_i64().unwrap_or(0),
                    event.get("y").as_i64().unwrap_or(0),
                ),
                other => println!("event {other:?}"),
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
            world_quad(Vec3::new(0.0, 0.0, 0.0), FLOOR_HALF, FLOOR_COLOR),
            world_quad(ball_pos, BALL_HALF, BALL_COLOR),
        ]);
        renderer.render_frame();
        std::thread::sleep(std::time::Duration::from_millis(16));
    }

    println!("done: {events_seen} browser events handled");
}
