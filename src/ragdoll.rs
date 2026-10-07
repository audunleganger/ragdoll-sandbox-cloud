//! The ragdoll: 14 rigid body parts connected by joints.
//!
//! A "rigid body" is a solid object the physics engine moves around: it has a
//! mass, a position, a rotation and a velocity. A "joint" connects two bodies
//! at one point (the anchor) and limits how they may rotate relative to each
//! other, e.g. a knee only bends backwards.
//!
//! This file only builds the *skeleton*. The muscles that pull each joint
//! toward a target angle live in `muscles.rs`.
//!
//! ## Coordinate conventions
//!
//! * Y is up. The person is built facing +Z, so their **left is +X**.
//! * Every joint measures angles around three axes of the *parent* part:
//!   * **X (sideways axis)**: bending forward/backward ("pitch").
//!     For a limb hanging down, a positive angle swings it *backward*.
//!   * **Y (vertical axis)**: twisting ("yaw").
//!   * **Z (forward axis)**: tilting sideways ("roll").
//!     For a limb hanging down, a positive angle swings it toward +X (left).

use bevy::prelude::*;
use bevy_rapier3d::prelude::*;
use bevy_rapier3d::rapier::dynamics::JointAxesMask;

use crate::layers;
use crate::muscles::{JointTarget, LIMP_DAMPING, standing_pose};
use crate::shape::{Paint, Shape};

/// Which part of the body an entity is.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum BodyPart {
    Pelvis,
    Belly,
    Chest,
    Head,
    UpperArmL,
    ForearmL,
    UpperArmR,
    ForearmR,
    ThighL,
    ShinL,
    FootL,
    ThighR,
    ShinR,
    FootR,
}

/// Attached to every body part entity.
#[derive(Component, Clone, Copy, Debug)]
pub struct RagdollPart {
    pub part: BodyPart,
    pub mass: f32,
}

/// Attached to every part that hangs off a parent part, for inspection and tests.
#[derive(Component, Clone, Copy, Debug)]
pub struct RagdollJoint {
    pub parent: Entity,
    /// Joint position in the parent's local coordinates.
    pub anchor_on_parent: Vec3,
    /// Joint position in this part's local coordinates.
    pub anchor_on_child: Vec3,
    /// Torque (N·m) of the joint's friction, which remains when muscles are off.
    pub friction: f32,
}

/// Send this message to remove the current person and spawn a fresh one.
#[derive(Message, Clone, Copy)]
pub struct SpawnRagdoll {
    /// Where the person's feet go.
    pub position: Vec3,
    /// Which way they face: rotation around the vertical axis, in radians.
    /// 0 = facing +Z.
    pub yaw: f32,
}

impl SpawnRagdoll {
    /// The starting spot: at the origin, facing the player (who starts at -Z).
    pub const START: SpawnRagdoll = SpawnRagdoll { position: Vec3::ZERO, yaw: std::f32::consts::PI };
}

// ---------------------------------------------------------------------------
// Body layout: all measurements are in metres, kilograms and radians, for a
// person about 1.75 m tall standing at the origin with arms hanging down.
// ---------------------------------------------------------------------------

struct PartDef {
    part: BodyPart,
    shape: Shape,
    /// Centre of the part when standing. The centre is also the centre of mass.
    center: Vec3,
    mass: f32,
}

const fn capsule_y(half_length: f32, radius: f32) -> Shape {
    Shape::CapsuleY { half_length, radius }
}

const fn capsule_x(half_length: f32, radius: f32) -> Shape {
    Shape::CapsuleX { half_length, radius }
}

const HIP_X: f32 = 0.09;
const SHOULDER_X: f32 = 0.24;

#[rustfmt::skip]
fn part_defs() -> [PartDef; 14] {
    use BodyPart::*;
    let foot = Shape::Cuboid(Vec3::new(0.05, 0.03, 0.12));
    [
        PartDef { part: Pelvis,    shape: capsule_x(0.08, 0.10),  center: Vec3::new(0.0, 0.98, 0.0),  mass: 11.0 },
        PartDef { part: Belly,     shape: capsule_x(0.06, 0.09),  center: Vec3::new(0.0, 1.15, 0.0),  mass: 9.0 },
        PartDef { part: Chest,     shape: capsule_x(0.09, 0.11),  center: Vec3::new(0.0, 1.35, 0.0),  mass: 20.0 },
        PartDef { part: Head,      shape: Shape::Ball(0.10),       center: Vec3::new(0.0, 1.62, 0.0),  mass: 5.5 },
        PartDef { part: UpperArmL, shape: capsule_y(0.10, 0.045), center: Vec3::new( SHOULDER_X, 1.275, 0.0), mass: 2.2 },
        PartDef { part: ForearmL,  shape: capsule_y(0.11, 0.040), center: Vec3::new( SHOULDER_X, 0.98, 0.0),  mass: 1.8 },
        PartDef { part: UpperArmR, shape: capsule_y(0.10, 0.045), center: Vec3::new(-SHOULDER_X, 1.275, 0.0), mass: 2.2 },
        PartDef { part: ForearmR,  shape: capsule_y(0.11, 0.040), center: Vec3::new(-SHOULDER_X, 0.98, 0.0),  mass: 1.8 },
        PartDef { part: ThighL,    shape: capsule_y(0.145, 0.065), center: Vec3::new( HIP_X, 0.72, 0.0), mass: 9.0 },
        PartDef { part: ShinL,     shape: capsule_y(0.16, 0.05),  center: Vec3::new( HIP_X, 0.30, 0.0),  mass: 4.0 },
        PartDef { part: FootL,     shape: foot,                    center: Vec3::new( HIP_X, 0.03, 0.04), mass: 1.1 },
        PartDef { part: ThighR,    shape: capsule_y(0.145, 0.065), center: Vec3::new(-HIP_X, 0.72, 0.0), mass: 9.0 },
        PartDef { part: ShinR,     shape: capsule_y(0.16, 0.05),  center: Vec3::new(-HIP_X, 0.30, 0.0),  mass: 4.0 },
        PartDef { part: FootR,     shape: foot,                    center: Vec3::new(-HIP_X, 0.03, 0.04), mass: 1.1 },
    ]
}

/// Allowed rotation range `[min, max]` per axis. `None` = axis is locked.
#[derive(Clone, Copy)]
struct AngleLimits {
    x: Option<[f32; 2]>,
    y: Option<[f32; 2]>,
    z: Option<[f32; 2]>,
}

struct JointDef {
    child: BodyPart,
    parent: BodyPart,
    /// Joint position when standing (world coordinates of the rest pose).
    anchor: Vec3,
    limits: AngleLimits,
    /// Maximum torque (N·m) of the joint's built-in friction. Real joints
    /// aren't perfectly free; a little friction stops a limp body from flopping
    /// around like a wet noodle.
    friction: f32,
}

/// Swap a left-side `[min, max]` range to the right side. Mirroring the body
/// left-to-right flips the sign of sideways tilt (Z) and twist (Y).
const fn mirror(r: [f32; 2]) -> [f32; 2] {
    [-r[1], -r[0]]
}

const fn ball(x: [f32; 2], y: [f32; 2], z: [f32; 2]) -> AngleLimits {
    AngleLimits { x: Some(x), y: Some(y), z: Some(z) }
}

/// A hinge only bends around the sideways X axis (knee, elbow).
const fn hinge(x: [f32; 2]) -> AngleLimits {
    AngleLimits { x: Some(x), y: None, z: None }
}

// Left-side limb ranges; the right side is mirrored.
// Hip: forward swing is negative X (up to ~110°), backward up to ~30°.
const HIP: ([f32; 2], [f32; 2], [f32; 2]) = ([-1.9, 0.5], [-0.5, 0.5], [-0.35, 0.8]);
// Shoulder: forward raise is negative X; raising sideways is positive Z on the left.
const SHOULDER: ([f32; 2], [f32; 2], [f32; 2]) = ([-2.8, 0.9], [-1.2, 1.2], [-0.3, 2.6]);
// Ankle: tilting the toes down is positive X; a little sideways roll.
const ANKLE_X: [f32; 2] = [-0.7, 0.6];
const ANKLE_Z: [f32; 2] = [-0.3, 0.3];

#[rustfmt::skip]
fn joint_defs() -> Vec<JointDef> {
    use BodyPart::*;
    let hip = |side: f32| if side > 0.0 { ball(HIP.0, HIP.1, HIP.2) } else { ball(HIP.0, mirror(HIP.1), mirror(HIP.2)) };
    let shoulder = |side: f32| if side > 0.0 { ball(SHOULDER.0, SHOULDER.1, SHOULDER.2) } else { ball(SHOULDER.0, mirror(SHOULDER.1), mirror(SHOULDER.2)) };
    let ankle = |side: f32| AngleLimits { x: Some(ANKLE_X), y: None, z: Some(if side > 0.0 { ANKLE_Z } else { mirror(ANKLE_Z) }) };
    // Spine and neck: forward bend is positive X for a part pointing up.
    let spine = ball([-0.4, 0.7], [-0.4, 0.4], [-0.35, 0.35]);
    let neck = ball([-0.6, 0.8], [-1.0, 1.0], [-0.5, 0.5]);

    let mut joints = vec![
        JointDef { child: Belly, parent: Pelvis, anchor: Vec3::new(0.0, 1.07, 0.0), limits: spine, friction: 8.0 },
        JointDef { child: Chest, parent: Belly,  anchor: Vec3::new(0.0, 1.24, 0.0), limits: spine, friction: 8.0 },
        JointDef { child: Head,  parent: Chest,  anchor: Vec3::new(0.0, 1.50, 0.0), limits: neck,  friction: 1.5 },
    ];
    for (side, upper, fore, thigh, shin, foot) in [
        ( 1.0, UpperArmL, ForearmL, ThighL, ShinL, FootL),
        (-1.0, UpperArmR, ForearmR, ThighR, ShinR, FootR),
    ] {
        let sx = side * SHOULDER_X;
        let hx = side * HIP_X;
        joints.extend([
            JointDef { child: upper, parent: Chest, anchor: Vec3::new(sx, 1.42, 0.0), limits: shoulder(side), friction: 1.5 },
            // Elbow: the forearm folds forward, which is negative X.
            JointDef { child: fore,  parent: upper, anchor: Vec3::new(sx, 1.13, 0.0), limits: hinge([-2.5, 0.0]), friction: 1.0 },
            JointDef { child: thigh, parent: Pelvis, anchor: Vec3::new(hx, 0.93, 0.0), limits: hip(side), friction: 6.0 },
            // Knee: the shin folds backward, which is positive X.
            JointDef { child: shin,  parent: thigh, anchor: Vec3::new(hx, 0.51, 0.0), limits: hinge([0.0, 2.4]), friction: 4.0 },
            JointDef { child: foot,  parent: shin,  anchor: Vec3::new(hx, 0.09, 0.0), limits: ankle(side), friction: 2.0 },
        ]);
    }
    joints
}

/// Turn a joint description into a Rapier joint.
///
/// We use a "generic" joint: the three *linear* axes are always locked (the
/// two parts stay glued together at the anchor), and each *angular* axis is
/// either locked or free within a limited range.
fn build_joint(def: &JointDef, anchor_on_parent: Vec3, anchor_on_child: Vec3) -> GenericJoint {
    let mut locked = JointAxesMask::LIN_X | JointAxesMask::LIN_Y | JointAxesMask::LIN_Z;
    let axes = [
        (JointAxis::AngX, JointAxesMask::ANG_X, def.limits.x),
        (JointAxis::AngY, JointAxesMask::ANG_Y, def.limits.y),
        (JointAxis::AngZ, JointAxesMask::ANG_Z, def.limits.z),
    ];
    for (_, mask, limit) in axes {
        if limit.is_none() {
            locked |= mask;
        }
    }

    let mut builder = GenericJointBuilder::new(locked)
        .local_anchor1(anchor_on_parent)
        .local_anchor2(anchor_on_child);
    for (axis, _, limit) in axes {
        if let Some(range) = limit {
            builder = builder
                .limits(axis, range)
                // "Force based" = motor settings are in real units (N·m).
                .motor_model(axis, MotorModel::ForceBased)
                // A motor that only resists motion (pure damping) and is weak
                // (max torque = friction) behaves like friction. `muscles.rs`
                // replaces this with real muscle settings every frame.
                .set_motor(axis, 0.0, 0.0, 0.0, LIMP_DAMPING)
                .motor_max_force(axis, def.friction);
        }
    }
    let mut joint = builder.build();
    // Parts joined together overlap slightly at the joint. Don't let them
    // collide with each other, or they'd push apart forever.
    joint.set_contacts_enabled(false);
    joint
}

fn part_color(part: BodyPart) -> Color {
    use BodyPart::*;
    match part {
        Head => Color::srgb(0.87, 0.70, 0.56),
        Chest | Belly | UpperArmL | UpperArmR => Color::srgb(0.20, 0.38, 0.65),
        ForearmL | ForearmR => Color::srgb(0.87, 0.70, 0.56),
        Pelvis | ThighL | ThighR | ShinL | ShinR => Color::srgb(0.22, 0.22, 0.26),
        FootL | FootR => Color::srgb(0.12, 0.10, 0.08),
    }
}

/// Spawn a complete person. Returns the pelvis entity.
pub fn spawn_ragdoll(commands: &mut Commands, position: Vec3, yaw: f32) -> Entity {
    let rotation = Quat::from_rotation_y(yaw);
    let defs = part_defs();
    let mut entities = std::collections::HashMap::new();

    for def in &defs {
        let entity = commands
            .spawn((
                Name::new(format!("{:?}", def.part)),
                RagdollPart { part: def.part, mass: def.mass },
                def.shape,
                Paint(part_color(def.part)),
                Transform::from_translation(position + rotation * def.center).with_rotation(rotation),
                // Physics components, grouped because Bevy limits how many fit in one tuple.
                (
                RigidBody::Dynamic,
                def.shape.collider(),
                // Mass is set directly instead of from density, so the body
                // weighs a realistic ~82 kg regardless of shape sizes.
                ColliderMassProperties::Mass(def.mass),
                layers::ragdoll(),
                // Shoe soles grip; clothes slide a little.
                Friction::coefficient(if matches!(def.part, BodyPart::FootL | BodyPart::FootR) { 1.5 } else { 0.8 }),
                Restitution::coefficient(0.0),
                // A little air resistance on spinning, so a limp body settles.
                Damping { linear_damping: 0.05, angular_damping: 0.5 },
                Velocity::default(),
                ExternalImpulse::default(),
                // Rapier normally "puts to sleep" bodies that stop moving, to
                // save work. Muscles change from frame to frame, so stay awake.
                Sleeping::disabled(),
                // Smooth rendering between physics steps (see `physics.rs`).
                TransformInterpolation::default(),
                ),
            ))
            .id();
        entities.insert(def.part, entity);
    }

    let center_of = |part: BodyPart| defs.iter().find(|d| d.part == part).unwrap().center;
    for def in joint_defs() {
        let parent = entities[&def.parent];
        let anchor_on_parent = def.anchor - center_of(def.parent);
        let anchor_on_child = def.anchor - center_of(def.child);
        let joint = build_joint(&def, anchor_on_parent, anchor_on_child);
        commands.entity(entities[&def.child]).insert((
            ImpulseJoint::new(parent, TypedJoint::GenericJoint(joint)),
            RagdollJoint { parent, anchor_on_parent, anchor_on_child, friction: def.friction },
            JointTarget(standing_pose(def.child)),
        ));
    }

    entities[&BodyPart::Pelvis]
}

pub struct RagdollPlugin;

impl Plugin for RagdollPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<SpawnRagdoll>()
            .add_systems(Startup, |mut spawn: MessageWriter<SpawnRagdoll>| {
                spawn.write(SpawnRagdoll::START);
            })
            .add_systems(Update, respawn_ragdoll);
    }
}

/// There is only ever one person: spawning a new one removes the old one.
fn respawn_ragdoll(
    mut commands: Commands,
    mut requests: MessageReader<SpawnRagdoll>,
    existing: Query<Entity, With<RagdollPart>>,
) {
    let Some(request) = requests.read().last().copied() else {
        return;
    };
    for entity in &existing {
        commands.entity(entity).despawn();
    }
    spawn_ragdoll(&mut commands, request.position, request.yaw);
}
