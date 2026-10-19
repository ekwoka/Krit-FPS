# 5.4 `krit_movement`: character motion

> Part of the [crate architecture plan](../architecture.md) (§5). Section references (§) point to
> that document unless stated otherwise.

> The detailed design (tick-by-tick algorithm, formulas, states, profiles, extension tiers, rollback
> rules and tests) is in [`krit_movement-design.md`](krit_movement-design.md). Its §17 lists the
> changes it proposes to this section and to `krit_core`.

**Owns:**
- `CharacterController` (tuning parameters, hull shape: capsule or cylinder);
- `MovementProfile` (an asset or component) with explicit, named parameters: gravity, ground and air
  acceleration, `air_wishspeed_cap`, friction, stop speed, `accel_model`
  (`QuakeProjection | Proper | Arcade`), jump speed and `jump_mode`
  (`RequireRelease | AutoHop | Buffered`), `skip_friction_on_jump_tick`, step height and
  `step_rule`, max walkable slope, ground snap distance, crouch timings, `crouch_in_air`
  (`ShrinkTop | TuckLegs`) and knockback friction lock. Presets: `Quake`, `Source`, `Arcade`,
  `Tactical`, `Immersive`. Reference values are in
  [`research/source-quake-lineage.md` §7.2](../research/source-quake-lineage.md);
- `MovementState`, the state-machine enum (`Ground`, `Air`, `Crouch`, `Slide`, `Prone`, `Ladder`,
  `Swim`, `Mantle`, `Attached { seat }`, `Noclip`), extensible through a `CustomMovementState` hook;
- `KinematicState` (velocity, grounded, ground normal, ground entity, `last_step_height`, timers such
  as coyote time);
- `Stamina`;
- `MovementStats`, computed through `krit_stats`;
- capsule height changes;
- the Aim stage: `ViewAngles = intent.aim + ViewAngleOffset` (yaw wrapping; pitch saturating, then
  clamped to `±PITCH_LIMIT`; §6.2.1), recomputed from scratch every tick, plus `AimRecoil` recovery. `AimRecoil` is never folded into
  `ViewAngles`; `AimRay` adds it. The stage writes neither `LocalAim` nor `intent.aim` (§6.2).

**The collide-and-slide algorithm is owned here**, not in the backend (P9). It is a port of the
Quake/Source `TryPlayerMove` / `ClipVelocity` / `StepMove` / `StayOnGround` family (and of
bevy_ahoy's version of it) written against `CollisionQuery`. The per-tick order is part of the
contract, so replacement crates can match it:

> jump check → friction (skipped on the jump tick if the profile says so) → accelerate → slide/step
> → categorize ground → second half of gravity → fall check.

The API has three tiers, following the Unity Character Controller package:
1. **Library steps** (`ground_probe`, `slide`, `step_up`, `snap_down`, `depenetrate`) that a custom
   controller can call;
2. **Policy hooks** (`CustomMovementState`, `GroundingPolicy`) for game rules;
3. **A velocity slot** the game may write before the slide, for fully custom motion.

`fn step(state, intent, stats, &impl CollisionQuery, dt) -> (state, MoveOutput)` remains pure, which
makes it unit-testable and rollback-safe. Unit tests check known numbers: jump apex ≈ `v²/2g`, top
ground speed converging to the profile cap, and positive strafe gain only when `air_wishspeed_cap`
is set.

**Consumes:**
- `CharacterIntent`: move, jump, crouch, sprint and aim;
- `ActionGates`;
- `MovementStats` modifiers from equipment;
- `ExternalImpulse` (knockback from explosions, jump pads);
- mover velocities from `KritSimSystems::Movers` (moving platforms; platform yaw is added to
  `ViewAngleOffset`).

**Produces:**
- position and velocity, which the backend writes to its own body (avian `Position` /
  `LinearVelocity`). Movement does **not** write `Transform`; that is presentation, interpolated;
- `ViewAngles` (yaw/pitch, with pitch clamped);
- `MovementSpreadFactor`, a 0..1 value that weapons read;
- `ActionGates` contributions (no ADS while sprinting or on a ladder);
- `Landed { fall_speed }`, `Jumped` and `StateChanged` facts;
- `CharacterPush { other, impulse }` for dynamic bodies the character walks into;
- `NoiseEvent` for footsteps and landings;
- a `DamageRequest` for fall damage (type `Fall`).

**Plugins:** `MovementCore`, `MovementLook`, `MovementCrouch`, `MovementSprint`, `MovementSlide`,
`MovementLadder`, `MovementSwim`, `MovementMantle`, `MovementStamina`, `FallDamage`. Each can be
disabled on its own.

**Customization:**
- pick a profile;
- disable features;
- add custom states with a hook trait, or call the library steps from your own controller;
- or replace the crate. Anything that writes body position, `ViewAngles` and `MovementSpreadFactor`
  satisfies the boundary. For M1, `krit_ahoy` is such an adapter (see §9, §10).
