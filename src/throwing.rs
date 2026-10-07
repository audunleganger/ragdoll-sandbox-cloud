//! Throwing objects: hold the right mouse button to charge, release to throw.
//! Q switches between a light ball and a heavy crate.
//!
//! Unlike bullets, thrown objects are real rigid bodies. When one hits the
//! person, the physics engine works out the collision itself: a heavy, fast
//! crate transfers much more momentum (mass × velocity) than a light ball.

use bevy::prelude::*;
use bevy_rapier3d::prelude::*;

use crate::layers;
use crate::player::{MouseCaptured, PlayerCamera, aim_ray};
use crate::shape::{Paint, Shape};

/// Seconds of holding needed for a full-power throw.
const FULL_CHARGE_TIME: f32 = 1.0;
const MIN_THROW_SPEED: f32 = 6.0;
const MAX_THROW_SPEED: f32 = 26.0;
/// Oldest objects are removed beyond this many, to keep things fast.
const MAX_PROPS: usize = 40;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Throwable {
    Ball,
    Crate,
}

impl Throwable {
    fn shape(self) -> Shape {
        match self {
            Throwable::Ball => Shape::Ball(0.12),
            Throwable::Crate => Shape::Cuboid(Vec3::splat(0.3)),
        }
    }
    fn mass(self) -> f32 {
        match self {
            Throwable::Ball => 2.0,
            Throwable::Crate => 25.0,
        }
    }
    fn color(self) -> Color {
        match self {
            Throwable::Ball => Color::srgb(0.85, 0.25, 0.25),
            Throwable::Crate => Color::srgb(0.60, 0.42, 0.22),
        }
    }
}

/// A thrown object. Spawn order lets us remove the oldest ones first.
#[derive(Component)]
pub struct Prop {
    pub mass: f32,
    order: u64,
}

#[derive(Resource)]
pub struct ThrowState {
    pub selected: Throwable,
    /// 0..1 while charging, `None` when not holding the button.
    pub charge: Option<f32>,
    spawned: u64,
}

pub struct ThrowingPlugin;

impl Plugin for ThrowingPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(ThrowState { selected: Throwable::Ball, charge: None, spawned: 0 })
            .add_systems(Update, (select, charge_and_throw, limit_props));
    }
}

fn select(keys: Res<ButtonInput<KeyCode>>, mut state: ResMut<ThrowState>) {
    if keys.just_pressed(KeyCode::KeyQ) {
        state.selected = match state.selected {
            Throwable::Ball => Throwable::Crate,
            Throwable::Crate => Throwable::Ball,
        };
    }
}

fn charge_and_throw(
    mut commands: Commands,
    time: Res<Time>,
    mouse: Res<ButtonInput<MouseButton>>,
    captured: Res<MouseCaptured>,
    mut state: ResMut<ThrowState>,
    camera: Single<&GlobalTransform, With<PlayerCamera>>,
) {
    if !captured.0 {
        state.charge = None;
        return;
    }
    if mouse.pressed(MouseButton::Right) {
        let charge = state.charge.unwrap_or(0.0) + time.delta_secs() / FULL_CHARGE_TIME;
        state.charge = Some(charge.min(1.0));
        return;
    }
    let Some(charge) = state.charge.take() else { return };

    let (origin, dir) = aim_ray(&camera);
    let kind = state.selected;
    let speed = MIN_THROW_SPEED + (MAX_THROW_SPEED - MIN_THROW_SPEED) * charge;
    state.spawned += 1;
    commands.spawn((
        Name::new(format!("{kind:?}")),
        Prop { mass: kind.mass(), order: state.spawned },
        kind.shape(),
        Paint(kind.color()),
        // Start a little in front of the camera, past the player's shoulder.
        Transform::from_translation(origin + dir * 1.5),
        RigidBody::Dynamic,
        kind.shape().collider(),
        ColliderMassProperties::Mass(kind.mass()),
        layers::prop(),
        // Slight upward arc and a little spin, like a real throw.
        Velocity { linear: dir * speed + Vec3::Y * 1.5, angular: camera.right().as_vec3() * -4.0 },
        // Fast small objects can tunnel through thin things between physics
        // steps; continuous collision detection (CCD) prevents that.
        Ccd::enabled(),
        ExternalImpulse::default(),
    ));
}

fn limit_props(mut commands: Commands, props: Query<(Entity, &Prop)>) {
    let count = props.iter().count();
    if count <= MAX_PROPS {
        return;
    }
    let mut by_age: Vec<_> = props.iter().map(|(e, p)| (p.order, e)).collect();
    by_age.sort();
    for (_, entity) in by_age.into_iter().take(count - MAX_PROPS) {
        commands.entity(entity).despawn();
    }
}
