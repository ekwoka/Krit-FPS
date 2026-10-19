# `krit_movement`: detailed design

> Status: **proposal**, expanding [§5.4](krit_movement.md).
> §5.4 remains the summary. This document describes how the crate works tick by tick: which components
> it reads and writes, the exact order of the step, the formulas, the states, and the contracts a
> replacement crate has to meet. Where it changes or adds to a contract in `architecture.md`, the
> change is listed in §17.
>
> Units are metres, seconds and radians unless stated. +Y is up and −Z is forward, as in Bevy.
> Reference values are converted from Quake/Source units at 0.0254 m/u; their sources are in
> [`research/source-quake-lineage.md`](../research/source-quake-lineage.md) §3.2 and §7.2.

## 1. Scope

`krit_movement` turns a Character's `CharacterIntent` into a new body position and velocity once per
simulation tick, and computes the Character's facing (`ViewAngles`). It is a **logic-tier** crate
(§3.2): no renderer, no physics engine, no BEI.

**It does:**
- the aim stage (`ViewAngles` from intent + offset, `AimRecoil` decay);
- the kinematic character controller (KCC): ground detection, friction, acceleration, gravity,
  collide-and-slide, step-up, ground snap and depenetration;
- the movement state machine (ground, air, slide, ladder, swim, mantle, attached, noclip) and stance
  (standing, crouched, prone), including hull size changes and eye height;
- jump rules, coyote time and jump buffering;
- stamina;
- the read models and facts other crates consume (`Locomotion`, `MovementSpreadFactor`, `Landed`, …);
- fall damage *requests*.

**It does not:**
- write `Transform`. The backend owns the body's canonical pose (P9, §6.5 of the ecosystem research);
- talk to avian. All world access goes through `krit_core::physics::CollisionQuery`;
- smooth anything for display. Step smoothing, crouch eye smoothing, head bob and landing dips are
  `krit_kamera` effects driven by the read models below;
- apply damage. Fall damage is a `DamageRequest`; `krit_damage` decides what happens to it;
- push dynamic bodies. It *requests* pushes with `CharacterPush`; the backend applies them.

## 2. Components on the Character

`#[require]` on `CharacterController` inserts everything below with defaults, so spawning a working
character is `(Character, CharacterController::default(), MovementProfileHandle(handle))`.

| Component | Crate | Holds | Written by | `SimStateClass` |
|---|---|---|---|---|
| `CharacterController` | movement | Marker plus per-character overrides (`profile_override: Option<MovementProfile>`) | game / spawn | `Replicated` |
| `MovementProfileHandle` | movement | `Handle<MovementProfile>`; the tuning asset (§11) | game / archetype | `Replicated` |
| `CharacterHull` | movement | Hull shape and sizes (§7) | game / archetype | `Replicated` |
| `KinematicState` | movement | Velocity, ground entity, timers, stance progress (§2.1) | movement only | `OwnerPredicted` |
| `MovementState` | **core** | The current mode (§6) | movement only | `OwnerPredicted` |
| `Stance` | **core** | `Standing`, `Crouched`, `Prone` | movement only | `OwnerPredicted` |
| `Stamina` | movement | `current`, `regen_delay_ticks` | movement only | `OwnerPredicted` |
| `BaseMovementStats` / `MovementStats` | movement | Speeds, accelerations, jump speed (§11.2) | `krit_stats` recompute | `Derived` |
| `ViewAngles` | core | Actual facing | movement's aim stage only | `OwnerPredicted`, interpolated for others |
| `ViewAngleOffset` | core | Sim-imposed facing (spawn, teleport, platforms) | any sim system | `OwnerPredicted` |
| `AimRecoil` | core | Shot-affecting recoil | weapons add, aim stage decays | `OwnerPredicted` |
| `EyeHeight` | core | Eye height above the body origin | movement only | `Derived` |
| `Locomotion` | **core** | Read model for other crates (§12) | movement only | `Derived` |
| `MovementSpreadFactor` | core | 0..1 movement inaccuracy for weapons | movement only | `Derived` |
| `BodyPose` | movement | Position copied from the backend at the start of the step | movement's Sense stage | `Derived` (never replicated) |
| `MoveOutput` | movement | Position, velocity and hull the step wants the backend to apply | movement's Step stage | `Derived` |
| `VelocityOverride` | movement | Optional velocity a game writes before the step (§14, tier 3) | game | game's choice |

`MovementState` and `Stance` move to `krit_core` because `krit_kamera`, `krit_weapons` and
`krit_feedback` read them and may only depend on core (§3, the one hard rule). `KinematicState` stays
private to `krit_movement`; other crates read the `Locomotion` summary instead (§12), which also
means a replacement crate only has to fill one small read model.

### 2.1 `KinematicState`

```rust
#[derive(Component, Clone, Default, Reflect, PartialEq, MapEntities)]
#[reflect(SimState)]
pub struct KinematicState {
    /// World-space velocity. While grounded on a mover it is *relative* to the mover (§10.2).
    pub velocity: Vec3,
    /// The entity stood on at the end of the last tick, if any.
    #[entities] pub ground: Option<Entity>,
    /// Ticks since the character was last grounded (0 while grounded). Drives coyote time.
    pub ticks_since_grounded: u16,
    /// Ticks left in the jump buffer (0 = no buffered jump).
    pub jump_buffer: u16,
    /// Set when a jump was taken and the button has not been released yet (`RequireRelease`).
    pub jump_held: bool,
    /// Ticks left during which ground friction is skipped (knockback, Q3 PMF_TIME_KNOCKBACK).
    pub friction_lock: u16,
    /// 0 = standing, 1 = fully crouched. Prone uses its own value (§7).
    pub stance_progress: u16,
    /// Distance walked since the last footstep (§12.2).
    pub stride: f32,
    /// Cumulative footstep counter, for replay-safe cosmetics (§8 of architecture.md).
    pub footsteps: u32,
    /// Vertical distance moved by the last step-up or step-down, for camera smoothing.
    pub last_step_height: f32,
    /// Per-state data (slide timer, mantle path, ladder normal), see §6.
    pub mode_data: ModeData,
}
```

Every timer is a **tick count**, never a `Stopwatch` or seconds accumulator. Profiles are written in
seconds and converted to ticks once, when the profile is loaded or the tick rate changes
(`ticks = ceil(seconds × tick_hz)`). This keeps the step free of float time accumulation and makes
rollback restore timers exactly. bevy_ahoy's frame-time stopwatches are the counter-example (see
[`research/bevy-ecosystem.md`](../research/bevy-ecosystem.md) §6.3).

Ground normal, water level and "is touching a ladder" are **not** stored. They are recomputed from the
world at the start of every tick (§4, stage 0), as Quake 3's `PM_GroundTrace` and Source's
`CategorizePosition` do. That keeps the rollback state to position, velocity, the ground entity and
the timers.

## 3. Schedule

### 3.1 Where the systems run

All of it runs in `KritSimPhase::PrePhysics`, which `krit_avian` places before `PhysicsSystems::First`.

```
KritSimSystems::Intent      (core)   edges, gates, forced/disabled buttons
KritSimSystems::Spawn       (match)
KritSimSystems::Movers      (game / core helpers)   moving platforms write MoverDelta     ← proposed: before Aim (§10.2)
KritSimSystems::Aim         MovementLook
   AimSystems::Ride           platform yaw → ViewAngleOffset
   AimSystems::Compose        ViewAngles = clamp(intent.aim + ViewAngleOffset)
   AimSystems::Recoil         AimRecoil decay
KritSimSystems::Movement    MovementCore and feature plugins
   MovementSystems::Prepare   ExternalImpulse, VelocityOverride, stamina regen, profile → ticks
   MovementSystems::Sense     BodyPose from the backend
   MovementSystems::Transition  state and stance changes (built-in, then game systems)
   MovementSystems::Step      the KCC step (§4), one call per character
   MovementSystems::Apply     MoveOutput → backend body (position, velocity, yaw rotation, hull)
   MovementSystems::Report    Locomotion, EyeHeight, MovementSpreadFactor, facts, noise, fall damage
```

All `AimSystems` and `MovementSystems` sets are public (P10), so a game can remove a stage or slot a
system between two stages.

### 3.2 How a backend plugs in without `krit_avian` depending on `krit_movement`

Backends depend only on `krit_core` (§3.1 of the architecture). So `krit_movement` cannot ask
`krit_avian` to put systems in `MovementSystems`, and `krit_avian` cannot name movement types.
Instead `krit_core::physics` defines a backend trait. The movement plugin is generic over it and
installs the Sense, Step and Apply systems itself:

```rust
// krit_core::physics (sketch: exact SystemParam bounds are an implementation detail)
pub trait PhysicsBackend: Send + Sync + 'static {
    /// Read-only world queries (shape casts, contents, surfaces). Implements CollisionQuery.
    type Query: ReadOnlySystemParam;
    /// Reads and writes the canonical pose and velocity of character bodies.
    type Bodies: SystemParam;
}

// krit_movement
pub struct MovementPlugins<B: PhysicsBackend> { /* … */ }

// krit_avian
pub struct Avian;
impl PhysicsBackend for Avian { type Query = AvianCollisionQuery<'static, 'static>; type Bodies = AvianBodies<'static, 'static>; }

// krit_fps (facade), with the `avian` feature
app.add_plugins(MovementPlugins::<krit_avian::Avian>::default());
```

The world query and the body writer are separate system params, in separate systems (Step reads the
world; Apply writes bodies), so a backend's spatial query never conflicts with mutable access to
its own position components. `krit_ballistics` follows the same pattern with `SpatialQueryBackend`.

## 4. The per-tick step

`MovementSystems::Step` calls one pure function per character. It has no ECS access; everything it
needs is passed in, so the same function serves the live game, unit tests, rollback resimulation and
a server stepping bots.

```rust
pub fn step(
    ctx: &StepContext,            // profile (in ticks), MovementStats, hull, gated intent + edges, dt
    state: &mut MoveState,        // &mut KinematicState, MovementState, Stance, Stamina
    pose: &mut Vec3,              // body position, from BodyPose
    world: &dyn CollisionQuery,   // object-safe, so hooks and tests can pass any implementation
) -> StepReport                   // facts, pushes, noise; consumed by the Report stage
```

The order inside the step is part of the contract. A replacement controller that wants the same feel
must match it, because feel lives in ordering, not constants (lineage pitfall 3: bunny-hopping only
works because jump comes before friction).

**Stage 0: prelude (rebuild derived facts about the world)**
1. **Ride.** If `state.ground` is a mover, add its `MoverDelta.translation` for this tick to `pose`
   (§10.2).
2. **Depenetrate.** `world.depenetrate(hull, pose)`. If the hull is still embedded after
   `max_depenetration_iterations`, set `Locomotion.stuck` and skip collision response this tick
   instead of tunnelling (Source's stuck handling, simplified).
3. **Categorize ground** (§5.5): shape cast down by `ground_probe_distance`. Gives `grounded`, the
   ground normal and the ground entity.
4. **Contents.** `world.contents()` at feet, waist and eye height gives `WaterLevel` and ladder
   contact.

**Stage 1: mode dispatch.** Transitions have already run (`MovementSystems::Transition`). If the
state is `Ladder`, `Swim`, `Mantle`, `Attached`, `Noclip` or `Custom`, run that mode's step (§6)
and skip to stage 3. `Ground`, `Air` and `Slide` continue below.

**Stage 2: walk/air move (the Quake/Source core)**
1. **Jump check** (§8). On a successful jump: set the vertical velocity, clear `grounded` and the
   ground entity, and mark the tick as a jump tick.
2. **Friction** (§5.2), only if grounded, not a jump tick with `skip_friction_on_jump_tick`, and
   `friction_lock == 0`.
3. **Wish velocity.** `wishdir`, `wishspeed` from `move_axis`, yaw and the state's speed (§5.1).
   If `VelocityOverride` is present, use it instead of steps 3–4 (§14).
4. **Accelerate** (§5.3) with the ground or air parameters.
5. **First half of gravity**, if not grounded: `v.y −= g·dt/2`.
6. **Move.** Grounded: `step_move` (§5.4–5.6), then `snap_down` unless this was a jump tick.
   Airborne: `slide`.
7. **Categorize ground** again from the new position.
8. **Second half of gravity**, if still not grounded. If grounded, clip `v` against the ground plane
   instead.

**Stage 3: post-move**
1. **Fall check.** If the character was airborne at the start of the tick and is grounded now, emit
   `Landed { fall_speed }` with `fall_speed = −v.y` measured *before* stage 2.7 clipped it, and,
   with `FallDamage`, a `DamageRequest` (§12.3).
2. **Leaving a mover.** If the character was on a mover and is no longer grounded, add the mover's
   velocity to `v` (inherit platform velocity, §10.2).
3. **Timers.** Advance `ticks_since_grounded`, `jump_buffer`, `friction_lock`, the stride counter and
   mode timers.
4. Write `MoveOutput { position, velocity, hull }`.

The two gravity halves make the trajectory exact for constant gravity: over one tick the body moves
`(v − g·dt/2)·dt`, which is the analytic displacement. A jump at speed `v₀` peaks at `v₀²/2g` to
within the sampling error of one tick (≤ `g·dt²/8`), whatever the tick rate. This is the
"second half of gravity" in §5.4's order; the first half sits immediately before the move.

## 5. The core algorithms

### 5.1 Wish direction and speed

```
yaw     = ViewAngles.yaw as radians
forward = (−sin yaw, 0, −cos yaw)
right   = ( cos yaw, 0, −sin yaw)
wish    = right·move_axis.x + forward·move_axis.y          // move_axis.y > 0 is forward
wishdir = normalize_or_zero(wish)
wishspeed = |move_axis| (≤ 1) × state_speed
```

`state_speed` comes from `MovementStats`: `walk_speed`, `sprint_speed`, `crouch_speed` or
`prone_speed`, chosen from stance and sprint. Pitch does not affect ground or air movement; it is
used only by `Swim`, `Ladder` and `Noclip`. On the ground `wishdir` is used as a horizontal direction
and slopes are handled by the slide. Analogue sticks produce slower walking for free because
`|move_axis|` scales `wishspeed`.

If the `MOVE` gate is set, `move_axis` is treated as zero: gravity, friction and sliding still run.

### 5.2 Friction

Quake/Source ground friction, applied to horizontal speed only:

```
speed   = |v.xz|
control = max(speed, stop_speed)                  // so slow movement stops in finite time
drop    = control × friction × surface.friction × dt
v.xz   *= max(speed − drop, 0) / speed
```

`surface.friction` is the `SurfaceInfo` multiplier of the ground collider (1.0 by default; ice is
lower). `Slide` uses `slide_friction` instead of `friction`. `Swim` and `Ladder` have their own
damping (§6).

### 5.3 Acceleration models

`MovementProfile::accel_model` selects one of three. Each takes `(v, wishdir, wishspeed, accel,
cap)`; on the ground the cap is `wishspeed`, in the air it is `air_wishspeed_cap` (or none).

**`QuakeProjection`** (QuakeWorld, Quake 3, Source). Air-strafing and bunny-hop gain come from this:

```
current   = dot(v, wishdir)
add       = min(wishspeed, cap) − current          // capped speed in the projection test …
if add <= 0 { return }
accel_amt = min(accel × wishspeed × dt, add)       // … uncapped wishspeed in the step
v        += wishdir × accel_amt
```

Turning so that `wishdir` stays nearly perpendicular to `v` keeps `current` small, so `add` stays
positive while total speed grows. With no cap (Quake 3), gain relies on a low `air_accel` instead.

**`Proper`** (Carmack's `#else` branch in Q3 `PM_Accelerate`: "avoids strafe jump maxspeed bug, but
feels bad"). Accelerates the velocity *vector* toward `wishdir × wishspeed`, so strafing never exceeds
`wishspeed`:

```
push      = wishdir × wishspeed − v.xz
can_push  = accel × wishspeed × dt
v.xz     += push.clamp_length_max(can_push)
```

**`Arcade`**. Moves horizontal velocity toward the target at a fixed rate, with separate rates for
speeding up and stopping (`accel`, `decel`), no friction pass, and full air control scaled by
`air_control`. This is the feel of most modern console shooters and of the Bevy `fps_controller`.

### 5.4 Collide-and-slide

`slide(pose, v, dt, extra_planes)` is the `TryPlayerMove` / `PM_SlideMove` loop:

```
time_left = dt;  planes = extra_planes (the ground plane, if grounded)
for bump in 0..MAX_BUMPS (4):
    hit = world.cast_shape(hull, pose, v × time_left, filter)
    if hit.start_solid { v = 0; return Stuck }
    pose += v × time_left × hit.fraction, backed off by skin_width along the motion
    if no hit { break }
    time_left −= time_left × hit.fraction
    if planes.len() == MAX_CLIP_PLANES (5) { v = 0; break }
    planes.push(hit.normal)
    // find a velocity that slides along every touched plane
    for each plane i:
        clipped = clip(v, plane_i, overbounce)
        if clipped points into some other plane j:
            clipped = clip(clipped, plane_j, overbounce)
            if it still points into plane i: slide along the crease, dir = cross(n_i, n_j)
            if a third plane blocks too: v = 0, stop
    if dot(v, original_v) <= 0 { v = 0; break }      // never move backwards (Q3)
clip(v, n, overbounce) = v − n × dot(v, n) × overbounce   (only when dot(v, n) < 0)
```

`overbounce` is 1.0 (Source) or 1.001 (Quake 3's `OVERCLIP`); the tiny extra stops the character
grinding along a wall it is pressed into. `skin_width` (default 0.01 m) keeps the hull just outside
contact so the next cast doesn't start in solid.

Every hit records `(entity, normal, impact speed)` into the report, which feeds `KinematicContacts`
and `CharacterPush` (§10.3).

### 5.5 Ground categorization

```
hit = world.cast_shape(hull, pose, −Y × ground_probe_distance, filter)
grounded = hit.is_some()
       && hit.normal.y >= max_walk_slope_cos
       && v.y <= unground_speed                   // moving up fast = launched, not grounded
       && not a jump tick
```

- `ground_probe_distance` defaults to 0.05 m (Source traces 2 u). It is extended by a descending
  mover's downward speed × dt so characters don't lose contact with a lift going down (bevy_ahoy).
- `max_walk_slope_cos` defaults to 0.7 (≈45.6°). Steeper surfaces are not ground: the character
  slides down them, which is what makes surf ramps work when air acceleration is high enough.
- `unground_speed` defaults to 3.56 m/s (Source `NON_JUMP_VELOCITY`, 140 u/s). It stops jump pads,
  explosions and fast elevators from re-grounding the character on the tick they launch it.
- `SurfaceInfo.flags` can veto ground (`NOT_WALKABLE`) or step-up (`NO_STEP`) per collider. This is
  the data half of the `GroundingPolicy` hook in §5.4 of the architecture.

### 5.6 Step-up and ground snap

`step_move` is Source's `StepMove`, the variant bevy_ahoy also uses:

1. Save the start. Run `slide` flat; remember where it ended (`flat`).
2. From the start: cast up by `step_height`; from there `slide` horizontally; cast down by
   `step_height` (plus the probe distance).
3. Reject the stepped result if it landed on a non-walkable normal, or if there is less than
   `min_step_ledge_space` of floor ahead (stops "stepping" onto a ledge too thin to stand on).
4. `step_rule` decides:
   - `FartherHorizontal` (Source): keep whichever of `flat` and stepped went farther in XZ;
   - `WhenBlocked` (Quake 3): only try the stepped move if the flat move hit something.
5. Record `last_step_height = stepped.y − start.y` when the stepped result wins.

`snap_down` is Source's `StayOnGround`: when the character was grounded at the start of the tick,
did not jump, and is not grounded after the move, cast down by `ground_snap_distance` (default: step
height). If that hits walkable ground, move down onto it and record a negative `last_step_height`.
That keeps the character glued to stairs and downward slopes instead of skipping off them.

Step-up only runs while grounded, unless the profile sets `step_in_air` (useful for arcade feel,
off in all lineage presets).

## 6. Movement states

`MovementState` is the mode, which decides which physics runs. `Stance` (§7) is orthogonal: a
character can be crouched while in `Air` (crouch-jump) or in `Slide`. Sprinting is a flag on
`Locomotion`, not a state, because it only changes speed, gates and stamina.

```rust
// krit_core::locomotion
pub enum MovementState {
    Ground, Air, Slide, Ladder, Swim, Mantle,
    Attached { seat: Entity },
    Noclip,
    Custom(CustomMovementId),
}
```

(§5.4 of the architecture lists `Crouch` and `Prone` as states; here they are `Stance` values. See
§17.)

| State | Entered when | Physics | Leaves when | Gate contributions | Plugin |
|---|---|---|---|---|---|
| `Ground` | categorize finds walkable ground | §4 stage 2, ground params | ground lost → `Air` | – | `MovementCore` |
| `Air` | not grounded and no other mode | §4 stage 2, air params | grounded → `Ground` | – | `MovementCore` |
| `Slide` | crouch **pressed** while sprinting on the ground with `speed ≥ slide_min_speed` | ground move with `slide_friction`, no wish accel, `slide_boost` along `v` on entry; slopes accelerate | `speed < slide_exit_speed`, `slide_max_ticks`, crouch released, or jump (keeps speed) | `SPRINT` | `MovementSlide` |
| `Ladder` | ladder contact and moving into it (`dot(wishdir, −ladder_normal) > 0`), or airborne and touching it | §6.1 | input away from it, reached the top, or grounded while moving down | `ADS`, optionally `FIRE` | `MovementLadder` |
| `Swim` | `WaterLevel ≥ Waist` | §6.2 | `WaterLevel < Waist` | `SPRINT` | `MovementSwim` |
| `Mantle` | jump pressed (or held, with `auto_mantle`) with a valid ledge ahead (§6.3) | scripted path, no collision | path finished | `FIRE`, `ADS`, `SWITCH` | `MovementMantle` |
| `Attached` | an `AttachRequest { seat }` (seats, turrets, vehicles, cutscenes) | none; pose follows the seat | `DetachRequest` | per seat | `MovementCore` |
| `Noclip` | `NoclipRequest`, only on entities with `NoclipAllowed` (authority-granted) | §6.4 | `NoclipRequest` again | – | `MovementCore` |
| `Custom(id)` | a game system in `Transition` | the game's own step (§14) | the game's own system | game's choice | game |

**Transition order.** Built-in transitions run in one system in `MovementSystems::Transition`, in
priority order: `Attached` > `Noclip` > `Mantle` (in progress) > `Ladder` > `Swim` > `Slide` >
`Ground`/`Air`. Game systems placed `.after(MovementTransitionSystems::Builtin)` in the same set can
override the result. Every change emits `MovementStateChanged { from, to }`.

**Sprint** is on when `SPRINT` is held, the `SPRINT` gate is clear, the character is on the ground
(or in the air with `sprint_in_air`), `move_axis.y ≥ sprint_min_forward` (default 0.5: no sprinting
sideways or backwards), stance is standing, and stamina is available. Movement contributes the
`ADS` gate while sprinting. The cancel rules ("firing cancels sprint") live in `ActionRules` (§5.6),
not here.

### 6.1 Ladder

- Ladders are found by `ContentsFlags::LADDER` in a thin probe in front of the hull, or by a
  `Ladder { normal }` component on a sensor volume (for levels without contents brushes).
- Movement follows Source's `LadderMove`: forward input climbs up when looking up or level and down
  when looking down past `ladder_look_down_pitch`; strafe moves along the ladder plane; there is no
  gravity and no friction pass. Speed is `ladder_speed`.
- Jump while on a ladder pushes off along the ladder normal at `ladder_jump_speed`.
- Reaching the top (no ladder contact at the feet while moving up) gives a small upward-and-forward
  pop so the character ends on the floor above, rather than catching on the lip.

### 6.2 Swim

- `WaterLevel::{None, Feet, Waist, Eyes}` comes from `contents(WATER)` at three points (feet, hull
  middle, eye), as the lineage's `WL_Feet/Waist/Eyes`.
- `wishdir` uses full view direction including pitch; `JUMP` held adds upward wish (swim up),
  `CROUCH` held adds downward wish. `swim_speed`, `water_accel` and `water_friction` replace the
  ground values. With no input the character sinks at `water_sink_speed`, or floats at the surface
  if `buoyant`.
- At `Waist` with a wall ahead and jump held, an optional water jump launches the character out of
  the water (Source `WaterJump`).
- `Locomotion.water_level` lets `krit_kamera` apply underwater effects and lets a game run oxygen.

### 6.3 Mantle

Detection runs only when a jump would otherwise happen (or every tick in the air with
`auto_mantle`), and uses three casts:

1. **Wall:** cast the hull forward by `mantle_reach` along the horizontal view direction. A hit with
   a near-horizontal normal is a candidate wall.
2. **Top:** cast a thin shape down from `max_mantle_height` above the feet, just past the wall. A
   walkable hit between `min_mantle_height` and `max_mantle_height` above the feet is the ledge top.
3. **Clearance:** cast the standing hull (falling back to the crouched hull, which forces `Crouched`)
   at the ledge top. It must fit.

On success the state becomes `Mantle` and `mode_data` stores `{ start, top, ticks, duration }`. The
pose then follows a fixed two-segment path (up to the ledge height, then forward onto it) as a
function of `ticks / duration`, so it is deterministic and rollback-safe without storing anything
else. Collision is not checked during the path because clearance was checked up front. The state
ends in `Ground`. `mantle_duration` comes from the profile, never from an animation length (§8 of the
architecture: the server does not animate).

### 6.4 Noclip

Velocity accelerates toward `wishdir × noclip_speed` using the full view direction (pitch included),
with `JUMP`/`CROUCH` for up/down. No collision, no gravity, no ground. Fall damage and footsteps are
suppressed. It is a debugging and level-editing tool, so it is authority-granted (`NoclipAllowed`) and
`krit_net` treats a client's noclip request like any other intent.

## 7. Hull, stance and eye height

```rust
pub struct CharacterHull {
    pub shape: HullShape,         // Cylinder (default) | Capsule | Box
    pub radius: f32,              // box: half-extent
    pub standing_height: f32,
    pub crouched_height: f32,
    pub prone_height: f32,
    pub eye_below_top: f32,       // eye distance below the top of the current hull
    pub skin_width: f32,
}
```

- **Shape.** The default is a cylinder, because parry's capsule casts are unreliable at edges
  (bevy_ahoy's note) and step-up assumes a flat bottom. A box gives GoldSrc feel. This is still
  listed as open in §12 of the architecture.
- **Origin.** The body origin is at the **feet**, so a hull height change never moves the feet and
  never needs a position correction while grounded.
- **Crouching down.** `stance_progress` counts up over `crouch_ticks`; `EyeHeight` follows it
  linearly. The *collision* hull switches to crouched only when progress completes (Source
  `FinishDuck`). Until then the character keeps the larger hull, so it never shrinks into a state it
  can't get out of. Crouch speed applies from the switch onward.
- **Standing up.** First cast the standing hull at the current pose. If blocked, stay crouched (and
  keep `wants_stand` so the character stands as soon as there is room). If clear, switch the hull
  back immediately and let the eye rise over `uncrouch_ticks`.
- **In the air**, `crouch_in_air` decides:
  - `ShrinkTop` (Quake 3): the top comes down, the feet stay put;
  - `TuckLegs` (Source): the feet come up by `standing − crouched` and the origin moves up with
    them, so the character clears obstacles that much higher (crouch-jumping). Before moving the
    origin, cast to check the space above is free; if not, fall back to `ShrinkTop`.
- **Prone** works the same way with `prone_height` and `prone_ticks`, and adds `prone_speed` and a
  `FIRE` gate during the transition. A prone hull is still upright (a short cylinder); a lying-down
  hull that rotates with yaw is left to a custom state.
- `EyeHeight = current_visual_height − eye_below_top`, where the visual height interpolates between
  standing and crouched with `stance_progress`. It is `Derived`: recomputed from `Stance` and
  `stance_progress` every tick, so it never needs replicating.

The backend reads `MoveOutput.hull` in Apply and swaps the body's collider when it changes. Hurtboxes
are separate colliders on their own layer (§6.3 of the architecture) and follow `Stance` through
`HurtboxPose`, not the movement hull.

## 8. Jumping

Jumping is evaluated in stage 2.1 of the step.

```
can_jump = (grounded || ticks_since_grounded <= coyote_ticks)
        && !gates.contains(MOVE)
        && stance allows it (not prone; crouched only if `crouch_jump`)
        && stamina >= jump_stamina_cost

held = buttons.contains(JUMP)
wants = match jump_mode {
    RequireRelease => held && !state.jump_held,                  // Quake 3
    PressEdge      => edges.pressed.contains(JUMP),              // Source
    AutoHop        => held,
    Buffered       => edges.pressed.contains(JUMP) || state.jump_buffer > 0,
}
```

- **`RequireRelease`** (Quake 3 `PMF_JUMP_HELD`, "don't pogo stick"): a jump needs the button to
  have been released since the last *jump*. `jump_held` is set by a jump and cleared when the button
  is up. Releasing in the air and holding again jumps on the landing tick.
- **`PressEdge`** (Source, `m_nOldButtons & IN_JUMP`): the press must happen on the tick the jump is
  taken. Holding jump through a landing never jumps, so bunny-hopping needs a press timed to the
  landing tick (or a scroll-wheel bind).
- **`AutoHop`**: holding jump re-jumps on every landing (CS:GO-style servers, many modern games).
- **`Buffered { window }`**: a press up to `window` before landing is stored in `jump_buffer` and
  taken on the landing tick.
- **Coyote time** (`coyote_time`, default 0 in lineage presets, ~0.1 s in `Arcade`): a jump is still
  allowed for a few ticks after walking off a ledge. Not after a jump; `ticks_since_grounded` is set
  past the window when a jump is taken.
- **Jump velocity**: `jump_impulse` is `Set` (Quake 3, Source: `v.y = max(v.y, jump_speed)`) or `Add`
  (QuakeWorld: `v.y += jump_speed`). Profiles can give `jump_height` instead, and the loader derives
  `jump_speed = sqrt(2·g·h)`.
- **Bunny-hop control**: `skip_friction_on_jump_tick` (true in `Quake` and `Source`) lets a perfectly
  timed jump keep all speed. Setting it false is the simplest anti-bhop rule; a `bhop_speed_cap`
  (scale `v.xz` down to `cap` on jump, CS:GO style) is the other.
- **No hidden boosts.** There is no HL2-style forward jump boost in the core step. If a game wants one,
  it is a separate plugin that writes `ExternalImpulse`, capped along the velocity direction (lineage
  pitfall 11, accelerated back-hopping).
- On success: emit `Jumped`, add a `NoiseEvent`, spend stamina, set `jump_held`, clear `jump_buffer`.

## 9. Aim stage (`MovementLook`)

Runs in `KritSimSystems::Aim`, before movement, because the step uses `ViewAngles.yaw`. It is the only
writer of `ViewAngles` (§6.2 of the architecture).

1. **Ride** (`AimSystems::Ride`): if the character's ground entity has a `MoverDelta` with a yaw
   rotation, add that yaw to `ViewAngleOffset` so the character turns with a rotating platform.
2. **Compose** (`AimSystems::Compose`):
   ```
   yaw   = intent.aim.yaw.wrapping_add(offset.yaw)
   pitch = intent.aim.pitch.saturating_add(offset.pitch).clamp(-PITCH_LIMIT, PITCH_LIMIT)
   ```
   then apply `AimLimits` if present (seats and turrets, §17), and write `ViewAngles`.
3. **`LOOK` gate.** While `LOOK` is blocked, the stage keeps `ViewAngles` where it was by rewriting
   `ViewAngleOffset = previous ViewAngles − intent.aim` every tick. When the gate clears, aim resumes
   from where it froze instead of snapping to wherever the mouse went. This is the same mechanism as
   forced facing, so it needs nothing new.
4. **Recoil** (`AimSystems::Recoil`): `AimRecoil` decays toward zero at the weapon's recovery rate,
   which weapons write into an `AimRecoilRecovery` component on the Character. Movement never
   inspects the weapon.
5. Optionally, as an anti-cheat sanity check on the authority, `max_turn_rate` clamps how far
   `ViewAngles.yaw` may change in one tick (networked-shooter research §6.1). Off by default.

The body's **rotation** is yaw only. Movement writes it to `MoveOutput` and the backend applies it in
Apply, so rollback restores rotation from `ViewAngles.yaw` alone (the DOTS "rebuild rotation from
yaw" rule).

## 10. External forces, movers and pushing

### 10.1 `ExternalImpulse`

```rust
// krit_core
#[derive(Component, Clone, Default, Reflect, PartialEq)]
pub struct ExternalImpulse {
    pub velocity: Vec3,        // summed velocity change to apply next step
    pub friction_lock: u16,    // ticks; the max of all contributions this tick
    pub mode: ImpulseMode,     // Add (knockback, explosions) | SetVertical (jump pads)
}
```

- Ballistics, damage (`DamagePipelineSystems::Knockback`) and game code (jump pads, wind) add to it.
  Those run post-physics, so an impulse written on tick N is applied by the movement step on tick
  N+1. That one-tick delay is deterministic, so client and server agree on it.
- `MovementSystems::Prepare` adds it to `KinematicState.velocity`, raises `friction_lock` to its
  value, and clears the component. A vertical component above `unground_speed` makes the character
  airborne in the same tick's categorize step.
- `friction_lock` is Quake 3's `PMF_TIME_KNOCKBACK`: rocket jumps and knockback are not eaten by
  ground friction on the next few ticks. Q3 locks for 50–200 ms depending on the knockback.
- Velocity is the character's; there is no mass. Damage scales knockback by its own rules before
  writing (`SelfDamagePolicy.knockback_scale`).

### 10.2 Moving platforms

- A mover (platform, elevator, door, train) is any entity a game moves in `KritSimSystems::Movers`.
  It records what it did this tick in `MoverDelta { translation, rotation }` and its velocity in the
  backend's body (`SurfaceInfo.linear_vel` / `angular_vel`).
- While grounded on a mover, the character is **carried**: stage 0.1 adds the mover's translation,
  plus the displacement from rotating the character's position about the mover's centre. The
  character's own `velocity` stays relative to the mover, so walking on a train feels like walking
  on the ground.
- Yaw is carried by the aim stage (§9.1), so the view turns with the platform.
- When the character leaves the mover by jumping or walking off, the mover's velocity at that point is
  added to `velocity` (`inherit_platform_velocity`, on by default), so jumping off a moving platform
  keeps its momentum.
- This needs `Movers` to run **before** `Aim` in the core set chain; the current order (`Aim` then
  `Movers`) applies platform yaw one tick late. See §17.

### 10.3 Contacts and pushing dynamic bodies

- Every hit in the slide is reported. `Report` writes the deduplicated list to `KinematicContacts`
  (`Derived`, a read model) so game code can react to "touched a hazard" without its own casts.
- For contacts with dynamic bodies, the step emits `CharacterPush { character, other, point,
  impulse }`, with `impulse = −normal × impact_speed × push_strength`. The backend applies it after
  its step. The character is not slowed by the body (it is kinematic); games that want heavier props
  to resist can give them a `NOT_PUSHABLE` surface flag, which makes them plain walls.
- Characters do not push each other. Character–character contact is resolved as a wall by the
  slide, so prediction never depends on another player's input.

## 11. Profiles and stats

### 11.1 Profile vs stats: the rule

- **`MovementProfile`** describes *how* the character moves: the acceleration model, jump mode, step
  rule, slopes, timings and the per-state parameters. It is an immutable asset (`assets` feature,
  RON) or an inline override on `CharacterController`, and nothing changes it at runtime.
- **`MovementStats`** are *how fast and how strong*, the numbers gameplay modifies: speeds,
  accelerations, jump speed, stamina. The profile provides the base values; `krit_stats`
  modifiers (item weight, perks, a slowing status effect, the ADS slowdown a weapon contributes
  while aiming) change them. Movement reads only the resulting `MovementStats`.

| `MovementStats` field | `StatId` | Base from profile |
|---|---|---|
| `walk_speed`, `sprint_speed`, `crouch_speed`, `prone_speed`, `swim_speed`, `ladder_speed` | `move.speed.*` | yes |
| `ground_accel`, `air_accel` | `move.accel.*` | yes |
| `jump_speed` | `move.jump` | yes (or derived from `jump_height`) |
| `gravity_scale` | `move.gravity_scale` | 1.0 |
| `max_stamina`, `stamina_regen` | `move.stamina.*` | yes |

Two items that each multiply speed by 0.9 give 0.81 (`krit_stats` multiplies; §5.2 of the
architecture).

### 11.2 Profile fields

| Group | Fields |
|---|---|
| Gravity and air | `gravity`, `air_wishspeed_cap: Option<f32>`, `air_control` (Arcade only) |
| Ground | `friction`, `stop_speed`, `accel_model`, `decel` (Arcade only) |
| Jump | `jump_mode`, `jump_impulse`, `jump_height` or base `jump_speed`, `skip_friction_on_jump_tick`, `coyote_time`, `bhop_speed_cap`, `crouch_jump` |
| Collision | `step_height`, `step_rule`, `step_in_air`, `min_step_ledge_space`, `max_walk_slope_cos`, `ground_probe_distance`, `ground_snap_distance`, `unground_speed`, `overbounce` |
| Stance | `crouch_time`, `uncrouch_time`, `prone_time`, `crouch_in_air` |
| Sprint and stamina | `sprint_min_forward`, `sprint_in_air`, `sprint_drain`, `jump_stamina_cost`, `stamina_regen_delay` |
| States | slide (`slide_min_speed`, `slide_exit_speed`, `slide_boost`, `slide_friction`, `slide_max_time`), ladder, swim, mantle (`mantle_reach`, `min/max_mantle_height`, `mantle_duration`, `auto_mantle`), noclip |
| Platforms and knockback | `inherit_platform_velocity`, `push_strength` |
| Fall | `fall_safe_speed`, `fall_fatal_speed`, `fall_damage_curve`, `land_effect_speed` |
| Spread | `spread_speed_ref`, `spread_air`, `spread_stance` (§12.1) |

### 11.3 Presets

`Quake` and `Source` use the verified lineage constants. The other three are starting points to tune;
their numbers are not from any reference.

| Field | `Quake` | `Source` | `Arcade` | `Tactical` | `Immersive` |
|---|---|---|---|---|---|
| `gravity` (m/s²) | 20.32 | 20.32 (HL2: 15.24) | ~20 | ~20 | ~18 |
| `walk_speed` (m/s) | 8.13 | 8.13 cap | ~7 | ~5, ADS lower | ~4.5 |
| `accel_model` | `QuakeProjection` | `QuakeProjection` | `Arcade` | `Proper` | `Arcade` |
| `ground_accel` | 10 | 10 | high | 10 | moderate |
| `air_accel` | 0.7 (QW) / 1.0 (Q3) | 10 | `air_control` ~0.3 | 1 | low |
| `air_wishspeed_cap` | 0.76 (QW) / none (Q3) | 0.76 | – | – | – |
| `friction` / `stop_speed` | 4 (QW) or 6 (Q3) / 2.54 | 4 / 2.54 | – (decel) | 6 / 2.54 | – (decel) |
| `jump_speed` (m/s) | 6.86 | 6.81 (HL2: 4.06) | from height ~1.2 m | low | low |
| `jump_mode` | `RequireRelease` | `PressEdge` | `Buffered`, coyote ~0.1 s | `RequireRelease` | `Buffered` |
| `skip_friction_on_jump_tick` | true | true | – | false | – |
| `step_height` (m) | 0.46 | 0.46 | 0.4 | 0.4 | 0.4 |
| `step_rule` | `WhenBlocked` | `FartherHorizontal` | `FartherHorizontal` | `FartherHorizontal` | `FartherHorizontal` |
| `max_walk_slope_cos` | 0.7 | 0.7 | 0.7 | 0.7 | 0.7 |
| `crouch_time` / `uncrouch_time` (s) | 0 / 0 | 0.4 / 0.2 (HL2) | 0.15 / 0.15 | 0.3 / 0.3 | 0.25 / 0.25 |
| `crouch_in_air` | `ShrinkTop` | `TuckLegs` | `TuckLegs` | `ShrinkTop` | `TuckLegs` |
| `overbounce` | 1.001 | 1.0 | 1.0 | 1.0 | 1.0 |
| `fall_safe` / `fall_fatal` (m/s) | events only | 14.7 / 26.0 | off | 12 / 22 | 12 / 22 |
| Plugins usually on | core, look, crouch | + ladder, swim, fall damage | + slide, mantle, sprint | + sprint, stamina, prone, fall damage | + ladder, swim, mantle, stamina |
| Hull (radius × standing / crouched, eye) | 0.38 × 1.42 / 1.02, eye 1.27 (Q3) | 0.41 × 1.83 / 0.91, eye 1.63 / 0.71 | 0.4 × 1.8 / 1.1 | 0.35 × 1.8 / 1.2 | 0.35 × 1.75 / 1.1 |

The tick rate is part of the feel: Quake friction is linear per tick, so 30 Hz and 128 Hz feel
slightly different with the same numbers. Presets are tuned and tested at the default tick rate.

## 12. Outputs and facts

### 12.1 Read models (components, `Derived`, rewritten every tick)

| Component | Contents | Read by |
|---|---|---|
| `Locomotion` (core) | `velocity`, `grounded`, `ground_normal`, `sprinting`, `water_level`, `last_step_height`, `footsteps`, `stuck` | kamera (bob, step smoothing, FOV), viewmodel (sway), feedback, AI, game code |
| `MovementState`, `Stance` (core) | §6, §7 | weapons (per-stance spread), kamera, hurtbox poses, animation |
| `EyeHeight` (core) | §7 | `AimRay`, kamera, interaction |
| `MovementSpreadFactor` (core) | 0..1 | weapons only |
| `KinematicContacts` (movement) | entities touched this tick | game code |

`MovementSpreadFactor` is computed as:

```
speed_term = clamp(|v.xz| / spread_speed_ref, 0, 1)     // spread_speed_ref defaults to walk_speed
air_term   = if grounded { 0 } else { spread_air }       // default 1.0
factor     = max(speed_term, air_term)
```

Stance is *not* folded in; weapons read `Stance` for per-stance base spread, so the two effects can be
tuned separately.

### 12.2 Facts

All facts go through core's `emit_fact`: a `Message` for sim consumers, and an `EntityEvent` on the
Character for game code and presentation, which is skipped while resimulating (§4.1–4.2 of the
architecture).

| Fact | When | Typical consumers |
|---|---|---|
| `Jumped { tick }` | a jump was taken | kamera, audio, AI |
| `Landed { tick, fall_speed, surface }` | airborne → grounded | kamera (landing dip above `land_effect_speed`), audio, fall damage |
| `MovementStateChanged { from, to }` | any state change | animation, game code |
| `StanceChanged { from, to }` | stance switch completed | hurtboxes, animation |
| `NoiseEvent` | footsteps, jumps, landings | AI, stealth |

**Footsteps** are simulated, not animated: `stride` accumulates horizontal distance while grounded,
and each time it passes the stance's stride length the step increments `footsteps` and emits a
`NoiseEvent` (quieter crouched, none prone or in noclip). Presentation plays sounds from the
cumulative `Locomotion.footsteps` counter, so rollback never doubles a footstep.

### 12.3 Fall damage (`FallDamage` plugin)

On `Landed`, if `fall_speed > fall_safe_speed`, send a `DamageRequest` with
`damage_type: Fall`, `amount = fall_damage_curve(fall_speed)` (by default linear, reaching
`MaxHealth`-scale lethal damage at `fall_fatal_speed`, Source's `DAMAGE_FOR_FALL_SPEED`), the
character as target, no instigator (`krit_damage`'s `LastHitBy` credits whoever knocked them off),
and the `NO_KNOCKBACK` flag. Every peer emits the request, but only the authority's `Damage` set
processes it (P12). Landing in water at `Waist` or deeper cancels fall damage.

## 13. Plugins

`MovementPlugins<B>` contains these, each disableable with `.disable::<X>()`:

| Plugin | Adds | Depends on |
|---|---|---|
| `MovementCore` | sets, `KinematicState`, Ground/Air/Attached/Noclip, collide-and-slide, step, snap, impulses, platforms, `Locomotion`, `MovementSpreadFactor`, facts | – |
| `MovementLook` | the aim stage (§9) | – |
| `MovementCrouch` | crouch stance and hull changes | core |
| `MovementProne` | prone stance | crouch |
| `MovementSprint` | sprint flag, gates | core |
| `MovementStamina` | `Stamina`, drain and regen; without it sprint is unlimited | sprint (optional) |
| `MovementSlide` | `Slide` | crouch, sprint |
| `MovementLadder` | `Ladder` | core |
| `MovementSwim` | `Swim`, `WaterLevel` | core |
| `MovementMantle` | `Mantle` | core |
| `FallDamage` | fall `DamageRequest`s | core |
| `MovementDebug` (feature `debug`) | gizmos for the hull, casts, ground normal and contacts | core |

Disabling a plugin removes its transitions, so a state it owns is never entered; profile fields it
uses are ignored.

## 14. Extending it: the three tiers

Following the Unity Character Controller split ([`research/unity-dots-online-fps.md`](../research/unity-dots-online-fps.md) §14):

**Tier 1: library steps.** Everything the built-in step uses is `pub` in `krit_movement::kcc`, takes
plain data and `&dyn CollisionQuery`, and has no ECS access:

```rust
pub fn depenetrate(world, hull, pose) -> DepenetrationResult;
pub fn ground_probe(world, hull, pose, v, cfg: &GroundConfig) -> GroundHit;
pub fn friction(v: Vec3, surface: &SurfaceInfo, p: &FrictionParams, dt) -> Vec3;
pub fn accelerate(v, wishdir, wishspeed, accel, cap: Option<f32>, model, dt) -> Vec3;
pub fn slide(world, hull, pose: &mut Vec3, v: &mut Vec3, dt, planes: &[Dir3], cfg) -> SlideResult;
pub fn step_up(world, hull, pose: &mut Vec3, v: &mut Vec3, dt, cfg) -> StepResult;
pub fn snap_down(world, hull, pose: &mut Vec3, cfg) -> Option<f32>;
pub fn wish_velocity(move_axis: Vec2, yaw: u32, speed: f32) -> (Dir3, f32);
```

A game writing its own controller (a hover vehicle, a wall-runner) calls these from its own system and
gets the same collision behaviour as the built-in one.

**Tier 2: custom states.** A game adds a state without forking:

```rust
const WALL_RUN: CustomMovementId = CustomMovementId(1);

// MovementSystems::Transition, after the built-in transitions
fn enter_wall_run(mut q: Query<(&mut MovementState, &Locomotion, &KinematicContacts, &CharacterIntent)>) {
    for (mut state, loco, contacts, intent) in &mut q {
        if *state == MovementState::Air && /* touching a wall, holding forward */ {
            *state = MovementState::Custom(WALL_RUN);
        }
    }
}

// MovementSystems::Step; the built-in step skips characters in Custom states
fn wall_run_step(world: CollisionParam<B>, mut q: Query<(&mut BodyPose, &mut KinematicState, &mut MoveOutput, &mut WallRunState, …)>) {
    // use kcc::slide, kcc::ground_probe … then write MoveOutput
}
```

A custom state's data lives in the game's own components (`WallRunState`), tagged
`#[reflect(SimState)]`, so it rolls back with everything else. This is why the extension point is
"a state id plus your own systems" rather than a `dyn` trait registry: a trait object could not read
the game's components, and its internal state would be invisible to rollback. The built-in Report
stage still runs for custom states, so `Locomotion`, facts and spread keep working.

**Tier 3: the velocity slot.** For one-off motion inside the built-in states, write
`VelocityOverride { velocity, mode: Replace | Add, ticks }` before `MovementSystems::Step`. The step
uses it in place of wish acceleration (§4 stage 2.3) and still slides, steps and grounds. Dashes,
grapple pulls and scripted shoves use this.

**`GroundingPolicy`.** Data in the profile (`max_walk_slope_cos`, `unground_speed`, probe distance)
plus per-collider `SurfaceInfo.flags` (`NOT_WALKABLE`, `NO_STEP`, `SLIPPERY`, `NOT_PUSHABLE`). Games
that need a per-surface rule beyond flags give the collider its own walkable limit in `SurfaceInfo`.

## 15. Networking and determinism

**State that is predicted and rolled back** (owner) and replicated (others):

| Rolled back | Recomputed every tick (never sent) |
|---|---|
| backend body position (avian `Position`) | `BodyPose`, `MoveOutput` |
| `KinematicState` (velocity, ground entity, timers, stance progress, mode data) | ground normal, water level, ladder contact |
| `MovementState`, `Stance`, `Stamina` | `EyeHeight`, `Locomotion`, `MovementSpreadFactor`, `KinematicContacts` |
| `ViewAngles`, `ViewAngleOffset`, `AimRecoil` | `MovementStats` (recomputed from modifiers) |

- Remote characters receive position, `ViewAngles`, `MovementState`, `Stance` and velocity,
  interpolated. They are not stepped from stale intents (`krit_net`'s `Simulate` filter).
- Every rolled-back component derives `PartialEq`; `KinematicState` and positions compare with a
  small tolerance so float noise in casts doesn't trigger needless rollbacks.
- `KinematicState.ground` holds an `Entity`, so it derives `MapEntities` (bevy_ahoy's state does
  not, which breaks replication).
- Nothing in the step reads `Time`, frame `dt`, `Entity` order or a resource of per-entity state.
  `dt` comes from `Time<Fixed>` via `StepContext`. Characters are stepped independently, so
  iteration order cannot change results.
- If `krit_net` quantizes replicated positions or velocities, `Report` applies the same quantization
  at the end of every step on every peer (lineage pitfall 1: Quake 3's `SnapVector`).
- Cross-platform bit-identical results also need deterministic trig and casts (avian's
  `enhanced-determinism`, `libm` for `sin`/`cos`). Without them, client and server on different
  platforms correct each other slightly more often; same-platform play is unaffected.
- Spawns and teleports set a `Discontinuity` marker for one tick, so lag compensation never rewinds
  across them and kamera snaps instead of smoothing (lineage pitfall 7).

## 16. Testing

Unit tests call `step` directly with a `TestWorld`: an in-crate `CollisionQuery` over analytic shapes
(planes, boxes, ramps, stairs, water and ladder volumes) behind a `testing` feature. No physics engine
is involved, which makes the tests fast and exact and keeps the crate in the logic tier.

**Known-number tests** (per preset where relevant):
- jump apex ≈ `jump_speed² / 2g`, within `g·dt²/8`;
- ground speed converges to `walk_speed` and never exceeds it;
- stopping distance from top speed matches the closed form of the friction formula;
- with `QuakeProjection` and an air cap, ideal strafing gains speed every tick; with `Proper` it never
  exceeds `wishspeed`;
- a perfectly timed re-jump loses no horizontal speed with `skip_friction_on_jump_tick`, and loses
  one tick of friction without it;
- `RequireRelease` and `PressEdge`: holding jump continuously through a landing does not jump again;
- a 0.45 m step is climbed and a 0.47 m step blocks (step height 0.46 m);
- a 44° ramp is walkable and a 47° ramp slides;
- walking down stairs stays grounded every tick (snap);
- a `TuckLegs` crouch-jump clears an obstacle `standing − crouched` higher than `ShrinkTop`;
- standing up under a low ceiling stays crouched, and stands on the first tick there is room.

**Trajectory (golden) tests.** Scripted intent sequences (circle-strafe, bhop chain, stair run, ledge
mantle) are stepped for a few hundred ticks and the full pose/velocity trace is compared against a
stored file. These catch the ordering changes that constant checks miss.

**Determinism and rollback tests.** Run the same sequence twice and require bit-identical state;
then save state at tick N, step to N+k, restore N, resimulate, and require the same result.

**Backend parity test** (in `krit_avian`): the golden trajectories, replayed against real avian
colliders through `AvianCollisionQuery`, stay within a small tolerance of the `TestWorld` result.

## 17. Changes this proposes to `architecture.md`

1. **Move `MovementState` and `Stance` into `krit_core`, and add a `Locomotion` read model there**
   (§2, §12.1). `krit_kamera` and `krit_weapons` read them but may only depend on core; today §5.4
   lists them as movement-owned. `KinematicState` stays private to movement.
2. **`Crouch` and `Prone` become `Stance` values**, orthogonal to `MovementState` (§6), so crouching
   in the air and while sliding are expressible. Sprint is a `Locomotion` flag, not a state.
3. **Order `Movers` before `Aim`** in `KritSimSystems` (§10.2), so platform yaw is applied on the same
   tick. Movers do not read aim, so nothing else changes. (`krit_kore::sets` and `KritKore` would
   change with it.)
4. **Add a `PhysicsBackend` trait to `krit_core::physics`** (§3.2), and make `MovementPlugins` generic
   over it. This is how a logic crate calls the backend's system params without the backend
   depending on it.
5. **Make `CollisionQuery` object-safe** and return trace-shaped hits (`fraction`, `normal`,
   `entity`, `start_solid`), and add `SurfaceInfo.flags` and `MoverDelta` (§5.5, §10.2).
6. **New core types:** `ExternalImpulse { velocity, friction_lock, mode }`, `AimRecoilRecovery`,
   `AimLimits` (yaw/pitch clamp relative to a seat, applied by the aim stage and mirrored by
   `krit_input` like `PITCH_LIMIT`), `Discontinuity`, `WaterLevel`, `ContentsFlags::LADDER`.
7. **Facts:** add `StanceChanged`, and rename `StateChanged` to `MovementStateChanged`.
8. **`jump_mode`** gains `PressEdge` (Source), separate from `RequireRelease` (Quake 3), since the
   two differ when jump is held through a landing (§8).
9. **`fn step` signature** takes `&dyn CollisionQuery` and a `StepContext`, and returns a
   `StepReport` (§4), rather than `(state, MoveOutput)`.

## 18. Open questions

- **Platform yaw and `LocalAim`.** Carrying yaw through `ViewAngleOffset` is exact, but the camera's
  pending term (§5.12 of the architecture) shows mouse input on top of the *last tick's* yaw. On a
  fast-spinning platform the camera may visibly step at tick rate. Does kamera need to extrapolate
  `ViewAngleOffset` between ticks?
- **Stamina as a generic pool.** Stamina, jetpack fuel, weapon heat and shields are all
  `ResourcePool { current, max, regen, regen_delay }` (FPS Microgame research §13). Should that type
  live in `krit_stats`, with movement only owning the drain rules?
- **Prone hull.** An upright short cylinder lets players poke through low walls they appear to lie
  behind. A yaw-aligned box is better, but changes shape when turning, which can embed the hull.
- **Character–character collision.** Treating other characters as walls is simple and predictable,
  but feels sticky in crowded co-op. Is a soft separation (a small `ExternalImpulse` between
  overlapping characters, authority-only) worth adding?
- **Surf.** The preset `Source` profile surfs on steep ramps, as in CS:S. Should there be an explicit
  `Surf` preset with tuned `air_accel` and `overbounce`, or is that left to games?
- **`Arcade` model and slopes.** `Arcade` moves toward a target velocity, which on a slope fights
  gravity in a way the Quake models don't. It needs a slope rule (project the target onto the ground
  plane) and its own golden tests.
