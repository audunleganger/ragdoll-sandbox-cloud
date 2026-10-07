//! Collision layers: which kinds of objects are allowed to touch which.
//!
//! Every collider gets a `CollisionGroups { memberships, filters }`. Two
//! colliders touch only if each one's memberships appear in the other's filters.

use bevy_rapier3d::prelude::*;

pub const WORLD: Group = Group::GROUP_1;
pub const RAGDOLL: Group = Group::GROUP_2;
pub const PLAYER: Group = Group::GROUP_3;
pub const PROP: Group = Group::GROUP_4;

pub fn world() -> CollisionGroups {
    CollisionGroups::new(WORLD, Group::ALL)
}

/// Body parts touch everything, including other body parts (so an arm can't
/// pass through the chest). Parts joined directly to each other are excluded
/// by the joint itself, see `ragdoll.rs`.
pub fn ragdoll() -> CollisionGroups {
    CollisionGroups::new(RAGDOLL, Group::ALL)
}

pub fn player() -> CollisionGroups {
    CollisionGroups::new(PLAYER, WORLD.union(RAGDOLL).union(PROP))
}

pub fn prop() -> CollisionGroups {
    CollisionGroups::new(PROP, Group::ALL)
}
