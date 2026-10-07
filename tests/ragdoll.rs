//! Headless physics checks: run the simulation without a window and assert
//! that the body behaves sensibly. Run with `cargo test`.

use bevy::prelude::*;
use bevy_rapier3d::prelude::*;
use ragdoll_sandbox::CorePlugin;
use ragdoll_sandbox::ragdoll::{BodyPart, RagdollJoint, RagdollPart};

const FPS: f32 = 60.0;

fn sandbox() -> App {
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, TransformPlugin, CorePlugin));
    // Tests advance physics by exactly 1/60 s per update, regardless of how
    // fast the test machine is.
    app.insert_resource(TimestepMode::Fixed { dt: 1.0 / FPS, substeps: ragdoll_sandbox::physics::SUBSTEPS });
    app.finish();
    app.cleanup();
    app
}

fn run_seconds(app: &mut App, seconds: f32) {
    for _ in 0..(seconds * FPS) as usize {
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

/// Bend angle around the sideways (X) axis of `child` relative to `parent`,
/// measured the same way Rapier measures joint limits.
fn bend_angle(app: &mut App, parent: BodyPart, child: BodyPart) -> f32 {
    let (_, p, _) = part(app, parent);
    let (_, c, _) = part(app, child);
    let relative = p.rotation.inverse() * c.rotation;
    2.0 * relative.x.clamp(-1.0, 1.0).asin() * relative.w.signum()
}

#[test]
fn limp_body_collapses_and_settles() {
    let mut app = sandbox();
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
    let mut app = sandbox();
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
    let mut app = sandbox();
    run_seconds(&mut app, 4.0);
    let tolerance = 0.15;
    for (thigh, shin) in [(BodyPart::ThighL, BodyPart::ShinL), (BodyPart::ThighR, BodyPart::ShinR)] {
        let a = bend_angle(&mut app, thigh, shin);
        assert!((-tolerance..2.4 + tolerance).contains(&a), "{shin:?} knee angle {a:.2} outside [0, 2.4]");
    }
    for (upper, fore) in [(BodyPart::UpperArmL, BodyPart::ForearmL), (BodyPart::UpperArmR, BodyPart::ForearmR)] {
        let a = bend_angle(&mut app, upper, fore);
        assert!((-2.5 - tolerance..tolerance).contains(&a), "{fore:?} elbow angle {a:.2} outside [-2.5, 0]");
    }
}

#[test]
fn a_shot_pushes_the_body() {
    let mut app = sandbox();
    run_seconds(&mut app, 0.1);
    let (chest, _, _) = part(&mut app, BodyPart::Chest);
    // Same impulse as one pistol shot.
    app.world_mut().get_mut::<ExternalImpulse>(chest).unwrap().impulse = Vec3::new(0.0, 0.0, -40.0);
    run_seconds(&mut app, 1.0 / FPS * 2.0);
    let (_, _, velocity) = part(&mut app, BodyPart::Chest);
    assert!(velocity.linear.z < -0.5, "chest should be knocked backward, velocity {:?}", velocity.linear);
}

#[test]
#[ignore = "diagnostic: run with --ignored --nocapture"]
fn print_fall() {
    let mut app = sandbox();
    for step in 0..8 {
        run_seconds(&mut app, 0.5);
        let (_, pelvis, _) = part(&mut app, BodyPart::Pelvis);
        let (_, head, _) = part(&mut app, BodyPart::Head);
        let kl = bend_angle(&mut app, BodyPart::ThighL, BodyPart::ShinL);
        let kr = bend_angle(&mut app, BodyPart::ThighR, BodyPart::ShinR);
        let el = bend_angle(&mut app, BodyPart::UpperArmL, BodyPart::ForearmL);
        let hip = bend_angle(&mut app, BodyPart::Pelvis, BodyPart::ThighL);
        println!(
            "t={:.1}s pelvis y={:.2} head=({:.2},{:.2},{:.2}) knees L={kl:.2} R={kr:.2} elbowL={el:.2} hipL={hip:.2}",
            (step + 1) as f32 * 0.5, pelvis.translation.y, head.translation.x, head.translation.y, head.translation.z
        );
    }
}
