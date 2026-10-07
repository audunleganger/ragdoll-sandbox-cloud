//! Global physics settings: solver quality and the slow-motion toggle.

use bevy::prelude::*;
use bevy_rapier3d::prelude::*;

/// Simulated seconds per real second while slow motion is on.
pub const SLOW_MOTION_SCALE: f32 = 0.2;

/// Each frame's physics step is split into this many smaller steps. A body is
/// a chain of 14 parts held together by joints, and joint chains stay much
/// tighter with small steps than with one big one.
pub const SUBSTEPS: usize = 4;

/// Whether slow motion is on. Toggled by the player (see `player.rs`).
#[derive(Resource, Default)]
pub struct SlowMotion(pub bool);

pub struct PhysicsSettingsPlugin;

impl Plugin for PhysicsSettingsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SlowMotion>()
            // "Variable" = advance physics by however long the last frame took
            // (capped so a lag spike can't make the simulation explode).
            .insert_resource(TimestepMode::Variable {
                max_dt: 1.0 / 60.0,
                time_scale: 1.0,
                substeps: SUBSTEPS,
            })
            .add_systems(Update, (configure_solver, apply_slow_motion));
    }
}

/// The solver resolves all contacts and joints by repeatedly nudging bodies
/// until they (almost) agree. More iterations = stiffer joints, less jitter.
fn configure_solver(mut sims: Query<&mut RapierContextSimulation, Added<RapierContextSimulation>>) {
    for mut sim in &mut sims {
        sim.integration_parameters.num_solver_iterations = 8;
    }
}

fn apply_slow_motion(slow: Res<SlowMotion>, mut mode: ResMut<TimestepMode>) {
    if !slow.is_changed() {
        return;
    }
    if let TimestepMode::Variable { time_scale, .. } = mode.as_mut() {
        *time_scale = if slow.0 { SLOW_MOTION_SCALE } else { 1.0 };
    }
}
