use bevy::prelude::*;
use ragdoll_sandbox::{CorePlugin, GamePlugin};

fn main() {
    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "Ragdoll Sandbox".into(),
                ..default()
            }),
            ..default()
        }))
        .add_plugins((CorePlugin, GamePlugin))
        .run();
}
