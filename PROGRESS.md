# Progress log

Newest first. Each entry says what was built, how it was checked, and what
you should look at when you run it.

## Shoulder-barge checked and tuned ✅

Never checked before; now verified in the game (with a log line per barge).
It was far too strong: walking into the person gave 38 N·s per bump, which
knocked them flat and then kept shoving the body. Now the shove grows with
speed squared: **walking** (4 m/s) = 11 N·s, and six bumps in a row left them
standing; **sprinting** (8 m/s) = 45 N·s, and they went down.

## Real game timing ✅ (with a frame-rate caveat)

All earlier tests advanced physics one fixed step at a time. The game itself
steps physics whenever enough real time has passed and *smooths* positions
between steps for display. A new test runs that exact mode at 20–144 fps.

**Found:** the balance controller was reading the smoothed positions, which
lag the real physics slightly, and at 90 fps a 30 N·s shove then knocked them
over. Fixed: the controller now reads position, rotation and velocity straight
from the physics engine.

**Measured with the game's own timing** (30 N·s forward shove after standing 3 s):

| fps | 20 | 30 | 45 | 60 | 90 | 144 |
|---|---|---|---|---|---|---|
| stands | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ |
| 20 N·s shove | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ |
| 30 N·s shove | ✅ | ❌ | ❌ | ✅ | ✅ | ✅ |

**Caveat:** the controller runs once per rendered frame, so below ~60 fps it
updates only every 2–4 physics steps and the hardest shoves get through. The
proper fix is to run the controller inside the fixed physics step (Bevy's
`FixedUpdate`) and smooth the rendering separately. That's a bigger
restructuring that I didn't want to do overnight without you able to check how it looks.

## Terrain: platform, stairs, ramp ✅ (partly, see the limits)

You picked ledges, stairs and a ramp to push people off, but until now every
test ran on flat ground at height 0, and the balance code quietly assumed
the ground *was* at height 0.

**Fixed**
- Heights are measured from the actual ground under the feet (the lower
  sole), and each step's landing height is found by casting a ray downward. So
  stepping works on the 3 m platform, and steps up/down small heights work.
- A ledge that drops more than 45 cm can't be stepped down: pushed near the
  platform edge, the person falls off and ends up lying on the ground below
  (that's a test).
- "Has landed" no longer means "close to height 0" but "centre of mass close to
  the lowest body part", so it works at any height.
- Ankles compensate for sloped ground (measured by a ray under each foot), so
  they can stand on the ramp facing up or down it.

**Measured**

| Where | Stands | 30 N·s forward | 30 N·s sideways |
|---|---|---|---|
| flat ground | ✅ | ✅ | ✅ |
| 3 m platform | ✅ | ✅ | ✅ |
| a stair step | ✅ | ❌ falls | ❌ falls |
| ramp, facing uphill | ✅ | ✅ | ❌ falls |
| ramp, facing across | ❌ falls | | |

**Known limits**
- **Across a slope** they can't stand: one foot is ~4.5 cm higher, and with
  equal straight legs the body leans downhill. People bend the uphill knee. I
  tried that and it broke flat-ground staggers, so I reverted it.
- **Stairs:** treads are 60 cm deep; staggers drift toward a tread's edge and a
  foot lands half on the step below. That would need proper foothold selection.

## Stage 3b: stepping (stagger) ✅

**Built**
- When the ankles can't cope (capture point past the edge of the feet), the
  person **takes a step**: the foot swings along a small arc and lands just
  past the capture point, re-aiming mid-swing as the body keeps moving.
  Legs alternate, up to 6 steps in a row, then they fall.
- Leg positioning uses two-segment *inverse kinematics* (law of cosines for
  the knee, then the hip angles), with the foot kept flat.
- While on one leg, the standing hip keeps the pelvis upright *in the world*
  (the SIMBICON trick). Arms fling out sideways: the classic stagger.
- HUD shows "staggering".

**Measured (headless, chest shoves)**

| Direction | Ankles only (3a) | With stepping (3b) |
|---|---|---|
| forward | 20 N·s | 30 N·s |
| back | 25 N·s | 35 N·s |
| left | 20 N·s | 35 N·s |
| right | 20 N·s | 35 N·s |

**Problems found and fixed on the way**
- The first version fell more often than no stepping at all. Tracing single
  steps showed: the target was chosen once and went stale while the body kept moving
  (fixed: re-aim every frame); the foot lagged its path (fixed: aim 0.08 s
  ahead); the pelvis pitched forward on one leg (fixed: world-upright hip
  control); the second step reused the leg that had just landed and was
  carrying the weight (fixed: always alternate); the sideways pelvis
  correction had the wrong sign (found by sweeping both signs: limit 20 → 40).
- Making the arms reach forward during a step looked more protective but
  moved the body's weight forward and broke forward staggers, so they fling
  sideways instead.

**Checked automatically** (16 tests): 30 N·s shoves caught in every direction
(and not without stepping); a stagger takes at least one step and ends
standing still; all earlier tests still pass. Also checked: 30 N·s holds in every
direction with the controller at 60 Hz.

**Checked by screenshot:** two quick pistol shots. The first makes the person
stagger (arms out, foot lifting); the second catches them mid-step, and they
twist and fall reaching out with an arm, then lie still.

**Please check when you run it**
- Shoot once, then twice quickly. Shove with Shift-sprint into them. Does the
  stagger look like a stagger?
- Watch in slow motion (T): steps take 0.32 s; does that look too quick or slow?
  (`step_duration` in `balance.rs`.)

## Stage 3a tuning: pistol, lean, foot creep ✅

**What was wrong:** in the game a single pistol shot still knocked the person
over. Tests replaying the exact logged shot showed why. Shooting someone from
the front pushes them *backward*, and the backward margin was the smallest:
the heel is only 8 cm behind the ankle, versus 16 cm to the toes. The pistol was
also tuned against a test shove that didn't include the spin from an
off-centre hit.

**Fixes**
- Standing pose leans ~1° forward (weight over mid-foot, like real people),
  so the shove limit is ~20–25 N·s in every direction instead of 30/20.
- Pistol 16 N·s (rocks them; quick follow-up shots drop them), shotgun
  9 × 10 N·s (drops them).
- The bullet-hit maths (push + spin) is now one shared function (`apply_hit`)
  used by both the gun and the tests.
- Feet were slowly sliding (~3 mm/s) under the lean. Fixed with a Rapier
  solver option (`friction_in_bias_pass`): 34 mm → 0 mm creep in 10 s.

**New tests:** one pistol shot (centre, left or right of the chest, fired
from the game's camera position) leaves them standing; a point-blank shotgun
blast drops them. Low controller rates (60/30/15 Hz) were also checked:
a 20 N·s backward shove holds at all of them.

## Stage 3a: balance and fall reactions ✅

**Built**
- `balance.rs`: the "brain". Each frame it measures the centre of mass and
  its velocity, predicts the *capture point* (where the body would come to
  rest), and shifts the ankle and hip muscle targets to keep it over the feet.
- Falling: when the capture point gets 30 cm outside the feet (or the chest
  leans over 40°), the person gives up balancing and **braces**: arms thrown
  toward the fall direction, chin tucked, knees softened. On landing, muscles
  relax over 1.5 s to a low tone (lying there, not a noodle).
- HUD shows the person's state: standing / falling / down.
- Pistol impulse lowered from 40 to 22 N·s.

**Measured (headless)**
- Without stepping, the ankles can only absorb about 20–30 N·s at the chest.
  That's physics, not tuning: 27 gain combinations all landed between 20
  and 30. The feet are small, and pushing harder with the ankles only rolls
  them onto their edges. Bigger hits need a *step* (Stage 3b).
- A 20 N·s shove from the front topples the Stage 2 statue but not the
  balancing person.
- Falling forward, the hands reach ~30 cm ahead of the chest within 0.3 s.

**Checked automatically** (`cargo test`, 11 tests)
- 20 N·s shoves in all four directions are absorbed.
- Balancing beats standing like a statue (same shove: statue falls, balancing doesn't).
- A 120 N·s shove: falls, hands reach ahead of the chest, ends lying down relaxed.
- Balance still works when the controller only runs 60 times a second.

**Note on screenshots:** my in-game checks run on a virtual screen with software
rendering at a few frames per second, so balance seen there isn't
representative of your PC. The controller runs once per frame.

## Stage 2: muscles ✅

**Built**
- `muscles.rs`: a PD motor (spring + shock absorber, capped at real-ish muscle
  strength) on every joint axis, pulling toward a standing pose.
- Muscle tone 0–100% scales all muscles: **G** = limp on/off, **−** / **=** = tone
  down/up, shown on the HUD.
- Physics now runs at a fixed 1/120 s step (4 substeps) with smooth
  interpolation for rendering, independent of monitor refresh rate.
- Slow motion now slows the whole game clock (the physics step size stays the same).
- Hits are logged: run with `RUST_LOG=ragdoll_sandbox=debug cargo run` to see
  `Pistol hit Chest at [...]` in the terminal.

**Problems found and fixed on the way**
1. *Standing person slowly toppled after 2–10 s, and faster at 144 Hz.*
   Measurements showed every joint holding its angle while the whole body
   tipped over as one piece, and the feet weren't sliding. Cause: the ground
   contacts were too soft, so the heels sank in a hair, the body leaned back,
   more weight went on the heels, and so on. Stiffer contacts fixed it, but only with small
   enough physics steps, which is why the step is now fixed at 1/480 s slices.
2. *A limp body crept for seconds after landing.* The joint "friction" was
   really a weak damper, and dampers can't hold still loads. It's now a very
   strong damper capped at the friction torque, which acts like real friction.

**Checked automatically** (`cargo test`)
- With muscles on, the person stands for 20 s: pelvis never below 0.9 m,
  drift under 10 cm, swaying under 5 cm/s at the end.
- Turning muscles off drops the body; all Stage 1 tests still pass with tone 0.
- The game's time-step settings match the ones the tests use.

**Checked by screenshot**
- Standing person in the game. A pistol shot to the chest logs the hit and
  knocks them flat on their back, toppling stiffly. That's expected until
  Stage 3 adds balance.

**Please check when you run it**
- Does the standing pose look natural?
- Press G (limp): they collapse. Pressing G again restores your tone setting,
  but once they're lying down the balance "brain" keeps them relaxed
  (getting up is out of scope). Press R to reset.
- Play with − / = while they stand (press R between tries). Measured headless:
  at 60% they sag a little (Stage 2: head 13 cm lower; with Stage 3 balance:
  4 cm); at 50% and below they topple over, with or without balance. That matches the ankle calculation in `HOW_IT_WORKS.md`:
  2 ankles × 900 N·m/rad × tone must beat ~800 N·m/rad, which fails just
  under 45%.

## Stage 1: world, player, limp ragdoll, weapons, throwing ✅

**Built**
- Test map: ground, 3 m platform with stairs, ramp up to a ledge, waist-high wall.
- Third-person player (WASD, sprint, jump) with an over-the-shoulder camera
  that doesn't clip through walls.
- 14-part ragdoll (~82 kg) with joint limits and joint friction.
- Pistol and shotgun (hitscan, impulse at the hit point), explosions (E),
  charge-and-throw balls and crates, shoulder-barge, slow motion (T), reset (R),
  spawn-in-front (F). HUD with crosshair and controls.

**Checked automatically** (`cargo test`, headless)
- A limp body collapses, comes to rest on the ground and doesn't sink in.
- Joints stay connected (under 3 cm gap) through the fall.
- Knees only bend backward and elbows only forward, within their limits.
- A pistol-sized impulse knocks the chest back.

**Checked by screenshot** on a virtual screen with software rendering. That's
slow, so only roughly:
- The game starts with no crashes; map, lights, shadows and HUD render.
- Mouse capture, F (spawn), E (explosion) and throwing work.
- The person spawns standing, then collapses into a believable heap.

**Changed from the plan**
- Joints use Rapier's ordinary *impulse joints*, not *multibody joints*.
  Rapier's multibody joints crash on 2-axis joints like the ankle (unfinished
  code in the library), and their ball joints track angles in a way that drifts.

**Please check when you run it**
- Does the camera feel OK (distance, sensitivity)? Values are at the top of `player.rs`.
- Are shots, the shotgun and explosions too strong or weak? Values are at the top of `weapons.rs`.
- Does the limp body look natural when it falls, or too stiff or floppy? See
  `friction` per joint in `ragdoll.rs`.
