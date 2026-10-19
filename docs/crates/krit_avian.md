# 5.15 `krit_avian`: physics backend

> Part of the [crate architecture plan](../architecture.md) (§5). Section references (§) point to
> that document unless stated otherwise.

**Owns:**
- the implementation of `SpatialQueryBackend` on a `SystemParam`, wrapping `SpatialQuery`;
- the `CollisionQuery` primitives (`cast_shape`, `depenetrate`, `intersections`, `contents`,
  `surface`) that `krit_movement`'s algorithm calls. It does **not** own the slide algorithm (P9);
- writing movement output to avian `Position` / `LinearVelocity`;
- mapping `KritLayer` to avian `CollisionLayers`;
- `register_required_components_with` (Character → `RigidBody::Kinematic` + `Collider` from the
  controller's hull shape, `Hurtbox` → `Sensor` collider);
- placing `KritSimPhase::PrePhysics` / `PostPhysics` around `PhysicsSystems` (§4);
- gating simulation while physics is paused.

**Boundary:** this is the only crate that imports avian. A `krit_rapier` backend could be added
later without touching any logic crate, and because the slide algorithm is in `krit_movement`,
movement would feel the same on it.
