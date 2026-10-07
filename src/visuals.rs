//! Rendering: give every physical object a mesh, plus sky and lights.
//!
//! Physics code never touches meshes. Anything with a `Shape` and a `Paint`
//! gets a matching mesh here, automatically, the moment it is spawned.

use bevy::prelude::*;

use crate::shape::{Paint, Shape};

pub struct VisualsPlugin;

impl Plugin for VisualsPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(ClearColor(Color::srgb(0.62, 0.75, 0.88)))
            .insert_resource(GlobalAmbientLight {
                color: Color::WHITE,
                brightness: 600.0,
                ..default()
            })
            .add_systems(Startup, spawn_sun)
            .add_systems(PostUpdate, attach_meshes);
    }
}

fn spawn_sun(mut commands: Commands) {
    commands.spawn((
        DirectionalLight {
            illuminance: 9000.0,
            shadow_maps_enabled: true,
            ..default()
        },
        Transform::from_xyz(4.0, 10.0, 6.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));
}

fn attach_meshes(
    mut commands: Commands,
    added: Query<(Entity, &Shape, &Paint), Added<Shape>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    for (entity, shape, paint) in &added {
        commands.entity(entity).insert((
            Mesh3d(meshes.add(shape.mesh())),
            MeshMaterial3d(materials.add(StandardMaterial {
                base_color: paint.0,
                perceptual_roughness: 0.8,
                ..default()
            })),
        ));
    }
}
