//! The player: a third-person character you walk around with, and the camera
//! that follows them.
//!
//! The player is *not* a ragdoll. It is a "kinematic character controller":
//! each frame we tell Rapier where we'd like to move and Rapier slides us
//! along walls, up steps and over slopes. Kinematic means forces don't push it
//! around; it goes where we say unless something solid is in the way.

use bevy::input::mouse::AccumulatedMouseMotion;
use bevy::prelude::*;
use bevy::window::{CursorGrabMode, CursorOptions};
use bevy_rapier3d::control::{KinematicCharacterController, KinematicCharacterControllerOutput};
use bevy_rapier3d::prelude::*;

use crate::layers;
use crate::physics::SlowMotion;
use crate::ragdoll::{RagdollPart, SpawnRagdoll};
use crate::shape::{Paint, Shape};

const WALK_SPEED: f32 = 4.0;
const SPRINT_SPEED: f32 = 8.0;
const JUMP_SPEED: f32 = 5.0;
const GRAVITY: f32 = 9.81;
const MOUSE_SENSITIVITY: f32 = 0.0025;

/// How hard running into the person shoves them: impulse (N·s) per m/s of speed.
const BARGE_STRENGTH: f32 = 9.0;
/// Seconds between shoves, so walking into someone isn't one shove per frame.
const BARGE_COOLDOWN: f32 = 0.35;

/// Camera placement relative to the player: distance behind, and how far
/// right of centre (an over-the-shoulder view, like GTA).
const CAMERA_DISTANCE: f32 = 4.5;
const CAMERA_SHOULDER: f32 = 0.9;
const CAMERA_HEIGHT: f32 = 0.9;

#[derive(Component, Default)]
pub struct Player {
    /// Current velocity. We track it ourselves since kinematic bodies have none.
    pub velocity: Vec3,
    barge_cooldown: f32,
}

/// The camera. Yaw = looking left/right, pitch = looking up/down (radians).
#[derive(Component, Default)]
pub struct PlayerCamera {
    pub yaw: f32,
    pub pitch: f32,
}

/// True while the mouse is captured by the game window.
#[derive(Resource, Default)]
pub struct MouseCaptured(pub bool);

pub struct PlayerPlugin;

impl Plugin for PlayerPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<MouseCaptured>()
            .add_systems(Startup, spawn_player)
            .add_systems(
                Update,
                (capture_mouse, look, move_player, follow_camera, barge, sandbox_keys).chain(),
            );
    }
}

/// The ray from the camera through the crosshair (the centre of the screen).
pub fn aim_ray(camera: &GlobalTransform) -> (Vec3, Vec3) {
    (camera.translation(), camera.forward().as_vec3())
}

/// Only these layers block the aim ray: the player never shoots themselves.
pub fn aim_filter() -> QueryFilter<'static> {
    QueryFilter::new().groups(CollisionGroups::new(
        Group::ALL,
        layers::WORLD.union(layers::RAGDOLL).union(layers::PROP),
    ))
}

fn spawn_player(mut commands: Commands) {
    let body = Shape::CapsuleY { half_length: 0.5, radius: 0.3 };
    commands.spawn((
        Name::new("Player"),
        Player::default(),
        body,
        Paint(Color::srgb(0.75, 0.30, 0.25)),
        Transform::from_xyz(0.0, 0.9, -5.0),
        body.collider(),
        layers::player(),
        KinematicCharacterController {
            // Only collide with things in the player's layer filter.
            filter_groups: Some(layers::player()),
            autostep: Some(CharacterAutostep {
                max_height: CharacterLength::Absolute(0.35),
                min_width: CharacterLength::Absolute(0.2),
                include_dynamic_bodies: false,
            }),
            snap_to_ground: Some(CharacterLength::Absolute(0.3)),
            // We shove the person ourselves (see `barge`), more controllably.
            apply_impulse_to_dynamic_bodies: false,
            ..default()
        },
    ));
    // Start looking toward +Z, where the person stands.
    let camera = PlayerCamera { yaw: std::f32::consts::PI, pitch: -0.15 };
    commands.spawn((Camera3d::default(), camera, Transform::default()));
}

/// Click in the window to capture the mouse; Escape releases it.
fn capture_mouse(
    mut cursor: Single<&mut CursorOptions>,
    mouse: Res<ButtonInput<MouseButton>>,
    keys: Res<ButtonInput<KeyCode>>,
    mut captured: ResMut<MouseCaptured>,
) {
    if !captured.0 && mouse.just_pressed(MouseButton::Left) {
        captured.0 = true;
    } else if captured.0 && keys.just_pressed(KeyCode::Escape) {
        captured.0 = false;
    } else {
        return;
    }
    cursor.grab_mode = if captured.0 { CursorGrabMode::Locked } else { CursorGrabMode::None };
    cursor.visible = !captured.0;
}

fn look(motion: Res<AccumulatedMouseMotion>, captured: Res<MouseCaptured>, mut camera: Single<&mut PlayerCamera>) {
    if !captured.0 {
        return;
    }
    camera.yaw -= motion.delta.x * MOUSE_SENSITIVITY;
    camera.pitch = (camera.pitch - motion.delta.y * MOUSE_SENSITIVITY).clamp(-1.4, 1.2);
}

fn move_player(
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    camera: Single<&PlayerCamera>,
    player: Single<(
        &mut Player,
        &mut Transform,
        &mut KinematicCharacterController,
        Option<&KinematicCharacterControllerOutput>,
    )>,
) {
    let (mut player, mut transform, mut controller, output) = player.into_inner();
    let dt = time.delta_secs();
    let grounded = output.map(|o| o.grounded).unwrap_or(false);

    // WASD relative to where the camera looks, flattened onto the ground.
    let facing = Quat::from_rotation_y(camera.yaw);
    let forward = facing * Vec3::NEG_Z;
    let right = facing * Vec3::X;
    let mut wish = Vec3::ZERO;
    if keys.pressed(KeyCode::KeyW) { wish += forward; }
    if keys.pressed(KeyCode::KeyS) { wish -= forward; }
    if keys.pressed(KeyCode::KeyD) { wish += right; }
    if keys.pressed(KeyCode::KeyA) { wish -= right; }
    let speed = if keys.pressed(KeyCode::ShiftLeft) { SPRINT_SPEED } else { WALK_SPEED };
    let horizontal = wish.normalize_or_zero() * speed;

    // Gravity and jumping act on the vertical speed only.
    let mut vertical = player.velocity.y;
    if grounded {
        vertical = if keys.just_pressed(KeyCode::Space) { JUMP_SPEED } else { 0.0 };
    } else {
        vertical -= GRAVITY * dt;
    }

    player.velocity = Vec3::new(horizontal.x, vertical, horizontal.z);
    controller.translation = Some(player.velocity * dt);

    // Turn the body toward the direction of travel.
    if horizontal.length_squared() > 0.01 {
        let target = Quat::from_rotation_y(f32::atan2(-horizontal.x, -horizontal.z));
        transform.rotation = transform.rotation.slerp(target, (dt * 12.0).min(1.0));
    }
}

fn follow_camera(
    player: Single<&Transform, (With<Player>, Without<PlayerCamera>)>,
    camera: Single<(&PlayerCamera, &mut Transform)>,
    rapier: ReadRapierContext,
) {
    let (cam, mut cam_transform) = camera.into_inner();
    let rotation = Quat::from_euler(EulerRot::YXZ, cam.yaw, cam.pitch, 0.0);
    let pivot = player.translation + Vec3::Y * CAMERA_HEIGHT;
    let wanted = pivot + rotation * Vec3::new(CAMERA_SHOULDER, 0.0, CAMERA_DISTANCE);

    // Don't let walls get between the camera and the player: if one does,
    // pull the camera in front of it.
    let mut position = wanted;
    if let Ok(context) = rapier.single() {
        let offset = wanted - pivot;
        let only_world = QueryFilter::new().groups(CollisionGroups::new(Group::ALL, layers::WORLD));
        let hit = context.with_query_pipeline(only_world, |q| q.cast_ray(pivot, offset.normalize(), offset.length(), true));
        if let Some((_, toi)) = hit {
            position = pivot + offset.normalize() * (toi - 0.2).max(0.3);
        }
    }
    *cam_transform = Transform::from_translation(position).with_rotation(rotation);
}

/// Running into the person gives them a shove proportional to your speed.
fn barge(
    time: Res<Time>,
    player: Single<(&mut Player, Option<&KinematicCharacterControllerOutput>)>,
    mut parts: Query<&mut ExternalImpulse, With<RagdollPart>>,
) {
    let (mut player, output) = player.into_inner();
    player.barge_cooldown -= time.delta_secs();
    let Some(output) = output else { return };
    if player.barge_cooldown > 0.0 {
        return;
    }
    let horizontal = Vec3::new(player.velocity.x, 0.0, player.velocity.z);
    if horizontal.length() < 0.5 {
        return;
    }
    for collision in &output.collisions {
        if let Ok(mut impulse) = parts.get_mut(collision.entity) {
            // Mostly along your direction of travel, slightly upward.
            impulse.impulse += (horizontal + Vec3::Y * 0.3 * horizontal.length()) * BARGE_STRENGTH;
            player.barge_cooldown = BARGE_COOLDOWN;
        }
    }
}

/// R = reset the person at the start, F = drop a new person in front of you,
/// T = toggle slow motion.
fn sandbox_keys(
    keys: Res<ButtonInput<KeyCode>>,
    player: Single<&Transform, With<Player>>,
    camera: Single<&PlayerCamera>,
    mut spawn: MessageWriter<SpawnRagdoll>,
    mut slow: ResMut<SlowMotion>,
) {
    if keys.just_pressed(KeyCode::KeyR) {
        spawn.write(SpawnRagdoll::START);
    }
    if keys.just_pressed(KeyCode::KeyF) {
        // 3 m ahead in the direction the camera looks, facing back toward you.
        let forward = Quat::from_rotation_y(camera.yaw) * Vec3::NEG_Z;
        let feet = player.translation + forward * 3.0 - Vec3::Y * 0.8;
        let yaw = f32::atan2(-forward.x, -forward.z);
        spawn.write(SpawnRagdoll { position: feet, yaw });
    }
    if keys.just_pressed(KeyCode::KeyT) {
        slow.0 = !slow.0;
    }
}
