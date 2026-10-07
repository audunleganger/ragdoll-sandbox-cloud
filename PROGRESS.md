# Progress log

Newest first. Each entry says what was built, how it was checked, and what
you should look at when you run it.

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
