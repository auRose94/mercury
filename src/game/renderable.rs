//! Renderables — gold's `renderable`/`meshRenderer`: components the
//! engine's draw pass turns into geometry. A `MeshRenderer` draws a solid
//! shape (rect or circle) at its entity's world transform; the camera maps
//! it to the screen.

use super::component::{Component, Priority};
use super::render::Quad;
use super::transform::Transform;
use crate::var::Vec3;

/// Something an entity owns that contributes geometry to the drawn scene.
/// The engine queries components for this trait during the draw pass.
pub trait Renderable {
    /// The geometry for this component, at the owning entity's world
    /// transform.
    fn quad(&self, world: &Transform) -> Quad;
}

/// What silhouette a [`MeshRenderer`] draws.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MeshShape {
    Rect,
    Circle,
}

/// A solid shape at the entity's transform (gold's `meshRenderer`, v0:
/// untextured). Draw tier — rendered after all updates. The entity's world
/// position is the shape's center; `half_size` is in world units.
#[derive(Debug, Clone)]
pub struct MeshRenderer {
    pub shape: MeshShape,
    pub half_size: Vec3,
    pub color: [f32; 4],
}

impl MeshRenderer {
    pub fn new(shape: MeshShape, half_size: Vec3, color: [f32; 4]) -> Self {
        MeshRenderer { shape, half_size, color }
    }

    /// A circular mesh.
    pub fn circle(radius: f32, color: [f32; 4]) -> Self {
        MeshRenderer {
            shape: MeshShape::Circle,
            half_size: Vec3::new(radius, radius, radius),
            color,
        }
    }

    /// A rectangular mesh.
    pub fn rect(half_size: Vec3, color: [f32; 4]) -> Self {
        MeshRenderer { shape: MeshShape::Rect, half_size, color }
    }
}

impl Component for MeshRenderer {
    fn priority(&self) -> Priority { Priority::Draw }

    fn as_any(&self) -> &dyn std::any::Any { self }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }

    fn as_renderable(&self) -> Option<&dyn Renderable> { Some(self) }
}

impl Renderable for MeshRenderer {
    fn quad(&self, world: &Transform) -> Quad {
        Quad {
            pos: [world.position.x, world.position.y],
            size: [self.half_size.x, self.half_size.y],
            color: self.color,
            shape: match self.shape {
                MeshShape::Rect => super::render::QuadShape::Rect,
                MeshShape::Circle => super::render::QuadShape::Circle,
            },
        }
    }
}
