//! Global physics settings: a fixed time step, solver quality, contact
//! stiffness, and the slow-motion toggle.
//!
//! ## Why a *fixed* time step
//!
//! The simulation advances in small time steps. Many physics details (how
//! stiff contacts are, how muscles respond) depend on the step size. If the
//! step followed your monitor's frame rate, a 60 Hz and a 144 Hz monitor would
//! simulate a *different person*: one might stand while the other topples.
//!
//! So physics always advances in steps of exactly `PHYSICS_DT`, each split
//! into `SUBSTEPS` smaller steps, however fast the screen refreshes. It runs
//! in Bevy's `FixedUpdate` schedule, which Bevy runs as many times per frame
//! as needed to keep up with the clock (0, 1 or several). The muscles and the
//! balance controller run in the same schedule, right before each physics
//! step, so they react to every single step whatever the frame rate.
//!
//! Rendering then *interpolates* (blends) between the last two physics states
//! so motion looks smooth at any frame rate (`visuals.rs`). The headless tests
//! use exactly these values too, so they test the same physics you see.

use std::time::Duration;

use bevy::prelude::*;
use bevy_rapier3d::prelude::*;

/// Physics steps per second.
pub const PHYSICS_HZ: f32 = 120.0;
pub const PHYSICS_DT: f32 = 1.0 / PHYSICS_HZ;

/// One physics step as a `Duration` (what Bevy's clocks use).
pub fn physics_step() -> Duration {
    Duration::from_secs_f64(1.0 / PHYSICS_HZ as f64)
}

/// Each physics step is split into this many substeps, so the solver works in
/// 1/480 s slices. The body is a chain of 14 parts held together by joints and
/// standing on two small feet; we found that larger slices let it slowly tip
/// over, even with strong muscles (see `PROGRESS.md`, Stage 2).
pub const SUBSTEPS: usize = 4;

/// Simulated seconds per real second while slow motion is on.
pub const SLOW_MOTION_SCALE: f32 = 0.2;

/// Whether slow motion is on. Toggled by the player (see `player.rs`).
#[derive(Resource, Default)]
pub struct SlowMotion(pub bool);

/// Rapier's time-step setting: every run of the fixed schedule advances
/// physics by exactly one step.
pub fn game_timestep() -> TimestepMode {
    TimestepMode::Fixed { dt: PHYSICS_DT, substeps: SUBSTEPS }
}

pub struct PhysicsSettingsPlugin;

impl Plugin for PhysicsSettingsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SlowMotion>()
            .insert_resource(game_timestep())
            .insert_resource(Time::<Fixed>::from_duration(physics_step()))
            .add_systems(Startup, cap_frame_time)
            .add_systems(Update, (configure_solver, apply_slow_motion));
    }
}

/// If a frame takes very long (window dragged, PC busy), don't try to catch up
/// on all the missed physics at once: that can snowball into an ever-slower
/// game. Instead, the game briefly runs in slow motion. (At most 100 ms of
/// game time per frame = at most 12 physics steps.)
fn cap_frame_time(mut time: ResMut<Time<Virtual>>) {
    time.set_max_delta(Duration::from_millis(100));
}

/// The solver resolves all contacts and joints by repeatedly nudging bodies
/// until they (almost) agree. More iterations = stiffer joints, less jitter.
///
/// Contacts are slightly "soft": shapes may sink into each other a tiny bit and
/// get pushed back out like a stiff spring. With 80 kg on two small feet, the
/// default softness lets the heels sink just enough to tip the body over, so we
/// make contacts stiffer.
fn configure_solver(mut sims: Query<&mut RapierContextSimulation, Added<RapierContextSimulation>>) {
    for mut sim in &mut sims {
        let params = &mut sim.integration_parameters;
        params.num_solver_iterations = 8;
        params.contact_softness.natural_frequency = 120.0;
        params.contact_softness.damping_ratio = 10.0;
        params.static_contact_softness.natural_frequency = 120.0;
        params.static_contact_softness.damping_ratio = 10.0;
        // Let friction act during the solver's position-correction pass too.
        // Without it, the slight forward lean of a standing person made the
        // feet creep across the floor at ~3 mm/s, like a slow moonwalk.
        params.friction_in_bias_pass = true;
    }
}

/// Slow motion slows down the whole game clock. Physics keeps its exact step
/// size and just takes fewer steps per real second.
fn apply_slow_motion(slow: Res<SlowMotion>, mut time: ResMut<Time<Virtual>>) {
    if slow.is_changed() {
        time.set_relative_speed(if slow.0 { SLOW_MOTION_SCALE } else { 1.0 });
    }
}
