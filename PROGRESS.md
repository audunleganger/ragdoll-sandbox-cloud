# Progress log

Newest first. Each entry says what was built, how it was checked, and what
you should look at when you run it.

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
- Press G (limp) then G again: the person can't get back up from the floor
  (out of scope), but tone returns. Press R to reset.
- Play with − / = while they stand (press R between tries). Measured headless:
  at 60% they visibly sag (head drops 13 cm); at 50% and below they topple
  over stiffly. That matches the ankle calculation in `HOW_IT_WORKS.md`:
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
