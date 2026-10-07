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

We split each frame into **4 substeps** (`physics.rs`). Smaller steps make chains
of joints much more stable, because errors don't get time to build up.

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

Each joint also has a little **friction** (a weak motor trying to keep the joint
still), so the limp body doesn't flop around like a wet noodle.

Parts connected by a joint don't collide with each other (they overlap slightly
at the joint), but all other pairs do: an arm can't pass through the chest.

**Stage 1 status:** no muscles yet, so the body collapses the moment it spawns.
That's expected: a pure ragdoll is just a dead body.

## 4. Shooting (`weapons.rs`)

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

## 5. Throwing (`throwing.rs`)

Thrown objects are real dynamic bodies, so the physics engine resolves their
collision with the person by itself. What matters is **momentum** (mass ×
velocity): a 25 kg crate at 20 m/s carries 500 kg·m/s; a 2 kg ball at the same
speed carries only 40.

Fast, small objects can pass through thin things between two physics steps
("tunnelling"). **CCD** (continuous collision detection) checks the path
between steps to prevent that.

## 6. The player and camera (`player.rs`)

The player is a **kinematic character controller**: each frame we say "I'd
like to move this much" and Rapier slides us along walls, up steps and down
slopes. Gravity and jumping are added by our code.

The camera sits behind and to the right of the player (an over-the-shoulder
view). If a wall gets between camera and player, a ray finds the wall and pulls
the camera in front of it.

**Shoulder-barging:** when the controller reports bumping into a body part
while moving, we kick that part with an impulse proportional to your speed.

## 7. Collision layers (`layers.rs`)

Every collider belongs to a **group** (world, ragdoll, player, prop) and lists
which groups it collides with. The aim ray, for example, ignores the player,
so you never shoot yourself in the back of the head.

## Coming next

- **Stage 2 (muscles):** motors in every joint pull toward a standing pose.
- **Stage 3a (balance):** keeping the centre of mass over the feet, stumbling,
  throwing arms out when falling.
- **Stage 3b (stepping):** taking a step to catch a fall.
