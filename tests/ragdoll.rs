//! Headless physics checks: run the simulation without a window and assert
//! that the body behaves sensibly. Run with `cargo test`.
//!
//! Diagnostics that print what the body is doing over time are marked
//! `#[ignore]`; run them with `cargo test -- --ignored --nocapture`.

use std::time::Duration;

use bevy::prelude::*;
use bevy::time::TimeUpdateStrategy;
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
    // The game clock (used by timers, e.g. how long since landing) also
    // advances by exactly one physics step per update, not by wall-clock time.
    app.insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f32(PHYSICS_DT)));
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
// Stage 3a: balance and fall reactions
// ---------------------------------------------------------------------------

fn balance_state(app: &mut App) -> ragdoll_sandbox::balance::BalanceState {
    let mut q = app.world_mut().query::<&ragdoll_sandbox::balance::Balance>();
    q.single(app.world()).unwrap().state
}

#[test]
fn small_shoves_are_absorbed_in_every_direction() {
    for (name, f, l) in [("forward", 1.0, 0.0), ("back", -1.0, 0.0), ("left", 0.0, 1.0), ("right", 0.0, -1.0)] {
        assert!(survives_shove(true, f, l, 20.0), "a 20 N·s shove {name} knocked the person over");
    }
}

#[test]
fn balancing_beats_standing_like_a_statue() {
    // Measured: a 20 N·s shove from the front topples the Stage 2 statue
    // (muscles only) but not a balancing person.
    assert!(!survives_shove(false, -1.0, 0.0, 20.0), "expected the statue to fall");
    assert!(survives_shove(true, -1.0, 0.0, 20.0), "expected the balancing person to stay up");
}

#[test]
fn a_big_shove_makes_them_fall_brace_and_relax() {
    use ragdoll_sandbox::balance::BalanceState;
    use ragdoll_sandbox::muscles::ToneScale;
    let mut app = active();
    run_seconds(&mut app, 1.0);
    let (_, pelvis, _) = part(&mut app, BodyPart::Pelvis);
    let fwd = pelvis.rotation * Vec3::Z;
    let (chest, _, _) = part(&mut app, BodyPart::Chest);
    app.world_mut().get_mut::<ExternalImpulse>(chest).unwrap().impulse = fwd * 120.0;

    let mut saw_falling = false;
    let mut best_reach = f32::MIN;
    for _ in 0..30 {
        run_seconds(&mut app, 0.1);
        if let BalanceState::Falling { .. } = balance_state(&mut app) {
            saw_falling = true;
            let (_, chest_t, _) = part(&mut app, BodyPart::Chest);
            let (_, hl, _) = part(&mut app, BodyPart::ForearmL);
            let (_, hr, _) = part(&mut app, BodyPart::ForearmR);
            let reach = ((hl.translation + hr.translation) / 2.0 - chest_t.translation).dot(fwd);
            best_reach = best_reach.max(reach);
        }
    }
    assert!(saw_falling, "never entered the falling state");
    assert!(best_reach > 0.2, "hands never reached out ahead of the chest (best {best_reach:.2} m)");
    assert_eq!(balance_state(&mut app), BalanceState::Down);
    let tone = app.world().resource::<ToneScale>().0;
    assert!(tone < 0.2, "still tensed on the ground (tone scale {tone:.2})");
}

#[test]
fn balance_works_when_the_controller_only_runs_60_times_a_second() {
    // In the game the controller runs once per rendered frame. At 60 fps that
    // is one controller update per two physics steps. Same physics slices
    // (1/480 s), half the controller rate:
    let mut app = active();
    app.insert_resource(TimestepMode::Fixed { dt: 1.0 / 60.0, substeps: SUBSTEPS * 2 });
    app.insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f32(1.0 / 60.0)));
    for _ in 0..(60 * 10) {
        app.update();
    }
    let (_, pelvis, _) = part(&mut app, BodyPart::Pelvis);
    assert!(pelvis.translation.y > 0.9, "fell over at a 60 Hz control rate");
    let (chest, _, _) = part(&mut app, BodyPart::Chest);
    app.world_mut().get_mut::<ExternalImpulse>(chest).unwrap().impulse = pelvis.rotation * Vec3::Z * 20.0;
    for _ in 0..(60 * 4) {
        app.update();
    }
    let (_, pelvis, _) = part(&mut app, BodyPart::Pelvis);
    assert!(pelvis.translation.y > 0.8, "a small shove toppled them at a 60 Hz control rate");
}

#[test]
fn one_pistol_shot_rocks_but_does_not_drop_them() {
    use ragdoll_sandbox::weapons::PISTOL_IMPULSE;
    // Shots as fired in the game: from the over-the-shoulder camera at the
    // start position, at the chest, centre and off-centre (which also twists).
    for target in [Vec3::new(0.0, 1.35, -0.1), Vec3::new(0.16, 1.33, -0.16), Vec3::new(-0.16, 1.33, -0.16)] {
        let mut app = active();
        run_seconds(&mut app, 1.0);
        shoot_like_game(&mut app, Vec3::new(-0.9, 1.8, -9.5), target, BodyPart::Chest, PISTOL_IMPULSE);
        run_seconds(&mut app, 4.0);
        let (_, pelvis, _) = part(&mut app, BodyPart::Pelvis);
        assert!(pelvis.translation.y > 0.8, "one pistol shot at {target} dropped them");
    }
}

#[test]
fn a_shotgun_blast_drops_them() {
    let mut app = active();
    run_seconds(&mut app, 1.0);
    for _ in 0..9 {
        shoot_like_game(&mut app, Vec3::new(-0.9, 1.8, -9.5), Vec3::new(0.0, 1.3, -0.1), BodyPart::Chest, 10.0);
    }
    run_seconds(&mut app, 4.0);
    let (_, pelvis, _) = part(&mut app, BodyPart::Pelvis);
    assert!(pelvis.translation.y < 0.5, "still standing after a point-blank shotgun blast");
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
        let (_, fl, flv) = part(&mut app, BodyPart::FootL);
        let (_, fr, _) = part(&mut app, BodyPart::FootR);
        println!(
            "t={:.1}s pelvis y={:.3} COM=({:.3},{:.3}) head speed={:.4} ankleL={ankle:.3} kneeL={knee:.3} footL=({:.4},{:.4}) footR=({:.4},{:.4}) footL v={:.4}",
            (step + 1) as f32 * 0.5, pelvis.translation.y, com.x, com.z, head_v.linear.length(),
            fl.translation.x, fl.translation.z, fr.translation.x, fr.translation.z, flv.linear.length()
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

/// Push the chest horizontally, `forward`/`left` in the person's own frame,
/// with `impulse` N·s, then report whether they're still up after 4 s.
fn survives_shove(balance: bool, forward: f32, left: f32, impulse: f32) -> bool {
    survives_shove_tuned(balance, None, forward, left, impulse)
}

fn survives_shove_tuned(
    balance: bool,
    tuning: Option<ragdoll_sandbox::balance::BalanceTuning>,
    forward: f32,
    left: f32,
    impulse: f32,
) -> bool {
    let mut app = active();
    app.insert_resource(ragdoll_sandbox::balance::BalanceEnabled(balance));
    if let Some(tuning) = tuning {
        app.insert_resource(tuning);
    }
    run_seconds(&mut app, 1.0);
    let (_, pelvis, _) = part(&mut app, BodyPart::Pelvis);
    let fwd = pelvis.rotation * Vec3::Z;
    let lft = pelvis.rotation * Vec3::X;
    let (chest, _, _) = part(&mut app, BodyPart::Chest);
    app.world_mut().get_mut::<ExternalImpulse>(chest).unwrap().impulse = (fwd * forward + lft * left) * impulse;
    run_seconds(&mut app, 4.0);
    let (_, pelvis, _) = part(&mut app, BodyPart::Pelvis);
    pelvis.translation.y > 0.8
}

#[test]
#[ignore = "diagnostic"]
fn print_shove_matrix() {
    let impulses = [20.0, 40.0, 60.0, 80.0, 110.0, 150.0, 200.0];
    for balance in [false, true] {
        for (name, f, l) in [("forward", 1.0, 0.0), ("back", -1.0, 0.0), ("left", 0.0, 1.0), ("right", 0.0, -1.0)] {
            let row: Vec<String> = impulses
                .iter()
                .map(|&j| if survives_shove(balance, f, l, j) { format!("{j:>4}:ok ") } else { format!("{j:>4}:FALL") })
                .collect();
            println!("balance={balance:<5} {name:<8} {}", row.join(" "));
        }
    }
}

#[test]
#[ignore = "diagnostic"]
fn print_shove_timeline() {
    use ragdoll_sandbox::balance::Balance;
    let j: f32 = std::env::var("J").ok().and_then(|s| s.parse().ok()).unwrap_or(40.0);
    let dir: f32 = std::env::var("DIR").ok().and_then(|s| s.parse().ok()).unwrap_or(1.0);
    let mut app = active();
    run_seconds(&mut app, 1.0);
    let (chest, _, _) = part(&mut app, BodyPart::Chest);
    let (_, pelvis, _) = part(&mut app, BodyPart::Pelvis);
    let fwd = pelvis.rotation * Vec3::Z;
    app.world_mut().get_mut::<ExternalImpulse>(chest).unwrap().impulse = fwd * dir * j;
    for i in 0..30 {
        run_seconds(&mut app, 0.05);
        let (_, pelvis, _) = part(&mut app, BodyPart::Pelvis);
        let ankle = joint_angle(&mut app, BodyPart::ShinL, BodyPart::FootL, 0);
        let hip = joint_angle(&mut app, BodyPart::Pelvis, BodyPart::ThighL, 0);
        let (_, foot, _) = part(&mut app, BodyPart::FootL);
        let mut q = app.world_mut().query::<&Balance>();
        let b = q.single(app.world()).unwrap();
        let err = (b.capture_point - b.support_center) * Vec3::new(1.0, 0.0, 1.0);
        let fwd_err = err.dot(fwd);
        let com_fwd = (b.com - b.support_center).dot(fwd);
        let toe_lift = (foot.rotation * Vec3::Y).dot(fwd);
        println!(
            "t={:.2} {:?} cp_err={fwd_err:+.3} com_fwd={com_fwd:+.3} v={:+.3} ankle={ankle:+.3} hip={hip:+.3} footTilt={toe_lift:+.3} pelvisY={:.2}",
            (i + 1) as f32 * 0.05,
            match b.state { ragdoll_sandbox::balance::BalanceState::Standing => "STAND", ragdoll_sandbox::balance::BalanceState::Falling { .. } => "FALL ", _ => "DOWN " },
            b.com_velocity.dot(fwd), pelvis.translation.y
        );
    }
}

/// Largest impulse (from a fixed ladder) survived in a direction.
fn max_survived(tuning: ragdoll_sandbox::balance::BalanceTuning, forward: f32, left: f32) -> f32 {
    let mut best = 0.0;
    for j in [10.0, 15.0, 20.0, 25.0, 30.0, 35.0, 40.0, 50.0] {
        if survives_shove_tuned(true, Some(tuning), forward, left, j) {
            best = j;
        } else {
            break;
        }
    }
    best
}

#[test]
#[ignore = "diagnostic"]
fn print_tuning_sweep() {
    use ragdoll_sandbox::balance::BalanceTuning;
    for ankle in [1.0, 2.5, 4.0] {
        for max in [0.12, 0.2, 0.35] {
            for hip in [-1.5, 0.0, 1.5] {
                let t = BalanceTuning { ankle_gain_forward: ankle, ankle_gain_sideways: ankle, hip_gain: hip, max_ankle_correction: max, lean: 0.0 };
                let f = max_survived(t, 1.0, 0.0);
                let b = max_survived(t, -1.0, 0.0);
                let l = max_survived(t, 0.0, 1.0);
                println!("ankle={ankle:.1} max={max:.2} hip={hip:+.1}: fwd={f:>3} back={b:>3} side={l:>3}");
            }
        }
    }
}

#[test]
#[ignore = "diagnostic"]
fn print_fall_reaction() {
    use ragdoll_sandbox::balance::{Balance, BalanceState};
    for (name, f, l) in [("forward", 1.0, 0.0), ("back", -1.0, 0.0), ("left", 0.0, 1.0)] {
        let mut app = active();
        run_seconds(&mut app, 1.0);
        let (_, pelvis, _) = part(&mut app, BodyPart::Pelvis);
        let fwd = pelvis.rotation * Vec3::Z;
        let lft = pelvis.rotation * Vec3::X;
        let dir = fwd * f + lft * l;
        let (chest, _, _) = part(&mut app, BodyPart::Chest);
        app.world_mut().get_mut::<ExternalImpulse>(chest).unwrap().impulse = dir * 120.0;
        for i in 0..12 {
            run_seconds(&mut app, 0.1);
            let (_, chest_t, _) = part(&mut app, BodyPart::Chest);
            let (_, hand_l, _) = part(&mut app, BodyPart::ForearmL);
            let (_, hand_r, _) = part(&mut app, BodyPart::ForearmR);
            let hands = (hand_l.translation + hand_r.translation) / 2.0 - chest_t.translation;
            let mut q = app.world_mut().query::<&Balance>();
            let b = q.single(app.world()).unwrap();
            let state = match b.state { BalanceState::Standing => "STAND", BalanceState::Falling { .. } => "FALL", BalanceState::Down => "DOWN" };
            println!(
                "{name:<7} t={:.1} {state:<5} hands vs chest: along fall {:+.2} up {:+.2} | chest y {:.2}",
                (i + 1) as f32 * 0.1, hands.dot(dir), hands.y, chest_t.translation.y
            );
        }
    }
}

#[test]
#[ignore = "diagnostic"]
fn print_protective_pose_static() {
    use ragdoll_sandbox::balance::{BalanceEnabled, protective_pose};
    use ragdoll_sandbox::muscles::JointTarget;
    for (name, f, l) in [("forward", 1.0, 0.0), ("back", -1.0, 0.0), ("left", 0.0, 1.0)] {
        let mut app = active();
        app.insert_resource(BalanceEnabled(false));
        run_seconds(&mut app, 0.5);
        // Hold only the arm part of the pose, legs stay standing.
        app.world_mut().remove_resource::<BalanceEnabled>();
        app.insert_resource(BalanceEnabled(false));
        let mut q = app.world_mut().query::<(&RagdollPart, &mut JointTarget)>();
        for (p, mut t) in q.iter_mut(app.world_mut()) {
            if matches!(p.part, BodyPart::UpperArmL | BodyPart::UpperArmR | BodyPart::ForearmL | BodyPart::ForearmR) {
                t.0 = protective_pose(p.part, f, l);
            }
        }
        // Balance disabled resets targets every frame; so step manually.
        for step in [0.1, 0.2, 0.4, 0.8] {
            run_seconds_keep(&mut app, step, f, l);
            let (_, pelvis, _) = part(&mut app, BodyPart::Pelvis);
            let (_, chest, _) = part(&mut app, BodyPart::Chest);
            let (_, hl, _) = part(&mut app, BodyPart::ForearmL);
            let (_, hr, _) = part(&mut app, BodyPart::ForearmR);
            let inv = pelvis.rotation.inverse();
            let rl = inv * (hl.translation - chest.translation);
            let rr = inv * (hr.translation - chest.translation);
            println!("{name:<7} after {step:.1}s: left hand (x={:+.2} y={:+.2} fwd={:+.2}) right hand (x={:+.2} y={:+.2} fwd={:+.2})", rl.x, rl.y, rl.z, rr.x, rr.y, rr.z);
        }
    }
}

/// Run while re-applying the arm part of the protective pose each frame.
fn run_seconds_keep(app: &mut App, seconds: f32, f: f32, l: f32) {
    use ragdoll_sandbox::balance::protective_pose;
    use ragdoll_sandbox::muscles::JointTarget;
    for _ in 0..(seconds / PHYSICS_DT).round() as usize {
        app.update();
        let mut q = app.world_mut().query::<(&RagdollPart, &mut JointTarget)>();
        for (p, mut t) in q.iter_mut(app.world_mut()) {
            if matches!(p.part, BodyPart::UpperArmL | BodyPart::UpperArmR | BodyPart::ForearmL | BodyPart::ForearmR) {
                t.0 = protective_pose(p.part, f, l);
            }
        }
    }
}

/// Fire one shot like the game does: from `camera` toward `target` (world
/// space), hitting `body_part` at `target`.
fn shoot_like_game(app: &mut App, camera: Vec3, target: Vec3, which: BodyPart, strength: f32) {
    let (entity, transform, _) = part(app, which);
    let dir = (target - camera).normalize();
    let mut impulse = app.world_mut().get_mut::<ExternalImpulse>(entity).unwrap();
    ragdoll_sandbox::weapons::apply_hit(&mut impulse, transform.translation, target, dir * strength);
}

#[test]
#[ignore = "diagnostic"]
fn print_game_shot_and_control_rate() {
    // 1. The exact shot from the screenshot session, at the normal rate.
    for strength in [15.0, 18.0, 22.0] {
        let mut app = active();
        run_seconds(&mut app, 1.0);
        shoot_like_game(&mut app, Vec3::new(-0.9, 1.8, -9.5), Vec3::new(0.16, 1.33, -0.16), BodyPart::Chest, strength);
        run_seconds(&mut app, 4.0);
        let (_, pelvis, _) = part(&mut app, BodyPart::Pelvis);
        println!("game-like shot {strength} N·s at 120 Hz control: {}", if pelvis.translation.y > 0.8 { "stands" } else { "FALLS" });
    }
    // 2. Same 20 N·s backward shove at lower controller rates.
    for hz in [60.0f32, 30.0, 15.0] {
        let mut app = active();
        let substeps = (480.0 / hz).round() as usize;
        app.insert_resource(TimestepMode::Fixed { dt: 1.0 / hz, substeps });
        app.insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f32(1.0 / hz)));
        for _ in 0..hz as usize {
            app.update();
        }
        let (_, pelvis, _) = part(&mut app, BodyPart::Pelvis);
        let (chest, _, _) = part(&mut app, BodyPart::Chest);
        app.world_mut().get_mut::<ExternalImpulse>(chest).unwrap().impulse = pelvis.rotation * Vec3::NEG_Z * 20.0;
        for _ in 0..(hz * 4.0) as usize {
            app.update();
        }
        let (_, pelvis, _) = part(&mut app, BodyPart::Pelvis);
        println!("20 N·s backward shove, controller at {hz} Hz: {}", if pelvis.translation.y > 0.8 { "stands" } else { "FALLS" });
    }
}

#[test]
#[ignore = "diagnostic"]
fn print_lean_sweep() {
    use ragdoll_sandbox::balance::BalanceTuning;
    for lean in [0.0, -0.02, -0.04, -0.06, -0.08] {
        let t = BalanceTuning { lean, ..default() };
        let f = max_survived(t, 1.0, 0.0);
        let b = max_survived(t, -1.0, 0.0);
        let l = max_survived(t, 0.0, 1.0);
        println!("lean={lean:+.2}: fwd={f:>3} back={b:>3} side={l:>3}");
    }
}

#[test]
#[ignore = "diagnostic: EXP=coulomb,fbias,wsj"]
fn print_creep_experiment() {
    use bevy_rapier3d::rapier::dynamics::FrictionModel;
    let exp = std::env::var("EXP").unwrap_or_default();
    let mut app = active();
    app.update();
    {
        let mut q = app.world_mut().query::<&mut RapierContextSimulation>();
        for mut sim in q.iter_mut(app.world_mut()) {
            let p = &mut sim.integration_parameters;
            for e in exp.split(',') {
                match e {
                    "coulomb" => p.friction_model = FrictionModel::Coulomb,
                    "fbias" => p.friction_in_bias_pass = true,
                    "wsj" => p.warmstart_joints = true,
                    _ => {}
                }
            }
        }
    }
    run_seconds(&mut app, 2.0);
    let (_, a, _) = part(&mut app, BodyPart::FootL);
    run_seconds(&mut app, 10.0);
    let (_, b, _) = part(&mut app, BodyPart::FootL);
    let (_, pelvis, _) = part(&mut app, BodyPart::Pelvis);
    println!("EXP={exp:<20} foot slid {:.1} mm in 10 s, pelvis y {:.2}", (b.translation - a.translation).length() * 1000.0, pelvis.translation.y);
}
