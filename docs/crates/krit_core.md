# 5.1 `krit_core`: contracts

> Part of the [crate architecture plan](../architecture.md) (§5). Section references (§) point to
> that document unless stated otherwise.

**Owns:** only types, traits, sets and tiny pure helpers. It has no gameplay systems except the set
configuration, edge derivation and generic gate application.

| Module | Contents |
|---|---|
| `tick` | `SimTick(u32)` resource, incremented in `KritSimSystems::Intent`. Every message carries a tick. `krit_net` aliases it to the network tick rather than running two counters. |
| `entity` | `Player`, `Character`, the relationship `Controls(Entity)` ↔ `ControlledBy`, `Owner` / `Instigator` relationship for spawned things (projectiles, grenades). `SimId(u64)`: a network-stable id assigned by the authority, used for keys and ordering. `PersistentId` for authored level entities. |
| `intent` | `CharacterIntent` (see §6.1), `Buttons` bitflags (held), `PrevButtons` and `ButtonEdges` (derived in sim), `ItemIntent`, `CharacterIntent::inert()` |
| `aim` | `ViewAngles { yaw: u32, pitch: i32 }` (binary angles, 2³² per turn; §6.2.1), `LocalAim { yaw, pitch }` (client-only `f32` input accumulator, written only by `krit_input`), `ViewAngleOffset { yaw: u32, pitch: i32 }` (sim-forced facing: spawn, teleport, scripted look, platform yaw; never input), `PITCH_LIMIT` (shared pitch clamp, just under 90°), `AimRecoil { pitch, yaw }` (aim-affecting, additive, decays), `EyeHeight`, `AimRay { origin, dir }` helper that includes `AimRecoil`, and shortest-angle and quantize helpers. The layering is in §6.2. |
| `gates` | `ActionGates` as a component: block bits (`MOVE`, `LOOK`, `FIRE`, `ADS`, `SPRINT`, `INTERACT`, `SWITCH`, `RELOAD`, plus game-defined bits), and `forced` / `disabled` `Buttons` masks applied to the intent in `Intent`. Domains *contribute*; consumers *query*. A debug build records which plugin set each bit. Cancel rules live in `ActionRules` (§5.6). |
| `authority` | `SimAuthority`, `SimReplay`, `has_authority`, `first_time_simulated` (§4.2) |
| `team` | `TeamId`, `Team` component, the `TeamRelations` resource trait (`relation(a, b) -> Relation::{Self_, Friendly, Neutral, Hostile}`) with a default impl |
| `combat` | `ShotRequest`, `HitResult`, `HitZone`, `Hurtbox { owner, zone, multiplier }`, `DamageRequest`, `DamageApplied`, `Died`, `DamageTypeId`, `DamageFlags`, `ImpactEvent`, `HitConfirmed` |
| `physics` | `SpatialQueryBackend` trait (`cast_ray`, `cast_shape`, `overlap`, `contents(point) -> ContentsFlags`, with layer filters), the KCC-facing `CollisionQuery` trait (`cast_shape`, `depenetrate`, `intersections`, `contents`, `surface(entity) -> SurfaceInfo`), `KritLayer::{World, Character, Hurtbox, Projectile, Interactable, PlayerClip}` |
| `rng` | `SimRng::for_shot(tick, shooter: SimId, shot_index)` and `SimRng::for_stream(tick, stream: SimId, salt)`: seeded, deterministic, predictable. Never seeded from `Entity`. |
| `curve` | `Curve1` piecewise-linear curve, shared by spread heat, falloff and stat curves |
| `noise` | `NoiseEvent { origin, loudness, kind, instigator }`, written by movement, weapons and impacts for AI and stealth |
| `sim_state` | `#[reflect(SimState)]` marker with `SimStateClass::{OwnerPredicted, Predicted, Interpolated, Replicated, ServerOnly, Derived}` and optional owner-only fields. Read by `krit_net` and `krit_persist`. |
| `sets` | `KritSimPhase`, `KritSimSystems`, `KritViewSystems`, `KritCorePlugin`, `emit_fact` |

**Customization:** you would swap backend implementations, not this crate. Any game needs it, even
one that uses only a single Krit crate.
