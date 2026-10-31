//! Physics tests (behind the `physics` feature): the rapier world sim —
//! gravity, falling bodies, raycasts — plus the none-backend contract.

#![cfg(feature = "physics")]

use mercury::game::physics::{BodyType, PhysicsWorld, Shape};
use mercury::var::{Quat, Vec3};
use mercury::Var;

#[test]
fn gravity_pulls_dynamic_bodies_down() {
    let mut world = PhysicsWorld::create("rapier", &Var::Null).unwrap();
    assert_eq!(world.backend_name(), "rapier");
    world.add_body("ball", &Shape::Sphere(0.5), BodyType::Dynamic, Vec3::new(0.0, 10.0, 0.0), Quat { x: 0.0, y: 0.0, z: 0.0, w: 1.0 }, 1.0);

    let dt = 1.0 / 60.0;
    let start_y = world.body_pose("ball").unwrap().0.y;
    for _ in 0..30 {
        world.step(dt);
    }
    let end_y = world.body_pose("ball").unwrap().0.y;
    assert!(end_y < start_y - 1.0, "body should have fallen: {start_y} → {end_y}");
}

#[test]
fn static_bodies_hold_the_line() {
    let mut world = PhysicsWorld::create("rapier", &Var::Null).unwrap();
    world.add_body("floor", &Shape::Box(Vec3::new(10.0, 0.5, 10.0)), BodyType::Static, Vec3::new(0.0, 0.0, 0.0), Quat { x: 0.0, y: 0.0, z: 0.0, w: 1.0 }, 0.0);
    let id = Quat { x: 0.0, y: 0.0, z: 0.0, w: 1.0 };
    world.add_body("ball", &Shape::Sphere(0.5), BodyType::Dynamic, Vec3::new(0.0, 5.0, 0.0), id, 1.0);

    let dt = 1.0 / 60.0;
    for _ in 0..240 {
        world.step(dt);
    }
    let (pos, _) = world.body_pose("ball").unwrap();
    // the ball came to rest on the floor (sphere radius 0.5, floor top 0.5)
    assert!(pos.y < 3.0, "should have fallen far: {pos:?}");
    assert!(pos.y > 0.5, "should rest on the floor, not through it: {pos:?}");
}

#[test]
fn raycast_names_the_hit_body() {
    let mut world = PhysicsWorld::create("rapier", &Var::Null).unwrap();
    world.add_body("target", &Shape::Sphere(1.0), BodyType::Static, Vec3::new(0.0, 0.0, -10.0), Quat { x: 0.0, y: 0.0, z: 0.0, w: 1.0 }, 0.0);

    // straight at the sphere's center
    let hit = world
        .raycast(Vec3::new(0.0, 0.0, 0.0), Vec3::new(0.0, 0.0, -1.0), 100.0)
        .expect("ray should hit the sphere");
    assert_eq!(hit.body, "target");
    assert!((hit.distance - 9.0).abs() < 1e-3, "hit at the sphere's near surface: {}", hit.distance);
    assert!((hit.point.z - (-9.0)).abs() < 1e-3);

    // and away from it: no hit
    assert!(world.raycast(Vec3::new(0.0, 0.0, 0.0), Vec3::new(0.0, 0.0, 1.0), 100.0).is_none());
}

#[test]
fn re_registering_a_name_replaces_the_body() {
    let mut world = PhysicsWorld::create("rapier", &Var::Null).unwrap();
    let id = Quat { x: 0.0, y: 0.0, z: 0.0, w: 1.0 };
    world.add_body("b", &Shape::Sphere(0.5), BodyType::Dynamic, Vec3::new(0.0, 10.0, 0.0), id, 1.0);
    world.step(1.0 / 60.0);
    // replace: back at the fresh position
    world.add_body("b", &Shape::Sphere(0.5), BodyType::Dynamic, Vec3::new(0.0, 10.0, 0.0), id, 1.0);
    let (pos, rot) = world.body_pose("b").unwrap();
    assert_eq!(pos, Vec3::new(0.0, 10.0, 0.0));
    assert_eq!(rot, Quat { x: 0.0, y: 0.0, z: 0.0, w: 1.0 });
}

// ---- the none backend: gold's contract ----

#[test]
fn none_backend_initializes_and_no_ops() {
    let mut world = PhysicsWorld::none();
    assert_eq!(world.backend_name(), "none");
    world.set_gravity(Vec3::new(0.0, -9.81, 0.0));
    world.add_body("ghost", &Shape::Sphere(0.5), BodyType::Dynamic, Vec3::new(1.0, 2.0, 3.0), Quat { x: 0.0, y: 0.0, z: 0.0, w: 1.0 }, 1.0);
    world.step(1.0 / 60.0);
    // no rigid state: body_pose is None, so transforms stay under script control
    assert!(world.body_pose("ghost").is_none());
    assert!(world.raycast(Vec3::new(0.0, 0.0, 0.0), Vec3::new(1.0, 0.0, 0.0), 10.0).is_none());
}

#[test]
fn unknown_backend_is_refused() {
    assert!(PhysicsWorld::create("dreamcast", &Var::Null).is_err());
}
