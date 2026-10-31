//! Rendering-pipeline tests: the camera maps world → pixels, circles clip
//! to discs, and entities render themselves through the full
//! transform → camera → pixel pipeline.

#![cfg(feature = "wgpu")]

use hydrargyros::game::camera::Camera;
use hydrargyros::game::renderable::MeshRenderer;
use hydrargyros::game::HeadlessWindow;
use hydrargyros::game::{Engine, Entity, PhysicsWorld, RenderBackend, StreamingWindow, WgpuRenderer};
use hydrargyros::var::Vec3;
use hydrargyros::o;

fn pixel(frame: &[u8], width: u32, x: u32, y: u32) -> [u8; 4] {
    let i = ((y * width + x) * 4) as usize;
    [frame[i], frame[i + 1], frame[i + 2], frame[i + 3]]
}

fn srgb8(linear: f64) -> u8 {
    let v = if linear <= 0.003_130_8 {
        12.92 * linear
    } else {
        1.055 * linear.powf(1.0 / 2.4) - 0.055
    };
    (v * 255.0).round() as u8
}

fn assert_close(got: [u8; 4], want: [u8; 4], what: &str) {
    for (g, w) in got.iter().zip(want) {
        assert!((*g as i32 - w as i32).abs() <= 3, "{what}: {got:?} vs {want:?}");
    }
}

/// The camera maps the world window onto the canvas: view center → canvas
/// center, view edges → edges.
#[test]
fn camera_maps_world_to_clip() {
    let camera = Camera {
        position: Vec3::new(0.0, 4.0, 0.0),
        half_extents: Vec3::new(8.0, 4.5, 1.0),
    };
    let center = camera.world_to_clip(Vec3::new(0.0, 4.0, 0.0));
    assert_eq!(center, [0.0, 0.0]);

    // view corners: +y up on the screen
    let top_left = camera.world_to_clip(Vec3::new(-8.0, 8.5, 0.0));
    assert_eq!(top_left, [-1.0, 1.0]);
    let bottom_right = camera.world_to_clip(Vec3::new(8.0, -0.5, 0.0));
    assert_eq!(bottom_right, [1.0, -1.0]);
}

#[tokio::test]
async fn world_space_quad_lands_where_the_camera_says() {
    // camera centered at (0, 4), half-extents (8, 4.5): a quad at the view
    // center must land at the canvas center
    let mut renderer = pollster::block_on(WgpuRenderer::new_offscreen(480, 270))
        .expect("no wgpu adapter on this machine");
    renderer.set_clear_color([0.0, 0.0, 0.0, 1.0]);
    renderer.begin_scene(&Camera {
        position: Vec3::new(0.0, 4.0, 0.0),
        half_extents: Vec3::new(8.0, 4.5, 1.0),
    });
    renderer.push_quad(hydrargyros::game::Quad::rect(
        [0.0, 4.0],
        [0.5, 0.5],
        [1.0, 0.0, 0.0, 1.0],
    ));
    renderer.render_frame();

    let frame = renderer.screenshot();
    // canvas center: inside the quad → red
    assert_close(pixel(frame, 480, 240, 135), [srgb8(1.0), 0, 0, 255], "center");
    // a corner: clear
    assert_close(pixel(frame, 480, 10, 10), [0, 0, 0, 255], "corner");
}

#[tokio::test]
async fn circle_shape_clips_to_a_disc() {
    let mut renderer = pollster::block_on(WgpuRenderer::new_offscreen(240, 240))
        .expect("no wgpu adapter on this machine");
    renderer.set_clear_color([0.0, 0.0, 0.0, 1.0]);
    renderer.set_scene(&[hydrargyros::game::Quad::circle(
        [0.0, 0.0],
        [0.5, 0.5],
        [0.0, 1.0, 0.0, 1.0],
    )]);
    renderer.render_frame();

    let frame = renderer.screenshot();
    // the disc's center: green
    assert_close(pixel(frame, 240, 120, 120), [0, srgb8(1.0), 0, 255], "disc center");
    // the bbox corner (NDC 0.7, 0.7 → length 0.99? no: (0.7,0.7) local ≈ 1.4
    // in unit-disc space — outside the disc): clear
    let corner = pixel(frame, 240, 204, 36);
    assert_eq!(corner, [0, 0, 0, 255]);
}

/// The full pipeline: an entity's MeshRenderer draws through the camera at
/// its entity's world transform — no manual mapping anywhere.
#[tokio::test]
async fn entity_meshrenderer_renders_through_camera() {
    let sink = hydrargyros::game::FrameSink::new();
    let mut renderer = pollster::block_on(WgpuRenderer::new_offscreen(480, 270))
        .expect("no wgpu adapter on this machine");
    renderer.attach_sink(sink.clone());

    let mut engine = Engine::new("acme", "pipeline-test");
    engine.set_physics(PhysicsWorld::none());
    engine.set_renderer(Box::new(renderer));

    let mut quad = Entity::new("marked");
    quad.transform_mut().position = Vec3::new(0.0, 4.0, 0.0); // view center
    quad.add_component(Box::new(MeshRenderer::rect(
        Vec3::new(1.0, 1.0, 1.0),
        [1.0, 0.0, 0.0, 1.0],
    )));
    engine.add_entity(quad);

    // one pumped frame: update → draw → render_frame → publish
    engine.start_with(Box::new(HeadlessWindow::for_frames(1)));

    let frame = sink.latest().expect("no frame published");
    let center = pixel(&frame.rgba, frame.width, frame.width / 2, frame.height / 2);
    assert_close(center, [srgb8(1.0), 0, 0, 255], "center");

    // the quad is 1 world half-unit tall on a 5-half-unit view → it spans
    // 1/5 of the canvas height: an edge just inside the boundary must be
    // red too (a scale bug shrinks or grows the quad from the center)
    let edge_y = 135 - (1.0 / 5.0 * 135.0) as u32 + 3;
    let edge = pixel(&frame.rgba, frame.width, frame.width / 2, edge_y);
    assert_close(edge, [srgb8(1.0), 0, 0, 255], "quad edge");
}

/// A child entity renders at its COMPOSED world transform — gold's
/// transform hierarchy reaches the pixels.
#[tokio::test]
async fn child_entities_render_at_world_transforms() {
    let sink = hydrargyros::game::FrameSink::new();
    let mut renderer = pollster::block_on(WgpuRenderer::new_offscreen(480, 270))
        .expect("no wgpu adapter on this machine");
    renderer.attach_sink(sink.clone());

    let mut engine = Engine::new("acme", "hierarchy-test");
    engine.set_physics(PhysicsWorld::none());
    engine.set_renderer(Box::new(renderer));
    // default camera: center (0, 4), half (8.89, 5)

    let mut parent = Entity::new("parent");
    parent.transform_mut().position = Vec3::new(0.0, 4.0, 0.0);
    let mut child = Entity::new("child");
    child.transform_mut().position = Vec3::new(4.0, 0.0, 0.0); // right of parent
    child.add_component(Box::new(MeshRenderer::circle(
        1.0,
        [0.0, 1.0, 0.0, 1.0],
    )));
    parent.add_child(child);
    engine.add_entity(parent);

    engine.start_with(Box::new(HeadlessWindow::for_frames(1)));

    let frame = sink.latest().expect("no frame");
    // child's world (4, 4) → clip (4/8.89, 0) → pixel (240 + 4/8.89*240, 135)
    // ≈ (348, 135): green disc center
    let px = (240.0 + 4.0 / 8.89 * 240.0) as u32;
    let center = pixel(&frame.rgba, frame.width, px, 135);
    assert_close(center, [0, srgb8(1.0), 0, 255], "child disc");
}

/// StreamingWindow events and renderables coexist: the window injects a
/// quit while the scene renders (the AI-debug loop, end to end).
#[tokio::test]
async fn stream_window_with_renderables() {
    use std::sync::Arc;

    let sink = hydrargyros::game::FrameSink::new();
    let mut renderer = pollster::block_on(WgpuRenderer::new_offscreen(320, 180))
        .expect("no wgpu adapter");
    renderer.attach_sink(sink.clone());

    let mut engine = Engine::new("acme", "stream-pipeline");
    engine.set_physics(PhysicsWorld::none());
    engine.set_renderer(Box::new(renderer));
    engine.add_entity(Entity::new("witness"));

    let window = Arc::new(StreamingWindow::new("127.0.0.1:0", sink, false).unwrap());
    // quit arrives through the window seam before the first frame ends
    window.inject(o!{ "type": "quit" }.into());
    engine.start_with(Box::new(window));

    // one pumped frame ran, then the quit stopped the pump
    assert!(!engine.running());
}
