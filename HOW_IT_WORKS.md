# How it works

A plain-language tour of the physics, with pointers to the code. You don't need
to read the code to follow this, but each section says where to look if you want to.

## 1. The game loop and the ECS (`main.rs`, `lib.rs`)

A game is a loop: about 60 times a second it reads input, updates the world,
and draws a picture. Bevy organises this as an **ECS** (Entity Component
System):

- An **entity** is just an ID, a "thing" in the game (the left shin, the player,
  a crate).
- **Components** are data attached to entities (`Transform` = position and
  rotation, `RigidBody` = "physics moves this", `RagdollPart` = "I am a body part").
- **Systems** are functions that run every frame on all entities with certain
  components. For example, `shoot` in `weapons.rs` runs every frame and checks
  if you clicked.

Code is grouped into **plugins**, one per topic. `CorePlugin` (physics, map,
person) works without a window, which lets the tests run it. `GamePlugin` adds
everything that needs a screen, keyboard and mouse.

## 2. Rigid bodies (`ragdoll.rs`, `world.rs`)

The physics engine (Rapier) simulates **rigid bodies**: solid objects that
never bend. Each one has a mass, a shape (for collisions), a position, a
rotation, and a velocity. Every step, Rapier:

1. applies gravity and any pushes (forces and impulses) to update velocities,
2. finds which shapes touch (collision detection),
3. solves **constraints**: "these two must not overlap", "these two must stay
   joined", by nudging velocities until everything agrees,
4. moves everything by its velocity × time step.

The map is made of **fixed** bodies (infinitely heavy, never move). Thrown
objects and body parts are **dynamic** (moved by physics). The player is
**kinematic** (moved by our code, and pushes things out of the way).

**Fixed time step** (`physics.rs`): physics always advances in steps of exactly
1/120 s, each split into 4 substeps (so 1/480 s slices), no matter how fast your
monitor refreshes. Rendering *interpolates* between the last two physics states
so motion still looks smooth. Why it matters: many details (how stiff contacts
are, how muscles respond) depend on the step size. With steps that followed the
frame rate, the person stood fine at 60 Hz but toppled at 144 Hz. Now the game
and the tests run identical physics.

**Soft contacts:** to stay stable, physics engines let touching objects sink into
each other a tiny bit and push back like a stiff spring. The default springiness
was too soft for 80 kg on two small feet: the heels sank a hair, the body tipped
back, more weight went onto the heels, they sank more... and it fell over.
We made contacts stiffer.

## 3. The ragdoll (`ragdoll.rs`)

The person is **14 rigid bodies**: pelvis, belly, chest, head, upper arms,
forearms, thighs, shins and feet. They're simple shapes (capsules, a ball and
boxes), with realistic masses adding up to ~82 kg.

They're connected by **joints**. A joint glues two bodies together at one
point (the *anchor*, e.g. the knee) and says how they may rotate:

- **Ball joints** (hips, shoulders, spine, neck) can rotate around all three
  axes: bend forward/back, twist, tilt sideways.
- **Hinges** (knees, elbows) only bend around one axis.
- The **ankle** bends forward/back and tilts a little sideways, but can't twist.

Each axis has **limits**: a knee bends 0° to ~140° and never backwards; a hip
swings forward ~110° but back only ~30°. Without limits you'd get a horror-movie
contortionist. Limbs on the right side use mirrored limits of the left side.

Each joint also has a little **friction**, so the limp body doesn't flop around
like a wet noodle. It's built from the joint motor: a very strong "resist any
movement" setting, capped at a small maximum torque. Small loads (an arm resting
on the ground) can't move the joint; big loads (falling) easily can.

Parts connected by a joint don't collide with each other (they overlap slightly
at the joint), but all other pairs do: an arm can't pass through the chest.

Without muscles a ragdoll is just a dead body; it collapses the moment it spawns.

## 4. Muscles (`muscles.rs`)

Every joint axis gets a **motor**, which behaves like a spring plus a shock absorber:

```text
torque = stiffness × (target angle − current angle) − damping × rotation speed
```

- The **spring** (stiffness) pulls the joint toward its target angle: further
  away = harder pull.
- The **shock absorber** (damping) resists fast movement, so the joint settles
  instead of wobbling back and forth.
- The torque is **capped** at the muscle's maximum strength (e.g. 320 N·m for
  the hips, 40 N·m for the elbows). That cap is what makes a hit *overpower*
  the muscles instead of the body standing there like a statue.

Engineers call this a **PD controller** (Proportional-Derivative). The physics
engine runs it inside its solver, which keeps even stiff muscles stable.

All joints together aim for a **target pose**: the standing pose has slightly
bent knees and elbows and arms hanging a little away from the body
(`standing_pose`).

**Muscle tone** (0–100%) scales every muscle at once. At 0% only friction
remains (limp). Try **G** to go limp and back, and **−** / **=** to see the
person sag as tone drops.

How strong is strong enough? Standing still is like balancing an 80 kg stick
on its end, pivoting at the ankles. If it leans by an angle θ, gravity twists
it further with about *weight × height × θ* ≈ 800 N·m per radian. The ankle
muscles must push back harder than that, so they're set to 900 N·m/rad each.
You can see this rule at work: below ~45% tone the two ankles together drop
under 800 N·m/rad, and the person topples over stiffly instead of standing.

**Stage 2 limitation:** muscles hold a *pose*, they don't *balance*. Push the
person and they tip over stiffly, like a statue. Fixing that is Stage 3.

## 5. Balance, stepping and falling (`balance.rs`)

Muscles hold whatever pose they're given. The **balance controller** is the
"brain" that picks that pose every frame. It's a small *state machine*:

```text
Standing ──(ankles not enough)──▶ Stepping ──(foot planted)──▶ Standing
    │                                │
    └──(hopeless / tipping over)─────┴──▶ Falling ──(landed)──▶ Down
```

**Sensing.** Add up every body part's position × mass to get the **centre of
mass** (COM), and the same for velocities. The feet's positions give the
**support area**.

**The capture point.** A standing body is like an upside-down pendulum. If
the COM is at height *h* and moving at speed *v*, it would come to rest above

```text
capture point = COM + v × √(h / g)        (g = 9.81 m/s²)
```

If your feet are under that point, you can stop. If not, you'll fall unless
you step. Because it includes speed, the controller reacts to a shove at once,
before the body has visibly leaned.

**Ankle strategy (Standing).** If the capture point is ahead of the feet, press
the toes down a little; if behind, lift them. In code: nudge the ankle muscle
targets in proportion to the error. There's a hard limit: a foot can only push
as hard as *weight × distance to the toe* before it rolls onto its edge. That's
why the person stands with a ~1° forward lean: it centres their weight over the
feet, leaving room to sway both ways.

**Stepping.** Once the capture point passes the edge of the feet, the ankles
can't win; a human takes a step. We choose a leg (the side of a sideways push,
then alternating), aim the foot just past the capture point (re-aiming every
frame as the body moves), and swing it along an arc. To put the foot where we
want, we need joint angles: that's **inverse kinematics**. For a two-segment leg
it's the law of cosines: the hip-to-ankle distance fixes the knee bend, and the
direction fixes the hip angles. Meanwhile, the standing hip keeps the pelvis
upright *in the world* (a trick from SIMBICON, a well-known walking controller).

**Falling.** If even a step can't reach (or the chest tips past ~40°), the
person **braces**: arms thrown toward the fall, chin tucked, knees soft. After
landing, muscle tone fades to ~12% over 1.5 s: lying there, not a noodle.

**How strong a shove can they take?** About 30–35 N·s at the chest (like a
30 kg crate hitting at 1 m/s). Ankles alone managed 20–25.

## 6. Shooting (`weapons.rs`)

Guns are **hitscan**: no bullet flies through the air. We cast an invisible ray
from the camera through the crosshair, find the first thing it hits, and give
that body an **impulse** at the hit point.

An impulse is an instant kick that changes velocity by *impulse ÷ mass*. Because
it hits a specific *point* rather than the centre, it also creates spin
(**torque**): torque = lever arm × push. A shot to the shoulder twists the
torso; a shot to the shin knocks the leg out.

The shotgun fires 9 rays with random spread, each weaker than a pistol round.

**Explosions** push every body within 6 m away from the centre, harder the
closer it is, with some extra upward lift. The kick is scaled by each part's mass,
so light and heavy parts get the same change in speed.

## 7. Throwing (`throwing.rs`)

Thrown objects are real dynamic bodies, so the physics engine resolves their
collision with the person by itself. What matters is **momentum** (mass ×
velocity): a 25 kg crate at 20 m/s carries 500 kg·m/s; a 2 kg ball at the same
speed carries only 40.

Fast, small objects can pass through thin things between two physics steps
("tunnelling"). **CCD** (continuous collision detection) checks the path
between steps to prevent that.

## 8. The player and camera (`player.rs`)

The player is a **kinematic character controller**: each frame we say "I'd
like to move this much" and Rapier slides us along walls, up steps and down
slopes. Gravity and jumping are added by our code.

The camera sits behind and to the right of the player (an over-the-shoulder
view). If a wall gets between camera and player, a ray finds the wall and pulls
the camera in front of it.

**Shoulder-barging:** when the controller reports bumping into a body part
while moving, we kick that part with an impulse that grows with your speed
*squared* (like collision energy): walking into someone (~11 N·s) nudges
them; sprinting (~45 N·s) knocks them into a stagger or over.

## 9. Collision layers (`layers.rs`)

Every collider belongs to a **group** (world, ragdoll, player, prop) and lists
which groups it collides with. The aim ray, for example, ignores the player,
so you never shoot yourself in the back of the head.

## Not built (yet)

- Getting back up after falling.
- Walking around on their own (AI locomotion).
- Shot reactions per body part (clutching a wounded arm, legs buckling when
  shot in the knee). The physics of the hit is already per body part; the
  "brain" doesn't react to *where* yet.
