# 5.4 `krit_movement`: character motion

> Part of the [crate architecture plan](../architecture.md) (§5). Section references (§) point to
> that document unless stated otherwise.

> This is the summary. The detailed design is in [`krit_movement-design.md`](krit_movement-design.md),
> which builds the controller up in parts (basic movement → jumping → crouching → other modes) and
> wins wherever the two disagree. "Design §n" below points into it.

**Owns:**
- **Configuration:** `CharacterController` (the opt-in marker, with an optional per-character
  profile override), `CharacterHull` (shape and standing/crouched/prone sizes), `MovementProfile`
  (an immutable asset: acceleration model, jump mode, step rule, slopes, timings; presets `Quake`,
  `Source`, `Arcade`, `Tactical`, `Immersive`) and `MovementStats` (speeds and accelerations,
  computed through `krit_stats`). Design §1.2, §8.
- **Motion state** (the rollback state): `KinematicState` (velocity, ground entity, tick timers, mode
  data) and `Stamina`. `MovementState` and `Stance` are also motion state but are defined in
  `krit_core` so other crates can read them. Design §1.3, §7.1.
- **Per-tick intermediate data:** `MoveCommand` (the gated, world-space wish), `BodyPose` (working
  copy of the body position), `Surroundings` (ground, water, ladder, stuck, measured at the start of
  the tick) and `StepEvents` (what happened, turned into facts by Report). Design §1.4 says what each
  one holds and why it exists.
- **The Aim stage:** `ViewAngles = intent.aim + ViewAngleOffset` (yaw wrapping; pitch saturating,
  then clamped to `±PITCH_LIMIT`; §6.2.1), recomputed from scratch every tick, plus `AimRecoil`
  recovery. `AimRecoil` is never folded into `ViewAngles`; `AimRay` adds it. The stage writes neither
  `LocalAim` nor `intent.aim` (§6.2). Design §6.

**The collide-and-slide algorithm is owned here**, not in the backend (P9). It is a port of the
Quake/Source `TryPlayerMove` / `ClipVelocity` / `StepMove` / `StayOnGround` family (and of
bevy_ahoy's version of it) written against `CollisionQuery`.

**Stages** (`MovementSystems`, public, in this order inside `KritSimSystems::Movement`):
Prepare (build `MoveCommand`, apply `ExternalImpulse`) → Sense (copy the body position, ride movers,
depenetrate, probe ground and contents) → Transition (enter/leave special modes) → Step (the pure
step) → Apply (write the backend body) → Report (read models and facts). Design §1.5.

**The step order is part of the contract**, so replacement crates can match it (design §4.3):

> stance → jump check → friction (skipped on the jump tick if the profile says so) → accelerate →
> first half of gravity → slide/step/snap → re-probe ground → second half of gravity → settle
> `Ground`/`Air` and detect landing → timers.

```rust
fn step(world: &dyn CollisionQuery, tuning: &Tuning, command: &MoveCommand,
        sensed: &Surroundings, motion: &mut Motion, events: &mut StepEvents, dt: f32)
```

is pure (no ECS access), which makes it unit-testable and rollback-safe. Unit tests check known
numbers: jump apex ≈ `v²/2g`, top ground speed converging to the profile cap, and positive strafe
gain only when `air_wishspeed_cap` is set.

The API has three extension tiers, following the Unity Character Controller package (design §11):
1. **Library steps** (`ground_probe`, `slide`, `step_up`, `snap_down`, `depenetrate`, …) that a
   custom controller can call;
2. **Custom modes** (`MovementState::Custom(id)` plus the game's own Transition and Step systems) and
   grounding rules from `SurfaceInfo` flags;
3. **A velocity slot** (`VelocityOverride`) the game may write for one-off motion.

**Consumes:**
- `CharacterIntent` (move, buttons, aim) and `ButtonEdges`;
- `ActionGates`;
- `MovementStats` modifiers from equipment;
- `ExternalImpulse` (knockback from explosions, jump pads);
- `MoverDelta` from `KritSimSystems::Movers` (moving platforms; platform yaw is added to
  `ViewAngleOffset`).

**Produces:**
- position and velocity, which the backend writes to its own body (avian `Position` and
  `LinearVelocity` on a kinematic body with `CustomPositionIntegration`). Movement does
  **not** write `Transform`; that is presentation, interpolated;
- `ViewAngles` (yaw/pitch, with pitch clamped);
- `MovementState` and `Stance` (core types);
- `Locomotion` (core read model: velocity, grounded, sprinting, water level, step height,
  footsteps), `EyeHeight` and `MovementSpreadFactor` (0..1, read by weapons);
- `ActionGates` contributions (no ADS while sprinting or on a ladder);
- `Jumped`, `Landed { fall_speed }`, `MovementStateChanged` and `StanceChanged` facts;
- `CharacterPush { other, impulse }` for dynamic bodies the character walks into;
- `NoiseEvent` for footsteps, jumps and landings;
- a `DamageRequest` for fall damage (type `Fall`).

**Plugins:** `MovementCore`, `MovementLook`, `MovementCrouch`, `MovementProne`, `MovementSprint`,
`MovementStamina`, `MovementSlide`, `MovementLadder`, `MovementSwim`, `MovementMantle`, `FallDamage`,
`MovementDebug`, grouped in `MovementPlugins<B: PhysicsBackend>`. Each can be disabled on its own.

**Customization:**
- pick a profile;
- disable features;
- add custom modes, or call the library steps from your own controller;
- or replace the crate. Anything that writes body position, `ViewAngles`, `EyeHeight`,
  `MovementSpreadFactor` and `Locomotion` satisfies the boundary. For M1, `krit_ahoy` is such an
  adapter (see §9, §10).
