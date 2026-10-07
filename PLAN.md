# Ragdoll Sandbox: Plan

A small GTA IV-inspired sandbox: one physically simulated person who stands using
simulated muscles, keeps their balance, stumbles when hit, and braces with their
arms when falling. You walk around in third person and shoot, throw things at,
blow up, or shoulder-barge them.

## Decisions (from planning Q&A)

| Topic | Decision |
|---|---|
| Stack | Rust + **Bevy 0.19** + **bevy_rapier3d 0.36** (Rapier 0.35), versions pinned exactly |
| Target PC | Arch Linux, Vulkan |
| Delivery | Public GitHub repo on `audunleganger`, delivered in stages |
| Fidelity | Active ragdoll: muscles + balance + stumble + arm bracing. **No** getting up, **no** walking AI |
| People | One person, `R` to reset |
| After a fall | Lies there (muscles relax to low tone) until reset |
| Health | None: pure physics |
| Player | Third-person, kinematic character controller (not a ragdoll) |
| Gun | Pistol + shotgun (switch `1`/`2`), hitscan |
| Throwing | Hold RMB to charge, release; cycle light ball / heavy crate |
| Explosions | Key press at the crosshair point (radial impulse) |
| Body contact | Shoulder-barge: impulse scaled by player speed |
| World | Flat ground + raised ledge + stairs + ramp |
| Extras | Slow-motion toggle |
| Visuals | Simple capsule mannequin, basic lighting |

## How the person works (the core idea)

1. **Ragdoll**: about 13 rigid bodies (pelvis, belly, chest, head, upper/lower arms,
   thighs, shins, feet) linked by Rapier *multibody joints* with realistic joint
   limits. Box-shaped feet so the body can actually stand.
2. **Muscles ("tone")**: every joint has a motor that pulls toward a target angle
   with a stiffness and damping (a PD controller running inside the physics solver).
   The motors have limited strength, which is why the body sags and staggers instead of
   standing like a statue. One global "tone" value scales all of them.
3. **Balance**: each frame we compute the centre of mass (COM) and the area between
   the feet. If the COM drifts outside it, we shift the ankle and hip targets to push it
   back (the "ankle strategy" and "hip strategy" humans use).
4. **Fall reaction**: if the torso tilts or the COM moves too far, switch to a
   *protective* pose (arms out toward the fall direction, chin tucked), then ramp
   the tone down once on the ground.

## Interaction mechanics

- **Shooting**: ray from the camera through the crosshair (ignoring the player),
  impulse applied *at the hit point* on the hit body part. A shotgun fires about 8 rays
  with spread.
- **Throwing**: spawn a dynamic ball/crate in front of the player, with velocity scaled
  by charge time.
- **Explosion**: every ragdoll part within a radius gets an impulse falling off with
  distance, plus some upward lift.
- **Shoulder-barge**: detect player-vs-ragdoll contact and apply an impulse to the
  touched part scaled by player speed (more tunable than the controller's built-in push).
- **Slow-mo**: scales the physics time step.

## Code layout (one concept per file)

```
src/main.rs       app setup, plugin wiring
src/world.rs      ground, ledge, stairs, ramp, lighting
src/player.rs     third-person character + camera
src/ragdoll.rs    body parts, joints, spawning/reset
src/muscles.rs    joint motors, tone, target poses
src/balance.rs    COM, support area, balance + fall reactions
src/weapons.rs    pistol, shotgun, explosions
src/throwing.rs   charge-and-throw objects
src/hud.rs        crosshair, current weapon/tone/slow-mo text
tests/            headless physics checks (no window)
HOW_IT_WORKS.md   plain-language explanation mapped to files
README.md         Arch install commands, controls, first-build warning
```

## Stages

Each stage ends with: it builds, headless tests pass, pushed to GitHub, **you run it
and report what looks wrong**, and I tune before moving on.

| Stage | Contents | Automated check (headless) | What you check by eye |
|---|---|---|---|
| **1** | World, third-person player, limp ragdoll, pistol/shotgun, throwing, explosions, slow-mo, reset | Ragdoll falls and settles without exploding or sinking through the floor | Controls feel OK; ragdoll looks like a body, not spaghetti |
| **2** | Muscles: holds a standing pose with tone | Pelvis stays above 0.8 m for 10 s untouched | Stands naturally, sags a little, not robot-stiff |
| **3a** | Balance (ankle/hip) + stumble + protective arms when falling + shoulder-barge | Small shove leads to recovery, large shove leads to a fall | Reactions look alive / GTA-ish |
| **3b** | *Stretch:* recovery steps (stepping a foot out to catch a fall) | Medium shove recovered by a step | Step looks plausible |

**Risk note on 3b:** ankle/hip balance and arm bracing are very achievable. Taking
an actual step is a large jump in difficulty (choose where to place the foot, swing
the leg, shift weight while in contact with the ground) and may need several tuning rounds.
It's split out so stages 1–3a are solid regardless.

## Known limitations / out of scope

- Getting back up, walking AI, multiple people, health/death.
- Rapier's motors are less precise than research simulators such as MuJoCo, so expect tuning.
- I can't see the window from the cloud machine. Visual tuning depends on your feedback.
