//! Rendering: give every physical object a mesh, plus sky and lights.
//!
//! Physics code never touches meshes. Anything with a `Shape` and a `Paint`
//! gets a matching mesh here, automatically, the moment it is spawned.
//!
//! ## Smooth motion between physics steps
//!
//! Physics advances in fixed steps of 1/120 s (see `physics.rs`), but your
//! screen may refresh at 60, 144 or any other rate. Drawing objects exactly
//! where physics last put them would make motion stutter: some frames would
//! show two steps of movement, others none.
//!
//! So moving objects aren't drawn at their physics position directly. Each
//! one gets a separate *visual stand-in* entity that remembers the last two
//! physics positions and, every frame, draws itself part-way between them,
//! according to how far the clock has got toward the next physics step.
//! This lags the physics by up to one step (8 ms), which you can't see.

use bevy::prelude::*;
use bevy_rapier3d::control::KinematicCharacterController;
use bevy_rapier3d::prelude::RigidBody;

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
            .add_systems(FixedPostUpdate, record_physics_poses)
            .add_systems(
                PostUpdate,
                (attach_meshes, remove_orphaned_stand_ins, draw_between_steps)
                    .chain()
                    .in_set(SmoothingSet)
                    .before(TransformSystems::Propagate),
            );
    }
}

/// Systems that place the visual stand-ins. The camera follows the player's
/// stand-in, so it runs after this.
#[derive(SystemSet, Clone, PartialEq, Eq, Hash, Debug)]
pub struct SmoothingSet;

/// A visual stand-in for a moving physics object.
#[derive(Component)]
pub struct VisualStandIn {
    /// The physics entity this draws.
    pub target: Entity,
    /// Its pose after the second-to-last and the last physics step.
    previous: Option<Transform>,
    latest: Option<Transform>,
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
    added: Query<(Entity, &Shape, &Paint, &Transform, Option<&RigidBody>, Has<KinematicCharacterController>), Added<Shape>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    for (entity, shape, paint, transform, body, is_character) in &added {
        let visuals = (
            Mesh3d(meshes.add(shape.mesh())),
            MeshMaterial3d(materials.add(StandardMaterial {
                base_color: paint.0,
                perceptual_roughness: 0.8,
                ..default()
            })),
        );
        let moves = is_character || matches!(body, Some(RigidBody::Dynamic));
        if moves {
            commands.spawn((visuals, *transform, VisualStandIn { target: entity, previous: None, latest: None }));
        } else {
            // Things that never move (or aren't physical): draw them directly.
            commands.entity(entity).insert(visuals);
        }
    }
}

/// After every physics step: remember where each moving object ended up.
fn record_physics_poses(mut stand_ins: Query<&mut VisualStandIn>, targets: Query<&Transform, Without<VisualStandIn>>) {
    for mut stand_in in &mut stand_ins {
        if let Ok(pose) = targets.get(stand_in.target) {
            stand_in.previous = stand_in.latest.or(Some(*pose));
            stand_in.latest = Some(*pose);
        }
    }
}

/// Every frame: draw each object part-way between its last two physics
/// poses. `overstep_fraction` is how far (0 to 1) the clock has moved past the
/// last physics step toward the next one.
fn draw_between_steps(
    fixed_time: Res<Time<Fixed>>,
    mut stand_ins: Query<(&VisualStandIn, &mut Transform)>,
    targets: Query<&Transform, Without<VisualStandIn>>,
) {
    let t = fixed_time.overstep_fraction();
    for (stand_in, mut transform) in &mut stand_ins {
        match (stand_in.previous, stand_in.latest) {
            (Some(a), Some(b)) => {
                transform.translation = a.translation.lerp(b.translation, t);
                transform.rotation = a.rotation.slerp(b.rotation, t);
            }
            // No physics step yet since it appeared: draw it where it is.
            _ => {
                if let Ok(pose) = targets.get(stand_in.target) {
                    *transform = *pose;
                }
            }
        }
    }
}

/// When a physics object is removed (e.g. the person respawns), remove its
/// stand-in too.
fn remove_orphaned_stand_ins(mut commands: Commands, stand_ins: Query<(Entity, &VisualStandIn)>, targets: Query<()>) {
    for (entity, stand_in) in &stand_ins {
        if targets.get(stand_in.target).is_err() {
            commands.entity(entity).despawn();
        }
    }
}
