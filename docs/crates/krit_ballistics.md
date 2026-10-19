# 5.8 `krit_ballistics`: resolving shots

> Part of the [crate architecture plan](../architecture.md) (§5). Section references (§) point to
> that document unless stated otherwise.

**Owns:**
- the `BallisticProfile` asset: `Hitscan { range, radius }`, `Projectile { speed, gravity, drag,
  lifetime, fuse }` or `Hybrid { hitscan_until }`, plus pellets, penetration power, ricochet rules,
  damage, a falloff `Curve1` and an `explosion: Option<ExplosionSpec>`;
- projectile entities (`Projectile`, `ProjectileState`, `Instigator`), keyed by `ShotId` so a
  client-predicted projectile can be matched to the server's (lightyear `PreSpawned`, replicon
  `Signature`);
- `PenetrationMaterial` / `SurfaceMaterial` components on colliders;
- explosion overlap and occlusion;
- `HurtboxPose` sources: hurtbox poses are driven from simulation state (stance, `ViewAngles`), never
  from animation, because the headless server does not animate;
- the lag-compensation *query hook* (`RewindProvider` trait in core, implemented by `krit_net`).

**Resolution rules (normative):**
- **Hitscan order:** trace the world first, then trace hurtboxes clipped to the world hit distance.
  Only hurtboxes are rewound.
- **Self-hits:** the shooter's own hurtboxes are ignored by default (`IgnoreInstigator`).
- **Projectile launch:** aim from the eye, launch from the eye plus the item's sim-side
  `MuzzleOffset`. If the eye-to-muzzle segment is blocked, launch from the eye.
- **Zone multipliers** are geometric and applied here. Explosions skip zone multipliers and damage
  each victim once, and a direct hit does not also take full splash unless the profile says so.
- **Explosions** check occlusion and falloff by default.
- Damage-emitting systems run under `has_authority`; clients may still trace for predicted
  hitmarkers and impacts (§5.14).

**Consumes:**
- `ShotRequest`;
- `SpatialQueryBackend`;
- `Hurtbox` components (defined in core);
- `SimRng::for_shot`, for deterministic spread.

**Produces:**
- `HitResult` for every hit, including penetrations, keyed by `(shot_id, pellet, penetration)` for
  de-duplication;
- `ImpactEvent` (point, normal, material), for decals and VFX, with the same key;
- a `DamageRequest` for every damaging hit. The amount already includes range falloff and the
  geometric zone multiplier, because ballistics is the only stage that knows distance and zone.
  Pellets are grouped by `shot_id`;
- `ExternalImpulse` for knockback;
- `NoiseEvent` for impacts.

**Plugins:** `Hitscan`, `Projectiles`, `Penetration`, `Ricochet`, `Explosions`, `HitboxDebug`.

**Boundary:**
- Ballistics only *requests* damage.
- Games without health (paintball scores, physics puzzles, tag) can consume `HitResult` directly and
  never install `krit_damage`.
