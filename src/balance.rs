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
//! 4. If the ankles can't cope, **step**: swing a foot out to land just past
//!    the capture point. A new foot under the body stops the fall; that's a
//!    stagger. Several steps in a row are allowed.
//! 5. If even stepping can't help, balance is lost. **Fall**: throw the arms
//!    out toward the fall to catch yourself, tuck the chin, and once on the
//!    ground, relax.
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
    /// Whether the person may take steps to catch themselves.
    pub stepping: bool,
    /// How long one step takes (lift, swing, plant), in seconds.
    pub step_duration: f32,
    /// How strongly the standing hip keeps the pelvis upright in the world,
    /// front-to-back and sideways (rad of hip correction per rad of tilt).
    pub pelvis_pitch_gain: f32,
    pub pelvis_roll_gain: f32,
}

impl Default for BalanceTuning {
    fn default() -> Self {
        // Picked by sweeping values in the `print_tuning_sweep` test. Gentle
        // gains do best: pushing harder only rolls the feet onto their edges.
        BalanceTuning { ankle_gain_forward: 1.0, ankle_gain_sideways: 1.0, hip_gain: 1.5, max_ankle_correction: 0.2, lean: -0.02, stepping: true, step_duration: 0.32, pelvis_pitch_gain: 1.5, pelvis_roll_gain: -1.0 }
    }
}

/// The feet span roughly 24 cm front-to-back and 28 cm side-to-side. Without
/// stepping, a capture point further than this from their centre is hopeless.
const FALL_THRESHOLD: f32 = 0.30;

// --- Stepping ---------------------------------------------------------------
/// Take a step once the capture point is this far (m) from the feet's centre:
/// about the edge of the feet, where the ankles start to lose.
const STEP_THRESHOLD: f32 = 0.11;
/// The furthest a step can reach (m from the standing foot). A capture point
/// further away than this can't be caught: fall instead.
const MAX_STEP: f32 = 0.55;
/// How high the foot lifts mid-swing (m).
const STEP_HEIGHT: f32 = 0.07;
/// Land this far past the capture point, so it ends up between the feet.
const STEP_OVERSHOOT: f32 = 0.06;
/// Give up after this many steps in a row (a stagger that never ends).
const MAX_STEPS: u32 = 6;
/// Muscles always trail their target a little, so the swinging foot aims
/// this far (seconds) ahead along its path.
const STEP_LEAD: f32 = 0.08;

/// Leg segment lengths (hip to knee, knee to ankle), from `ragdoll.rs`.
const THIGH: f32 = 0.42;
const SHIN: f32 = 0.42;
/// Hip joint position relative to the pelvis centre (left side; mirror x).
const HIP_OFFSET: Vec3 = Vec3::new(0.09, -0.05, 0.0);
/// Ankle height above the ground, and how far the foot's centre sits in
/// front of the ankle.
const ANKLE_HEIGHT: f32 = 0.09;
const FOOT_FORWARD: f32 = 0.04;
/// Also give up if the chest leans more than this (radians, ~40°).
const FALL_TILT: f32 = 0.7;

/// Muscle tone while falling (bracing) and after landing (lying there).
/// Bracing is a reflex: it has to be fast, so muscles stay fully tensed while
/// falling. (The protective pose itself softens the knees.)
const FALLING_TONE: f32 = 1.0;
const DOWN_TONE: f32 = 0.12;

/// Which leg.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side {
    Left,
    Right,
}

impl Side {
    /// +1 for left (+X in the body's own frame), -1 for right.
    fn sign(self) -> f32 {
        match self {
            Side::Left => 1.0,
            Side::Right => -1.0,
        }
    }
    fn other(self) -> Side {
        match self {
            Side::Left => Side::Right,
            Side::Right => Side::Left,
        }
    }
}

/// What the person is currently doing.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum BalanceState {
    /// On their feet, actively balancing.
    Standing,
    /// Taking a step to catch themselves: moving the `side` foot's centre
    /// from `from` to `to` (world positions on the ground).
    Stepping { side: Side, from: Vec3, to: Vec3 },
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
    /// Steps taken in the current stagger, and which leg stepped last.
    pub steps: u32,
    pub last_step: Option<Side>,
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
            steps: 0,
            last_step: None,
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
    /// Centre of each foot, projected onto the ground.
    foot_l: Vec3,
    foot_r: Vec3,
}

fn sense(parts: &Query<(&RagdollPart, &Transform, &Velocity)>, pelvis: &Transform) -> BodyState {
    let mut mass = 0.0;
    let mut com = Vec3::ZERO;
    let mut momentum = Vec3::ZERO;
    let mut feet = Vec3::ZERO;
    let mut foot_l = Vec3::ZERO;
    let mut foot_r = Vec3::ZERO;
    let mut chest_up = Vec3::Y;
    for (part, transform, velocity) in parts.iter() {
        mass += part.mass;
        com += transform.translation * part.mass;
        momentum += velocity.linear * part.mass;
        match part.part {
            BodyPart::FootL => {
                feet += transform.translation / 2.0;
                foot_l = transform.translation * Vec3::new(1.0, 0.0, 1.0);
            }
            BodyPart::FootR => {
                feet += transform.translation / 2.0;
                foot_r = transform.translation * Vec3::new(1.0, 0.0, 1.0);
            }
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
        foot_l,
        foot_r,
    }
}

/// Decide where to step: which leg, and where its foot should land.
/// Returns `None` if the capture point is out of reach (time to fall).
fn plan_step(body: &BodyState, capture_point: Vec3, last: Option<Side>) -> Option<(Side, Vec3, Vec3)> {
    let error = (capture_point - body.support_center) * Vec3::new(1.0, 0.0, 1.0);
    let lateral = error.dot(body.left);
    let forward = error.dot(body.forward);
    // After a step, the leg that just landed is carrying the body, so the
    // next step is always with the other leg, like walking. For the first
    // step: if pushed mostly sideways, step with the leg on that side (so the
    // legs never cross); otherwise with the leg on the side the push leans to.
    let side = match last {
        Some(previous) => previous.other(),
        None if lateral.abs() > 0.6 * forward.abs() => {
            if lateral > 0.0 { Side::Left } else { Side::Right }
        }
        None => {
            if lateral >= 0.0 { Side::Left } else { Side::Right }
        }
    };
    let _ = forward;
    let swing = match side {
        Side::Left => body.foot_l,
        Side::Right => body.foot_r,
    };
    let target = step_target(body, capture_point, side)?;
    Some((side, swing, target))
}

/// Where the `side` foot should land to catch the body: just past the
/// capture point, on its own side of the body (so the capture point ends up
/// between the two feet), never crossing the other leg. `None` if that's
/// further than a step can reach.
fn step_target(body: &BodyState, capture_point: Vec3, side: Side) -> Option<Vec3> {
    let stance = match side {
        Side::Left => body.foot_r,
        Side::Right => body.foot_l,
    };
    let cp = Vec3::new(capture_point.x, 0.0, capture_point.z);
    let error = cp - body.support_center;
    let mut target = cp + error.normalize_or_zero() * STEP_OVERSHOOT + body.left * side.sign() * 0.09;
    let across = (target - stance).dot(body.left) * side.sign();
    if across < 0.14 {
        target += body.left * side.sign() * (0.14 - across);
    }
    ((target - stance).length() <= MAX_STEP).then_some(target)
}

/// Joint angles (hip, knee, ankle) that put a leg's ankle at `ankle_world`
/// with the foot flat. Two-segment "inverse kinematics": given where the end
/// of the leg should be, work out the joint angles.
fn leg_ik(pelvis: &Transform, side: Side, ankle_world: Vec3) -> (Vec3, Vec3, Vec3) {
    let hip = HIP_OFFSET * Vec3::new(side.sign(), 1.0, 1.0);
    // Ankle position relative to the hip, in the pelvis's own frame
    // (x = left, y = up, z = forward).
    let d = pelvis.rotation.inverse() * (ankle_world - pelvis.translation) - hip;
    let reach = d.length().clamp(0.3, THIGH + SHIN - 0.01);
    // Law of cosines: the angle inside the knee for this hip-to-ankle distance.
    let inner = ((THIGH * THIGH + SHIN * SHIN - reach * reach) / (2.0 * THIGH * SHIN)).clamp(-1.0, 1.0).acos();
    let knee = std::f32::consts::PI - inner; // 0 = straight leg
    let pitch = d.z.atan2(-d.y); // how far forward the ankle is from straight down
    let roll = d.x.atan2(-d.y); // how far left
    // The thigh points knee/2 further forward than the hip-to-ankle line (both
    // segments are equally long); forward swing is negative X.
    let hip_angles = Vec3::new(-(pitch + knee / 2.0), 0.0, roll);
    let knee_angles = Vec3::new(knee, 0.0, 0.0);
    // Undo the leg's total rotation so the sole stays parallel to the pelvis.
    let ankle_angles = Vec3::new(-(hip_angles.x + knee), 0.0, -roll);
    (hip_angles, knee_angles, ankle_angles)
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
    let falling = BalanceState::Falling { direction: error.normalize_or(body.forward) };
    let next = match balance.state {
        _ if matches!(balance.state, BalanceState::Standing | BalanceState::Stepping { .. }) && body.chest_tilt > FALL_TILT => {
            Some(falling)
        }
        BalanceState::Standing if tuning.stepping && error.length() > STEP_THRESHOLD => {
            // Short pause between steps, so each foot really lands.
            if balance.time_in_state < 0.05 && balance.steps > 0 {
                None
            } else if balance.steps >= MAX_STEPS {
                Some(falling)
            } else {
                match plan_step(&body, capture_point, balance.last_step) {
                    Some((side, from, to)) => Some(BalanceState::Stepping { side, from, to }),
                    None => Some(falling),
                }
            }
        }
        BalanceState::Standing if !tuning.stepping && error.length() > FALL_THRESHOLD => Some(falling),
        BalanceState::Standing if error.length() < STEP_THRESHOLD * 0.5 && balance.time_in_state > 0.5 => {
            // Recovered: the stagger is over.
            balance.steps = 0;
            balance.last_step = None;
            None
        }
        BalanceState::Stepping { side, .. } if balance.time_in_state >= tuning.step_duration => {
            balance.steps += 1;
            balance.last_step = Some(side);
            Some(BalanceState::Standing)
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
        BalanceState::Stepping { side, from, mut to } => {
            tone_scale.0 = 1.0;
            // Keep aiming at where the capture point is *now*: people adjust
            // their step mid-swing. (If it's out of reach, keep the old target;
            // the fall check will take over.)
            if let Some(new_to) = step_target(&body, capture_point, side) {
                to = new_to;
                balance.state = BalanceState::Stepping { side, from, to };
            }
            // Where the swinging foot is along its arc: slide from `from` to
            // `to` (smoothly speeding up and slowing down), lifting in between.
            let s = ((balance.time_in_state + STEP_LEAD) / tuning.step_duration).clamp(0.0, 1.0);
            let ease = s * s * (3.0 - 2.0 * s);
            let foot = from.lerp(to, ease) + Vec3::Y * STEP_HEIGHT * (std::f32::consts::PI * s).sin();
            let ankle = foot + Vec3::Y * ANKLE_HEIGHT - body.forward * FOOT_FORWARD;
            let (hip, knee, ankle_angles) = leg_ik(pelvis, side, ankle);
            let (swing_thigh, swing_shin, swing_foot, stance_thigh) = match side {
                Side::Left => (BodyPart::ThighL, BodyPart::ShinL, BodyPart::FootL, BodyPart::ThighR),
                Side::Right => (BodyPart::ThighR, BodyPart::ShinR, BodyPart::FootR, BodyPart::ThighL),
            };
            // On one leg, nothing else keeps the pelvis upright: the standing
            // hip rotates it back toward vertical *in the world*, not just
            // relative to the leg. (The trick behind SIMBICON, a classic
            // walking controller.) The gains' signs were found by testing:
            // tilted forward = extend the hip (+X); the sideways gain is
            // negative (`print_step_sweep` in the tests: the other sign halves
            // how hard a sideways shove they can take).
            let pelvis_up = pelvis.rotation * Vec3::Y;
            let pelvis_pitch = pelvis_up.dot(body.forward).asin();
            let pelvis_roll = pelvis_up.dot(body.left).asin();
            for (part, mut target) in &mut targets {
                let p = part.part;
                target.0 = if p == swing_thigh {
                    hip
                } else if p == swing_shin {
                    knee
                } else if p == swing_foot {
                    ankle_angles
                } else if p == stance_thigh {
                    standing_pose(p) + Vec3::new(pelvis_pitch * tuning.pelvis_pitch_gain, 0.0, pelvis_roll * tuning.pelvis_roll_gain)
                } else {
                    // The standing leg just holds the foot flat. While it carries
                    // the whole body alone, balance corrections would only roll
                    // the foot onto its edge; the step does the balancing.
                    // Arms flung out sideways: the classic stagger. (Reaching
                    // forward instead looked more protective but moved the
                    // body's weight forward and made forward staggers fail.)
                    let mut angles = standing_pose(p);
                    match p {
                        BodyPart::UpperArmL => angles.z += 0.5,
                        BodyPart::UpperArmR => angles.z -= 0.5,
                        _ => {}
                    }
                    angles
                };
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
