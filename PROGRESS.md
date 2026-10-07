# Progress log

Newest first. Each entry says what was built, how it was checked, and what
you should look at when you run it.

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
