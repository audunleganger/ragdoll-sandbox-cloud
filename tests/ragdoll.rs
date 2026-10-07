//! Headless physics checks: run the simulation without a window and assert
//! that the body behaves sensibly. Run with `cargo test`.
//!
//! Diagnostics that print what the body is doing over time are marked
//! `#[ignore]`; run them with `cargo test -- --ignored --nocapture`.

use bevy::prelude::*;
use bevy_rapier3d::prelude::*;
use ragdoll_sandbox::CorePlugin;
use ragdoll_sandbox::muscles::MuscleTone;
use ragdoll_sandbox::physics::{PHYSICS_DT, SUBSTEPS};
use ragdoll_sandbox::ragdoll::{BodyPart, RagdollJoint, RagdollPart};

fn sandbox(tone: f32) -> App {
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, TransformPlugin, CorePlugin));
    // One `app.update()` = exactly one physics step, the same step the game
    // uses, regardless of how fast the test machine is.
    app.insert_resource(TimestepMode::Fixed { dt: PHYSICS_DT, substeps: SUBSTEPS });
    app.insert_resource(MuscleTone(tone));
    app.finish();
    app.cleanup();
    app
}

/// A person with muscles switched off.
fn limp() -> App {
    sandbox(0.0)
}

/// A person with full muscle tone.
fn active() -> App {
    sandbox(1.0)
}

fn run_seconds(app: &mut App, seconds: f32) {
    for _ in 0..(seconds / PHYSICS_DT).round() as usize {
        app.update();
    }
}

fn part(app: &mut App, which: BodyPart) -> (Entity, Transform, Velocity) {
    let mut q = app.world_mut().query::<(Entity, &RagdollPart, &Transform, &Velocity)>();
    q.iter(app.world())
        .find(|(_, p, _, _)| p.part == which)
        .map(|(e, _, t, v)| (e, *t, *v))
        .expect("body part exists")
}

/// Angle around a local axis (0 = x, 1 = y, 2 = z) of `child` relative to
/// `parent`, measured the same way Rapier measures joint limits and motors.
fn joint_angle(app: &mut App, parent: BodyPart, child: BodyPart, axis: usize) -> f32 {
    let (_, p, _) = part(app, parent);
    let (_, c, _) = part(app, child);
    let mut q = p.rotation.inverse() * c.rotation;
    if q.w < 0.0 {
        q = -q;
    }
    2.0 * [q.x, q.y, q.z][axis].clamp(-1.0, 1.0).asin()
}

/// Whole-body centre of mass, in world coordinates.
fn center_of_mass(app: &mut App) -> Vec3 {
    let mut q = app.world_mut().query::<(&RagdollPart, &Transform)>();
    let (sum, mass) = q
        .iter(app.world())
        .fold((Vec3::ZERO, 0.0), |(s, m), (p, t)| (s + t.translation * p.mass, m + p.mass));
    sum / mass
}

// ---------------------------------------------------------------------------
// Stage 1: the skeleton
// ---------------------------------------------------------------------------

#[test]
fn limp_body_collapses_and_settles() {
    let mut app = limp();
    run_seconds(&mut app, 6.0);

    let (_, pelvis, _) = part(&mut app, BodyPart::Pelvis);
    assert!(pelvis.translation.y < 0.5, "a limp body should end up on the ground, pelvis at {}", pelvis.translation.y);

    let mut q = app.world_mut().query::<(&RagdollPart, &Transform, &Velocity)>();
    for (p, t, v) in q.iter(app.world()) {
        assert!(t.translation.y > 0.0, "{:?} sank into the ground: y = {}", p.part, t.translation.y);
        assert!(v.linear.length() < 0.3, "{:?} still moving at {} m/s", p.part, v.linear.length());
    }
}

#[test]
fn joints_stay_connected() {
    let mut app = limp();
    run_seconds(&mut app, 3.0);

    let mut q = app.world_mut().query::<(&RagdollPart, &Transform, &RagdollJoint)>();
    let joints: Vec<_> = q.iter(app.world()).map(|(p, t, j)| (p.part, *t, *j)).collect();
    for (child, child_t, joint) in joints {
        let parent_t = *app.world().get::<Transform>(joint.parent).unwrap();
        let on_parent = parent_t.transform_point(joint.anchor_on_parent);
        let on_child = child_t.transform_point(joint.anchor_on_child);
        let gap = on_parent.distance(on_child);
        assert!(gap < 0.03, "{child:?} came {gap:.3} m apart from its parent");
    }
}

#[test]
fn knees_and_elbows_bend_the_right_way() {
    let mut app = limp();
    run_seconds(&mut app, 4.0);
    let tolerance = 0.15;
    for (thigh, shin) in [(BodyPart::ThighL, BodyPart::ShinL), (BodyPart::ThighR, BodyPart::ShinR)] {
        let a = joint_angle(&mut app, thigh, shin, 0);
        assert!((-tolerance..2.4 + tolerance).contains(&a), "{shin:?} knee angle {a:.2} outside [0, 2.4]");
    }
    for (upper, fore) in [(BodyPart::UpperArmL, BodyPart::ForearmL), (BodyPart::UpperArmR, BodyPart::ForearmR)] {
        let a = joint_angle(&mut app, upper, fore, 0);
        assert!((-2.5 - tolerance..tolerance).contains(&a), "{fore:?} elbow angle {a:.2} outside [-2.5, 0]");
    }
}

#[test]
fn a_shot_pushes_the_body() {
    let mut app = active();
    run_seconds(&mut app, 0.1);
    let (chest, _, _) = part(&mut app, BodyPart::Chest);
    // Same impulse as one pistol shot.
    app.world_mut().get_mut::<ExternalImpulse>(chest).unwrap().impulse = Vec3::new(0.0, 0.0, -40.0);
    run_seconds(&mut app, PHYSICS_DT * 2.0);
    let (_, _, velocity) = part(&mut app, BodyPart::Chest);
    assert!(velocity.linear.z < -0.5, "chest should be knocked backward, velocity {:?}", velocity.linear);
}

// ---------------------------------------------------------------------------
// Stage 2: muscles
// ---------------------------------------------------------------------------

#[test]
fn muscles_hold_the_body_upright() {
    let mut app = active();
    let mut lowest = f32::MAX;
    for _ in 0..80 {
        run_seconds(&mut app, 0.25);
        let (_, pelvis, _) = part(&mut app, BodyPart::Pelvis);
        lowest = lowest.min(pelvis.translation.y);
    }
    let (_, head, head_v) = part(&mut app, BodyPart::Head);
    let com = center_of_mass(&mut app);
    assert!(lowest > 0.9, "pelvis dropped to {lowest:.2} m within 20 s");
    assert!(head.translation.y > 1.55, "head at {:.2} m, not upright", head.translation.y);
    assert!(head_v.linear.length() < 0.05, "still swaying at {:.3} m/s after 20 s", head_v.linear.length());
    assert!(Vec2::new(com.x, com.z).length() < 0.1, "drifted {:.2} m", Vec2::new(com.x, com.z).length());
}

#[test]
fn turning_muscles_off_drops_the_body() {
    let mut app = active();
    run_seconds(&mut app, 1.0);
    app.insert_resource(MuscleTone(0.0));
    run_seconds(&mut app, 3.0);
    let (_, pelvis, _) = part(&mut app, BodyPart::Pelvis);
    assert!(pelvis.translation.y < 0.5, "pelvis still at {:.2} m after going limp", pelvis.translation.y);
}

#[test]
fn the_game_uses_the_tested_time_step() {
    match ragdoll_sandbox::physics::game_timestep() {
        TimestepMode::Interpolated { dt, substeps, time_scale } => {
            assert_eq!((dt, substeps, time_scale), (PHYSICS_DT, SUBSTEPS, 1.0));
        }
        other => panic!("game uses {other:?}; tests assume a fixed step"),
    }
}

// ---------------------------------------------------------------------------
// Diagnostics
// ---------------------------------------------------------------------------

#[test]
#[ignore = "diagnostic"]
fn print_fall() {
    let mut app = limp();
    for step in 0..8 {
        run_seconds(&mut app, 0.5);
        let (_, pelvis, _) = part(&mut app, BodyPart::Pelvis);
        let (_, head, _) = part(&mut app, BodyPart::Head);
        let knee = joint_angle(&mut app, BodyPart::ThighL, BodyPart::ShinL, 0);
        let hip = joint_angle(&mut app, BodyPart::Pelvis, BodyPart::ThighL, 0);
        let elbow = joint_angle(&mut app, BodyPart::UpperArmL, BodyPart::ForearmL, 0);
        println!(
            "t={:.1}s pelvis y={:.2} head=({:.2},{:.2},{:.2}) kneeL={knee:.2} hipL={hip:.2} elbowL={elbow:.2}",
            (step + 1) as f32 * 0.5, pelvis.translation.y, head.translation.x, head.translation.y, head.translation.z
        );
    }
}

#[test]
#[ignore = "diagnostic"]
fn print_stand() {
    let mut app = active();
    for step in 0..20 {
        run_seconds(&mut app, 0.5);
        let (_, pelvis, _) = part(&mut app, BodyPart::Pelvis);
        let (_, _, head_v) = part(&mut app, BodyPart::Head);
        let com = center_of_mass(&mut app);
        let ankle = joint_angle(&mut app, BodyPart::ShinL, BodyPart::FootL, 0);
        let knee = joint_angle(&mut app, BodyPart::ThighL, BodyPart::ShinL, 0);
        println!(
            "t={:.1}s pelvis y={:.3} COM=({:.3},{:.3}) head speed={:.4} ankleL={ankle:.3} kneeL={knee:.3}",
            (step + 1) as f32 * 0.5, pelvis.translation.y, com.x, com.z, head_v.linear.length()
        );
    }
}

#[test]
#[ignore = "diagnostic"]
fn print_limp_speeds() {
    let mut app = limp();
    for step in 0..20 {
        run_seconds(&mut app, 0.5);
        let mut q = app.world_mut().query::<(&RagdollPart, &Transform, &Velocity)>();
        let worst = q
            .iter(app.world())
            .map(|(p, t, v)| (v.linear.length(), p.part, t.translation.y))
            .fold((0.0, BodyPart::Pelvis, 0.0), |a, b| if b.0 > a.0 { b } else { a });
        println!("t={:.1} fastest part {:?} at {:.3} m/s (y={:.3})", (step + 1) as f32 * 0.5, worst.1, worst.0, worst.2);
    }
}

#[test]
#[ignore = "diagnostic"]
fn print_tone_sweep() {
    for tone in [0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.8] {
        let mut app = sandbox(tone);
        run_seconds(&mut app, 5.0);
        let (_, pelvis, _) = part(&mut app, BodyPart::Pelvis);
        let (_, head, _) = part(&mut app, BodyPart::Head);
        let knee = joint_angle(&mut app, BodyPart::ThighL, BodyPart::ShinL, 0);
        println!("tone={tone:.1}: pelvis y={:.2} head y={:.2} knee={knee:.2}", pelvis.translation.y, head.translation.y);
    }
}
