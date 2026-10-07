//! Ragdoll sandbox library.
//!
//! The project is split into a *core* that only needs physics (usable in
//! headless tests, without a window) and the *game* parts that need a window,
//! keyboard and mouse. `main.rs` puts them together.

pub mod balance;
pub mod hud;
pub mod layers;
pub mod muscles;
pub mod physics;
pub mod player;
pub mod ragdoll;
pub mod shape;
pub mod throwing;
pub mod visuals;
pub mod weapons;
pub mod world;

use bevy::prelude::*;

/// Everything that works without a window: physics settings, the map and the person.
pub struct CorePlugin;

impl Plugin for CorePlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            // Physics runs in Bevy's fixed-rate schedule (see `physics.rs`).
            bevy_rapier3d::prelude::RapierPhysicsPlugin::<bevy_rapier3d::prelude::NoUserData>::default().in_fixed_schedule(),
            physics::PhysicsSettingsPlugin,
            world::WorldPlugin,
            ragdoll::RagdollPlugin,
            muscles::MusclesPlugin,
            balance::BalancePlugin,
        ));
    }
}

/// Everything you interact with: player, camera, weapons, throwing, HUD, rendering.
pub struct GamePlugin;

impl Plugin for GamePlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            visuals::VisualsPlugin,
            player::PlayerPlugin,
            weapons::WeaponsPlugin,
            throwing::ThrowingPlugin,
            hud::HudPlugin,
        ));
    }
}
