//! Headless checks of the player character (movement and jumping), running
//! the real player code with simulated key presses.

use bevy::input::InputPlugin;
use bevy::prelude::*;
use bevy::time::TimeUpdateStrategy;
use ragdoll_sandbox::CorePlugin;
use ragdoll_sandbox::player::{Player, PlayerPlugin};

fn app() -> App {
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, TransformPlugin, InputPlugin, CorePlugin, PlayerPlugin));
    app.insert_resource(TimeUpdateStrategy::ManualDuration(ragdoll_sandbox::physics::physics_step()));
    app.finish();
    app.cleanup();
    app
}

fn player_y(app: &mut App) -> f32 {
    let mut q = app.world_mut().query_filtered::<&Transform, With<Player>>();
    q.single(app.world()).unwrap().translation.y
}

/// Press or release a key the way a real window does: by sending a keyboard
/// event. (Setting the key state directly doesn't work for "just pressed":
/// Bevy clears that at the start of each frame, before game code runs.)
fn key(app: &mut App, code: KeyCode, pressed: bool) {
    use bevy::input::ButtonState;
    use bevy::input::keyboard::{Key, KeyboardInput};
    app.world_mut().write_message(KeyboardInput {
        key_code: code,
        logical_key: Key::Unidentified(bevy::input::keyboard::NativeKey::Unidentified),
        state: if pressed { ButtonState::Pressed } else { ButtonState::Released },
        text: None,
        repeat: false,
        window: Entity::PLACEHOLDER,
    });
}

fn steps(app: &mut App, n: usize) {
    for _ in 0..n {
        app.update();
    }
}

#[test]
fn space_makes_the_player_jump() {
    let mut app = app();
    steps(&mut app, 120); // land on the ground
    let ground = player_y(&mut app);
    key(&mut app, KeyCode::Space, true);
    app.update();
    key(&mut app, KeyCode::Space, false);
    let mut highest = ground;
    for _ in 0..180 {
        app.update();
        highest = highest.max(player_y(&mut app));
    }
    // Jump speed 5 m/s with gravity 9.81: peak ≈ 5² / (2 × 9.81) ≈ 1.27 m.
    assert!(highest - ground > 0.8, "jumped only {:.2} m", highest - ground);
    assert!((player_y(&mut app) - ground).abs() < 0.05, "didn't land again");
}

#[test]
fn w_walks_the_player_forward() {
    let mut app = app();
    steps(&mut app, 60);
    let start = {
        let mut q = app.world_mut().query_filtered::<&Transform, With<Player>>();
        q.single(app.world()).unwrap().translation
    };
    key(&mut app, KeyCode::KeyW, true);
    steps(&mut app, 60); // half a second
    let end = {
        let mut q = app.world_mut().query_filtered::<&Transform, With<Player>>();
        q.single(app.world()).unwrap().translation
    };
    let moved = (end - start) * Vec3::new(1.0, 0.0, 1.0);
    // Walking speed 4 m/s for 0.5 s, toward the person (+Z: the camera starts facing them).
    assert!((moved.length() - 2.0).abs() < 0.3, "walked {:.2} m in 0.5 s", moved.length());
    assert!(moved.z > 1.5, "walked the wrong way: {moved:?}");
}
