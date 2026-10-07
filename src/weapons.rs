//! Guns and explosions.
//!
//! Shooting is "hitscan": no bullet object flies through the air. We cast an
//! invisible ray from the camera through the crosshair, find the first thing it
//! hits, and give that body an *impulse* at the exact hit point.
//!
//! An impulse is an instant kick: it changes velocity by `impulse / mass`.
//! Because it's applied at a point rather than the centre, it also makes the
//! body spin. A shot to the shoulder twists the torso; a shot to the shin
//! knocks the leg out.

use bevy::prelude::*;
use bevy_rapier3d::prelude::*;

use crate::player::{MouseCaptured, PlayerCamera, aim_filter, aim_ray};
use crate::ragdoll::RagdollPart;
use crate::shape::{Paint, Shape};
use crate::throwing::Prop;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Weapon {
    Pistol,
    Shotgun,
}

#[derive(Resource)]
pub struct Arsenal {
    pub current: Weapon,
    cooldown: f32,
}

struct WeaponStats {
    /// Impulse per ray, in N·s. Real bullets carry only ~5 N·s; games
    /// exaggerate so hits are visible, and so do we.
    impulse: f32,
    /// Rays per shot.
    pellets: usize,
    /// Random spread of each ray, in radians.
    spread: f32,
    /// Seconds between shots.
    cooldown: f32,
}

fn stats(weapon: Weapon) -> WeaponStats {
    match weapon {
        Weapon::Pistol => WeaponStats { impulse: 40.0, pellets: 1, spread: 0.0, cooldown: 0.2 },
        Weapon::Shotgun => WeaponStats { impulse: 14.0, pellets: 9, spread: 0.06, cooldown: 0.8 },
    }
}

/// Explosion radius (m) and the speed change (m/s) it gives at its centre.
const EXPLOSION_RADIUS: f32 = 6.0;
const EXPLOSION_DELTA_V: f32 = 14.0;

pub struct WeaponsPlugin;

impl Plugin for WeaponsPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(Arsenal { current: Weapon::Pistol, cooldown: 0.0 })
            .add_systems(Update, (switch_weapon, shoot, explode, fade_markers));
    }
}

fn switch_weapon(keys: Res<ButtonInput<KeyCode>>, mut arsenal: ResMut<Arsenal>) {
    if keys.just_pressed(KeyCode::Digit1) {
        arsenal.current = Weapon::Pistol;
    }
    if keys.just_pressed(KeyCode::Digit2) {
        arsenal.current = Weapon::Shotgun;
    }
}

/// Small deterministic pseudo-random numbers for shotgun spread, so we don't
/// need an extra dependency.
fn jitter(seed: &mut u32) -> f32 {
    *seed ^= *seed << 13;
    *seed ^= *seed >> 17;
    *seed ^= *seed << 5;
    (*seed as f32 / u32::MAX as f32) * 2.0 - 1.0
}

fn shoot(
    mut commands: Commands,
    time: Res<Time>,
    mouse: Res<ButtonInput<MouseButton>>,
    captured: Res<MouseCaptured>,
    mut arsenal: ResMut<Arsenal>,
    camera: Single<&GlobalTransform, With<PlayerCamera>>,
    rapier: ReadRapierContext,
    mut bodies: Query<(&GlobalTransform, &mut ExternalImpulse)>,
    names: Query<&Name>,
    mut seed: Local<u32>,
) {
    arsenal.cooldown -= time.delta_secs();
    // `is_changed`: the click that captures the mouse shouldn't also fire.
    if !captured.0 || captured.is_changed() {
        return;
    }
    if !mouse.just_pressed(MouseButton::Left) || arsenal.cooldown > 0.0 {
        return;
    }
    let Ok(context) = rapier.single() else { return };
    let weapon = stats(arsenal.current);
    arsenal.cooldown = weapon.cooldown;
    if *seed == 0 {
        *seed = 0x9E37_79B9;
    }

    let (origin, forward) = aim_ray(&camera);
    let right = camera.right().as_vec3();
    let up = camera.up().as_vec3();

    for _ in 0..weapon.pellets {
        let dir = (forward + right * jitter(&mut seed) * weapon.spread + up * jitter(&mut seed) * weapon.spread).normalize();
        let hit = context.with_query_pipeline(aim_filter(), |q| q.cast_ray(origin, dir, 200.0, true));
        let Some((entity, distance)) = hit else { continue };
        let point = origin + dir * distance;
        spawn_marker(&mut commands, point);
        let name = names.get(entity).map(|n| n.as_str()).unwrap_or("?");
        debug!("{:?} hit {name} at {point:.2}", arsenal.current);

        // Hit something movable? Kick it at the hit point. The spin part
        // (torque impulse) is lever arm × impulse, measured from the body's
        // centre of mass. Our shapes are centred, so that's the body's origin.
        if let Ok((body, mut impulse)) = bodies.get_mut(entity) {
            let kick = dir * weapon.impulse;
            impulse.impulse += kick;
            impulse.torque_impulse += (point - body.translation()).cross(kick);
        }
    }
}

/// E = explosion where the crosshair points.
fn explode(
    mut commands: Commands,
    keys: Res<ButtonInput<KeyCode>>,
    camera: Single<&GlobalTransform, With<PlayerCamera>>,
    rapier: ReadRapierContext,
    mut bodies: Query<(&GlobalTransform, &mut ExternalImpulse, Option<&RagdollPart>, Option<&Prop>)>,
) {
    if !keys.just_pressed(KeyCode::KeyE) {
        return;
    }
    let Ok(context) = rapier.single() else { return };
    let (origin, dir) = aim_ray(&camera);
    let Some((_, distance)) = context.with_query_pipeline(aim_filter(), |q| q.cast_ray(origin, dir, 200.0, true)) else {
        return;
    };
    let center = origin + dir * distance;

    // Every body inside the radius is pushed away from the centre, harder the
    // closer it is, with extra upward lift (explosions throw things *up*).
    for (transform, mut impulse, part, prop) in &mut bodies {
        let Some(mass) = part.map(|p| p.mass).or(prop.map(|p| p.mass)) else { continue };
        let offset = transform.translation() - center;
        let falloff = 1.0 - offset.length() / EXPLOSION_RADIUS;
        if falloff <= 0.0 {
            continue;
        }
        let direction = (offset.normalize_or_zero() + Vec3::Y * 0.6).normalize();
        impulse.impulse += direction * EXPLOSION_DELTA_V * falloff * mass;
    }

    commands.spawn((
        Marker { life: 0.4, size: EXPLOSION_RADIUS * 0.4 },
        Shape::Ball(1.0),
        Paint(Color::srgb(1.0, 0.6, 0.1)),
        Transform::from_translation(center),
    ));
}

/// A short-lived ball showing where something hit.
#[derive(Component)]
struct Marker {
    life: f32,
    size: f32,
}

fn spawn_marker(commands: &mut Commands, point: Vec3) {
    commands.spawn((
        Marker { life: 0.25, size: 0.04 },
        Shape::Ball(1.0),
        Paint(Color::srgb(1.0, 0.9, 0.3)),
        Transform::from_translation(point).with_scale(Vec3::splat(0.04)),
    ));
}

fn fade_markers(mut commands: Commands, time: Res<Time>, mut markers: Query<(Entity, &mut Marker, &mut Transform)>) {
    for (entity, mut marker, mut transform) in &mut markers {
        marker.life -= time.delta_secs();
        if marker.life <= 0.0 {
            commands.entity(entity).despawn();
        } else {
            transform.scale = Vec3::splat(marker.size * (marker.life * 4.0).min(1.0));
        }
    }
}
