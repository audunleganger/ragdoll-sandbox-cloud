//! Muscles: motors in every joint that pull the body toward a target pose.
//!
//! This is the heart of an "active ragdoll" (what Euphoria does in GTA IV).
//! A plain ragdoll is a dead body. Give each joint a muscle and it can hold
//! itself up, and still be pushed, shot and knocked over by physics.
//!
//! ## How one muscle works: a spring with a shock absorber
//!
//! For every free axis of a joint, the motor applies a torque (a twisting
//! force, in newton-metres):
//!
//! ```text
//! torque = stiffness × (target angle − current angle) − damping × rotation speed
//! ```
//!
//! * The **stiffness** part is a spring: the further the joint is from where
//!   it should be, the harder it pulls back.
//! * The **damping** part is a shock absorber: it resists fast movement, so
//!   the joint doesn't wobble back and forth around the target.
//! * The torque is **capped** at a maximum, like real muscle strength. This
//!   cap is what makes the body *give* under a big hit instead of standing
//!   there like a statue: a bullet or crate briefly overpowers the muscles.
//!
//! Engineers call this a **PD controller** (Proportional-Derivative). Rapier
//! runs it inside the physics solver, which keeps even stiff muscles stable.
//!
//! ## Muscle tone
//!
//! One number from 0 to 1 scales every muscle. 1 = fully tensed, 0 = limp
//! (only joint friction remains). Stage 3 lowers it automatically when the
//! person falls; for now you can change it with keys.

use bevy::prelude::*;
use bevy_rapier3d::prelude::*;

use crate::ragdoll::{BodyPart, RagdollJoint, RagdollPart};

/// How strong one joint's muscles are, at full tone.
#[derive(Clone, Copy, Debug)]
pub struct MuscleSpec {
    /// Torque per radian away from the target (N·m/rad).
    pub stiffness: f32,
    /// Torque per radian/second of rotation speed (N·m·s/rad).
    pub damping: f32,
    /// Strongest torque the muscle can produce (N·m).
    pub max_torque: f32,
}

/// Muscle strength per joint, named by the part *below* the joint (the knee
/// is `Shin`, the hip is `Thigh`, ...). Roughly based on real human strength.
///
/// Standing upright is balancing an 80 kg inverted pendulum on the ankles.
/// To hold it without active balance (Stage 3), the ankles need a stiffness
/// above about weight × height ≈ 800 N·m/rad, shared between both feet.
pub fn muscle_spec(part: BodyPart) -> MuscleSpec {
    use BodyPart::*;
    let (stiffness, damping, max_torque) = match part {
        Belly | Chest => (900.0, 60.0, 300.0),
        Head => (120.0, 8.0, 40.0),
        UpperArmL | UpperArmR => (140.0, 10.0, 60.0),
        ForearmL | ForearmR => (70.0, 5.0, 40.0),
        ThighL | ThighR => (1100.0, 70.0, 320.0),
        ShinL | ShinR => (1100.0, 70.0, 320.0),
        FootL | FootR => (900.0, 60.0, 260.0),
        Pelvis => (0.0, 0.0, 0.0), // the root: has no joint above it
    };
    MuscleSpec { stiffness, damping, max_torque }
}

/// Damping used when the muscles are off. Combined with the small torque cap
/// (the joint's friction) it acts like friction: it resists any movement
/// with up to the friction torque. Very high, so joints don't slowly creep
/// under their own weight, but the cap means it can't hold the body up.
pub const LIMP_DAMPING: f32 = 1000.0;

/// Global muscle tone, 0 (limp) to 1 (fully tensed).
#[derive(Resource, Clone, Copy, Debug)]
pub struct MuscleTone(pub f32);

impl Default for MuscleTone {
    fn default() -> Self {
        MuscleTone(1.0)
    }
}

/// A multiplier on muscle tone controlled by the balance "brain" (e.g. it
/// relaxes the body after a fall). The tone the muscles actually use is
/// `MuscleTone × ToneScale`, so your own tone setting still applies.
#[derive(Resource, Clone, Copy, Debug)]
pub struct ToneScale(pub f32);

impl Default for ToneScale {
    fn default() -> Self {
        ToneScale(1.0)
    }
}

/// Systems that copy targets into the joint motors. Anything that changes
/// targets or tone should run before this.
#[derive(SystemSet, Clone, PartialEq, Eq, Hash, Debug)]
pub struct MuscleSet;

/// The angle each joint's muscles are trying to reach, per axis (x, y, z) in
/// radians. Lives on the part below the joint.
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct JointTarget(pub Vec3);

/// The relaxed standing pose. Most joints are straight; elbows and knees are
/// slightly bent, and arms hang slightly away from the body.
pub fn standing_pose(part: BodyPart) -> Vec3 {
    use BodyPart::*;
    match part {
        ForearmL | ForearmR => Vec3::new(-0.25, 0.0, 0.0),
        UpperArmL => Vec3::new(0.05, 0.0, 0.08),
        UpperArmR => Vec3::new(0.05, 0.0, -0.08),
        ShinL | ShinR => Vec3::new(0.06, 0.0, 0.0),
        ThighL | ThighR => Vec3::new(-0.04, 0.0, 0.0),
        _ => Vec3::ZERO,
    }
}

pub struct MusclesPlugin;

impl Plugin for MusclesPlugin {
    fn build(&self, app: &mut App) {
        // Run right before every physics step, after anything that changes
        // targets or tone.
        app.init_resource::<MuscleTone>()
            .init_resource::<ToneScale>()
            .add_systems(FixedUpdate, drive_muscles.in_set(MuscleSet).before(PhysicsSet::SyncBackend));
    }
}

/// Copy the current targets and tone into the joint motors.
fn drive_muscles(
    tone: Res<MuscleTone>,
    scale: Res<ToneScale>,
    mut joints: Query<(&RagdollPart, &RagdollJoint, &JointTarget, &mut ImpulseJoint)>,
) {
    let t = (tone.0 * scale.0).clamp(0.0, 1.0);
    for (part, joint_info, target, mut joint) in &mut joints {
        let spec = muscle_spec(part.part);
        let mut data = *joint.data.as_ref();
        for (axis, angle) in [(JointAxis::AngX, target.0.x), (JointAxis::AngY, target.0.y), (JointAxis::AngZ, target.0.z)] {
            if data.limits(axis).is_none() {
                continue; // locked axis: nothing to drive
            }
            // Blend from friction-like damping (limp) to muscle damping
            // (tensed). The limp part fades out quickly, so half-tensed
            // muscles don't feel like they're moving through syrup.
            let damping = spec.damping * t + LIMP_DAMPING * (1.0 - t).powi(4);
            data.set_motor(axis, angle, 0.0, spec.stiffness * t, damping);
            data.set_motor_max_force(axis, (spec.max_torque * t).max(joint_info.friction));
        }
        // Only write when something changed: writing marks the joint as
        // changed, which makes the physics plugin re-upload it.
        if data != *joint.data.as_ref() {
            *joint.data.as_mut() = data;
        }
    }
}
