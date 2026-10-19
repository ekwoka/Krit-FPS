# 5.7 `krit_weapons`: deciding to shoot

> Part of the [crate architecture plan](../architecture.md) (§5). Section references (§) point to
> that document unless stated otherwise.

**Owns** per-weapon-entity components:
- `WeaponState` machine: `Idle`, `Firing`, `Cooldown`, `Reloading { phase }`, `Charging`, `Cooking`;
- `FireMode` (`Semi`, `Auto`, `Burst { n, delay }`, `Charge { min, max }`, `Cycle` for pump/bolt),
  plus a `FireModeSelector`;
- `FireRate` (RPM) with a **sub-tick accumulator**, so 1200 RPM at 60 Hz produces several shots per
  tick. A fresh press resets the schedule so idle time cannot bank shots (Source `ItemPostFrame`);
- `Magazine`, `Chamber` and `AmmoSource` (`Infinite`, `OwnerPool(ammo_types)` accepting several ammo
  types, `Internal`, `Regen { rate, delay }`);
- `Heat` (overheat weapons) as an optional component;
- `ReloadSpec` (tactical vs empty, per-shell, and a commit point given as data rather than as an
  animation event). Tools may bake animation lengths into `ReloadSpec` and `equip_time` at asset
  build time;
- `SpreadModel`: per-stance base spread, Lyra-style heat curves (`Curve1` for spread-by-heat,
  heat-per-shot and cooldown rate), recovery delay, centre-bias exponent and first-shot accuracy.
  Driven only by simulation inputs (`MovementSpreadFactor`, `AdsState`), never by the camera;
- `RecoilModel`, which is either `Pattern(Vec<Vec2>)` (CS-style) or `Random { … }`, plus recovery;
- `AdsState` (0..1 progress);
- `Melee` and `Throwable` (cook, fuse) as separate optional components;
- `WeaponStats`, computed through `krit_stats`;
- `WeaponDefinition` assets.

**Per-tick order** inside `WeaponSystems`, from Source's `ItemPostFrame`:
1. busy (switching, holstered) → nothing fires;
2. reload completion;
3. secondary (may block primary);
4. primary (fresh press resets the schedule; empty → `DryFire`, then auto-reload or switch by policy);
5. reload start;
6. idle (spread and recoil recovery).

**Consumes:**
- `ItemIntent`;
- the owner's `ActionGates`, `MovementSpreadFactor` and `ViewAngles`/`AimRecoil`/`EyeHeight` (read
  through `Owner`);
- `WeaponStats`;
- the owner's ammo pool, read through `krit_core`'s `AmmoProvider` trait so that it never imports
  inventory.

**Produces:**
- `ShotRequest` messages, one per shot (pellets are inside the request), carrying a snapshot of the
  stats ballistics needs so a projectile that lands later keeps the stats it was fired with;
- `AimRecoil` deltas on the owner, which affect aim;
- `WeaponFired { shot_count_total }`, a cumulative counter that presentation and remote clients use
  to recreate cosmetics;
- `ReloadStarted`, `ReloadFinished`, `DryFire`, `FireModeChanged` and `AdsChanged`;
- `NoiseEvent` for shots;
- an `ActionGates` contribution (no sprint while ADS, if the profile says so);
- `CurrentSpread`, a read model for the crosshair.

**Plugins:** `WeaponFireControl`, `WeaponAmmo`, `WeaponReload`, `WeaponSpread`, `WeaponRecoil`,
`WeaponAds`, `WeaponMelee`, `WeaponThrowables`, `WeaponHeat`.

**Boundary:**
- It *never* traces rays, spawns bullets or touches health. Its job ends at `ShotRequest`.
- Melee produces a `ShotRequest` with `ShotShape::Sweep { … }`.
- Throwables produce a `ShotRequest` with a `ProjectileSpec`.
- This means ballistics owns *all* spatial hit logic.
