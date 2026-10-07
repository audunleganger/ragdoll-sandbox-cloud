//! The test map: flat ground, a raised platform with stairs up to it, a ramp,
//! and a waist-high wall to tip people over.
//!
//! Everything here is a "fixed" body: it takes part in collisions but never
//! moves, as if it had infinite mass.

use bevy::prelude::*;
use bevy_rapier3d::prelude::*;

use crate::layers;
use crate::shape::{Paint, Shape};

pub struct WorldPlugin;

impl Plugin for WorldPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_world);
    }
}

fn solid(commands: &mut Commands, name: &str, half_size: Vec3, transform: Transform, color: Color) {
    let shape = Shape::Cuboid(half_size);
    commands.spawn((
        Name::new(name.to_string()),
        shape,
        Paint(color),
        transform,
        RigidBody::Fixed,
        shape.collider(),
        layers::world(),
        Friction::coefficient(0.9),
    ));
}

fn spawn_world(mut commands: Commands) {
    let ground = Color::srgb(0.45, 0.50, 0.42);
    let concrete = Color::srgb(0.62, 0.60, 0.57);
    let accent = Color::srgb(0.70, 0.52, 0.35);

    // Ground: a thick slab whose top surface is at y = 0.
    solid(&mut commands, "Ground", Vec3::new(40.0, 0.5, 40.0), Transform::from_xyz(0.0, -0.5, 0.0), ground);

    // Raised platform, 3 m tall, to push people off.
    let platform_center = Vec3::new(13.0, 1.5, -8.0);
    solid(&mut commands, "Platform", Vec3::new(3.0, 1.5, 4.0), Transform::from_translation(platform_center), concrete);

    // Stairs climbing toward the platform along +X: 10 steps of 30 cm each.
    let steps = 10;
    let step_height = 0.3;
    let step_depth = 0.6;
    let stairs_end_x = platform_center.x - 3.0;
    for i in 0..steps {
        let height = step_height * (i + 1) as f32;
        let x = stairs_end_x - step_depth * (steps - i) as f32 + step_depth / 2.0;
        solid(
            &mut commands,
            "Step",
            Vec3::new(step_depth / 2.0, height / 2.0, 1.2),
            Transform::from_xyz(x, height / 2.0, platform_center.z),
            accent,
        );
    }

    // Ramp: an 8 m long slope rising 2 m (about 14 degrees).
    let length: f32 = 8.0;
    let rise: f32 = 2.0;
    let angle = (rise / length).atan();
    solid(
        &mut commands,
        "Ramp",
        Vec3::new(length / 2.0 / angle.cos(), 0.1, 2.0),
        Transform::from_xyz(-10.0, rise / 2.0, -8.0).with_rotation(Quat::from_rotation_z(angle)),
        concrete,
    );
    // Block at the top of the ramp so it has a ledge to fall off.
    solid(&mut commands, "Ramp top", Vec3::new(1.5, 1.0, 2.0), Transform::from_xyz(-4.5, 1.0, -8.0), concrete);

    // Waist-high wall to flip people over.
    solid(&mut commands, "Low wall", Vec3::new(2.0, 0.45, 0.15), Transform::from_xyz(0.0, 0.45, 4.0), accent);
}
