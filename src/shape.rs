//! A tiny shape description shared by physics and rendering.
//!
//! Every physical object in the sandbox is one of these simple shapes. The same
//! description produces both the *collider* (what the physics engine sees) and
//! the *mesh* (what you see), so they can never disagree.

use bevy::prelude::*;
use bevy_rapier3d::prelude::*;

/// Sizes are "half" sizes (centre to edge), which is what Rapier expects.
#[derive(Component, Clone, Copy, Debug)]
pub enum Shape {
    /// A box. `Vec3` holds the half-extents along x, y and z.
    Cuboid(Vec3),
    /// A pill shape lying along the x axis (left-right).
    CapsuleX { half_length: f32, radius: f32 },
    /// A pill shape standing along the y axis (up-down).
    CapsuleY { half_length: f32, radius: f32 },
    /// A sphere.
    Ball(f32),
}

impl Shape {
    pub fn collider(&self) -> Collider {
        match *self {
            Shape::Cuboid(h) => Collider::cuboid(h.x, h.y, h.z),
            Shape::CapsuleX { half_length, radius } => Collider::capsule_x(half_length, radius),
            Shape::CapsuleY { half_length, radius } => Collider::capsule_y(half_length, radius),
            Shape::Ball(r) => Collider::ball(r),
        }
    }

    pub fn mesh(&self) -> Mesh {
        match *self {
            Shape::Cuboid(h) => Cuboid::new(h.x * 2.0, h.y * 2.0, h.z * 2.0).into(),
            // Bevy's capsule stands along y; rotate the vertices to lie along x.
            Shape::CapsuleX { half_length, radius } => Mesh::from(Capsule3d::new(radius, half_length * 2.0))
                .rotated_by(Quat::from_rotation_z(std::f32::consts::FRAC_PI_2)),
            Shape::CapsuleY { half_length, radius } => Capsule3d::new(radius, half_length * 2.0).into(),
            Shape::Ball(r) => Sphere::new(r).into(),
        }
    }
}

/// The colour an object should be drawn with. Only used when there is a window.
#[derive(Component, Clone, Copy)]
pub struct Paint(pub Color);
