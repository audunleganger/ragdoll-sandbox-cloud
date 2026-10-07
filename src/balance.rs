//! Balance and fall reactions: the "brain" that decides where the muscles
//! should pull.
//!
//! `muscles.rs` holds whatever pose it's given. This file chooses that pose
//! every frame, based on what the body is doing:
//!
//! 1. **Sense** the centre of mass (COM) and how fast it moves.
//! 2. **Predict** where it's heading: the *capture point*.
//! 3. **Correct** by shifting the ankle (and hip) targets so the feet push the
//!    COM back over them, the way people sway on their ankles to stay up.
//! 4. If the capture point gets too far outside the feet, balance is lost.
//!    **Fall**: throw the arms out toward the fall to catch yourself, tuck the
//!    chin, and once on the ground, relax.
//!
//! ## The capture point
//!
//! A standing body behaves like an upside-down pendulum. If the COM is at
//! height `h` and moves at speed `v`, it will come to rest above the point
//!
//! ```text
//! capture point = COM position + v × √(h / g)
//! ```
//!
//! (`g` = 9.81 m/s², gravity). If the feet are under that point, the body can
//! stop; if not, it will fall unless it steps. Using the capture point instead
//! of the COM alone means the controller reacts to a shove *immediately*,
//! before the body has leaned far.

use bevy::prelude::*;
use bevy_rapier3d::prelude::Velocity;

use crate::muscles::{JointTarget, ToneScale, standing_pose};
use crate::ragdoll::{BodyPart, RagdollPart};

const GRAVITY: f32 = 9.81;

/// Balance controller settings. A resource so tests can try variations.
#[derive(Resource, Clone, Copy, Debug)]
pub struct BalanceTuning {
    /// Extra ankle angle (rad) per metre of capture-point error, front-to-back.
    pub ankle_gain_forward: f32,
    /// Extra ankle roll (rad) per metre of capture-point error, side-to-side.
    pub ankle_gain_sideways: f32,
    /// Extra hip bend (rad) per metre of error, front-to-back.
    pub hip_gain: f32,
    /// Largest correction the ankles may ask for (rad). A foot can only push
    /// as hard as body weight × distance to the toe (or heel) allows; more than
    /// that and the foot rolls onto its edge instead of pushing the body back.
    pub max_ankle_correction: f32,
    /// Constant ankle angle (rad) added while standing. Negative = lean
    /// slightly forward, which puts the weight over the middle of the feet
    /// instead of near the heels, leaving room to sway both ways.
    pub lean: f32,
}

impl Default for BalanceTuning {
    fn default() -> Self {
        // Picked by sweeping values in the `print_tuning_sweep` test. Gentle
        // gains do best: pushing harder only rolls the feet onto their edges.
        BalanceTuning { ankle_gain_forward: 1.0, ankle_gain_sideways: 1.0, hip_gain: 1.5, max_ankle_correction: 0.2, lean: -0.02 }
    }
}

/// The feet span roughly 24 cm front-to-back and 28 cm side-to-side. If the
/// capture point is further than this from their centre, standing is hopeless.
const FALL_THRESHOLD: f32 = 0.30;
/// Also give up if the chest leans more than this (radians, ~40°).
const FALL_TILT: f32 = 0.7;

/// Muscle tone while falling (bracing) and after landing (lying there).
/// Bracing is a reflex: it has to be fast, so muscles stay fully tensed while
/// falling. (The protective pose itself softens the knees.)
const FALLING_TONE: f32 = 1.0;
const DOWN_TONE: f32 = 0.12;

/// What the person is currently doing.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum BalanceState {
    /// On their feet, actively balancing.
    Standing,
    /// Balance lost: bracing for impact. `direction` is the horizontal
    /// direction of the fall (world space).
    Falling { direction: Vec3 },
    /// On the ground, relaxing.
    Down,
}

/// The balance controller's memory. One per person, on the pelvis.
#[derive(Component, Debug)]
pub struct Balance {
    pub state: BalanceState,
    /// Seconds spent in the current state.
    pub time_in_state: f32,
    /// Latest measurements, kept for the debug overlay and tests.
    pub com: Vec3,
    pub com_velocity: Vec3,
    pub capture_point: Vec3,
    pub support_center: Vec3,
}

impl Default for Balance {
    fn default() -> Self {
        Balance {
            state: BalanceState::Standing,
            time_in_state: 0.0,
            com: Vec3::ZERO,
            com_velocity: Vec3::ZERO,
            capture_point: Vec3::ZERO,
            support_center: Vec3::ZERO,
        }
    }
}

/// Switch balancing on or off (for comparing with Stage 2's statue).
#[derive(Resource)]
pub struct BalanceEnabled(pub bool);

impl Default for BalanceEnabled {
    fn default() -> Self {
        BalanceEnabled(true)
    }
}

pub struct BalancePlugin;

impl Plugin for BalancePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<BalanceEnabled>().init_resource::<BalanceTuning>().add_systems(
            PostUpdate,
            (attach_balance, think)
                .chain()
                .before(crate::muscles::MuscleSet),
        );
    }
}

/// Give every newly spawned person a fresh balance controller.
fn attach_balance(
    mut commands: Commands,
    new_parts: Query<(Entity, &RagdollPart), Added<RagdollPart>>,
    mut tone_scale: ResMut<ToneScale>,
) {
    for (entity, part) in &new_parts {
        if part.part == BodyPart::Pelvis {
            commands.entity(entity).insert(Balance::default());
            tone_scale.0 = 1.0;
        }
    }
}

/// Everything the controller needs to know about the body this frame.
struct BodyState {
    com: Vec3,
    com_velocity: Vec3,
    support_center: Vec3,
    /// Horizontal unit vectors of the person's facing direction.
    forward: Vec3,
    left: Vec3,
    /// How far the chest leans away from vertical (radians).
    chest_tilt: f32,
}

fn sense(parts: &Query<(&RagdollPart, &Transform, &Velocity)>, pelvis: &Transform) -> BodyState {
    let mut mass = 0.0;
    let mut com = Vec3::ZERO;
    let mut momentum = Vec3::ZERO;
    let mut feet = Vec3::ZERO;
    let mut chest_up = Vec3::Y;
    for (part, transform, velocity) in parts.iter() {
        mass += part.mass;
        com += transform.translation * part.mass;
        momentum += velocity.linear * part.mass;
        match part.part {
            BodyPart::FootL | BodyPart::FootR => feet += transform.translation / 2.0,
            BodyPart::Chest => chest_up = transform.rotation * Vec3::Y,
            _ => {}
        }
    }
    // The person was built facing +Z, with their left at +X.
    let flat = |v: Vec3| Vec3::new(v.x, 0.0, v.z).normalize_or(Vec3::Z);
    let forward = flat(pelvis.rotation * Vec3::Z);
    BodyState {
        com: com / mass,
        com_velocity: momentum / mass,
        support_center: Vec3::new(feet.x, 0.0, feet.z),
        forward,
        left: Vec3::Y.cross(forward),
        chest_tilt: chest_up.angle_between(Vec3::Y),
    }
}

/// The balance "brain": runs every frame, before the muscles.
fn think(
    time: Res<Time>,
    enabled: Res<BalanceEnabled>,
    tuning: Res<BalanceTuning>,
    mut tone_scale: ResMut<ToneScale>,
    mut people: Query<(&mut Balance, &Transform)>,
    parts: Query<(&RagdollPart, &Transform, &Velocity)>,
    mut targets: Query<(&RagdollPart, &mut JointTarget)>,
) {
    let Some((mut balance, pelvis)) = people.iter_mut().next() else { return };
    let body = sense(&parts, pelvis);
    let dt = time.delta_secs();
    balance.time_in_state += dt;

    // Capture point: where the COM would come to rest (see the top of this file).
    let height = body.com.y.max(0.1);
    let capture_point = body.com + body.com_velocity * (height / GRAVITY).sqrt();
    let error = Vec3::new(capture_point.x - body.support_center.x, 0.0, capture_point.z - body.support_center.z);
    balance.com = body.com;
    balance.com_velocity = body.com_velocity;
    balance.capture_point = capture_point;
    balance.support_center = body.support_center;

    // Switched off: leave the targets alone (they start as the standing pose).
    if !enabled.0 {
        return;
    }

    // --- Decide which state we're in -------------------------------------
    let next = match balance.state {
        BalanceState::Standing if error.length() > FALL_THRESHOLD || body.chest_tilt > FALL_TILT => {
            Some(BalanceState::Falling { direction: error.normalize_or(body.forward) })
        }
        // Landed: the COM is low, or we've been falling long enough.
        BalanceState::Falling { .. } if body.com.y < 0.45 || balance.time_in_state > 2.5 => Some(BalanceState::Down),
        _ => None,
    };
    if let Some(next) = next {
        balance.state = next;
        balance.time_in_state = 0.0;
    }

    // --- Act --------------------------------------------------------------
    let forward_error = error.dot(body.forward);
    let left_error = error.dot(body.left);
    match balance.state {
        BalanceState::Standing => {
            tone_scale.0 = 1.0;
            let max = tuning.max_ankle_correction;
            let ankle_pitch = (forward_error * tuning.ankle_gain_forward).clamp(-max, max);
            let ankle_roll = (left_error * tuning.ankle_gain_sideways).clamp(-max, max);
            let hip_pitch = -forward_error * tuning.hip_gain;
            for (part, mut target) in &mut targets {
                let mut angles = standing_pose(part.part);
                match part.part {
                    BodyPart::FootL | BodyPart::FootR => {
                        angles.x += ankle_pitch + tuning.lean;
                        angles.z -= ankle_roll;
                    }
                    BodyPart::ThighL | BodyPart::ThighR => angles.x += hip_pitch,
                    _ => {}
                }
                target.0 = angles;
            }
        }
        BalanceState::Falling { direction } => {
            // Brace: arms thrown toward the fall, chin tucked, knees soft.
            tone_scale.0 = FALLING_TONE;
            let fall_forward = direction.dot(body.forward);
            let fall_left = direction.dot(body.left);
            for (part, mut target) in &mut targets {
                target.0 = protective_pose(part.part, fall_forward, fall_left);
            }
        }
        BalanceState::Down => {
            // Ease off over a second and a half: lying there, not a noodle.
            let t = (balance.time_in_state / 1.5).min(1.0);
            tone_scale.0 = FALLING_TONE + (DOWN_TONE - FALLING_TONE) * t;
        }
    }
}

/// Bracing pose while falling. `fall_forward` and `fall_left` are the
/// components of the fall direction (in the person's own frame).
pub fn protective_pose(part: BodyPart, fall_forward: f32, fall_left: f32) -> Vec3 {
    use BodyPart::*;
    // Shoulder: arms forward (negative X) when falling forward; out to the
    // side when falling sideways; when falling backward, arms go out and
    // slightly back to break the fall.
    let raise_forward = 1.3 * fall_forward.max(0.0);
    let raise_back = 0.6 * (-fall_forward).max(0.0);
    let side_out = 1.0 + 0.8 * fall_left.abs();
    match part {
        UpperArmL => Vec3::new(-raise_forward + raise_back, 0.0, side_out * 0.8),
        UpperArmR => Vec3::new(-raise_forward + raise_back, 0.0, -side_out * 0.8),
        ForearmL | ForearmR => Vec3::new(-0.4, 0.0, 0.0),
        // Chin tucked (protect the head), slight curl of the spine.
        Head => Vec3::new(0.5 * (-fall_forward).max(0.0) + 0.2, 0.0, 0.0),
        Belly | Chest => Vec3::new(0.15, 0.0, 0.0),
        // Knees soften so the fall isn't stiff as a plank.
        ShinL | ShinR => Vec3::new(0.5, 0.0, 0.0),
        ThighL | ThighR => Vec3::new(-0.4, 0.0, 0.0),
        _ => standing_pose(part),
    }
}
