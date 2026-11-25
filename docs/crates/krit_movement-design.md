# `krit_movement`: detailed design

> Status: **proposal**, expanding the summary in [§5.4](krit_movement.md). Where this document and
> the summary disagree, this document wins and the summary is out of date.
>
> **How to read it.** Parts 1–4 build the controller up one feature at a time: basic movement, then
> jumping, then crouching, then the other states. Each part only *adds* fields, steps and states; it
> never changes what an earlier part defined. Part 5 covers forces from the world (knockback, moving
> platforms, pushing props). Everything after that is reference.
>
> Units are metres, seconds and radians unless stated. +Y is up and −Z is forward, as in Bevy.
> Reference values are converted from Quake/Source units at 0.0254 m/u; their sources are in
> [`research/source-quake-lineage.md`](../research/source-quake-lineage.md) §3.2 and §7.2.
> Section references written "arch §n" point to [`architecture.md`](../architecture.md); plain "§n"
> points inside this document.

## 0. Overview

### 0.1 Scope

`krit_movement` turns a Character's `CharacterIntent` into a new body position and velocity once per
simulation tick, and computes the Character's facing (`ViewAngles`). It is a **logic-tier** crate
(arch §3.2): no renderer, no physics engine, no BEI.

**It does:**
- the aim stage (`ViewAngles` from intent + offset, `AimRecoil` decay), §6;
- the kinematic character controller (KCC): ground detection, friction, acceleration, gravity,
  collide-and-slide, step-up, ground snap and depenetration;
- jumping, stance (stand, crouch, prone) and the movement modes (ground, air, slide, ladder, swim,
  mantle, attached, noclip);
- stamina;
- the read models and facts other crates consume (`Locomotion`, `MovementSpreadFactor`, `Landed`, …);
- fall damage *requests*.

**It does not:**
- write `Transform`. The physics backend owns the body's position (arch P9);
- talk to avian. All world access goes through `krit_core::physics::CollisionQuery`;
- smooth anything for display. Step smoothing, crouch eye smoothing, head bob and landing dips are
  `krit_kamera` effects driven by the read models in §9;
- apply damage. Fall damage is a `DamageRequest`; `krit_damage` decides what happens to it;
- push dynamic bodies. It *requests* pushes with `CharacterPush`; the backend applies them.

### 0.2 Five kinds of data

Every type in this crate is one of five kinds. Knowing the kind tells you who writes it, how long it
lives, and whether rollback has to restore it.

| Kind | Question it answers | Lifetime | Rolled back? | Types |
|---|---|---|---|---|
| **Configuration** | *What is this character?* | Set at spawn; changes rarely | Replicated, not predicted | `CharacterController`, `CharacterHull`, `MovementProfile`, `MovementStats` |
| **Command** | *What does it want this tick?* | One tick | No, rebuilt from the intent | `MoveCommand` |
| **Surroundings** | *What is around it right now?* | One tick | No, re-measured from the world | `Surroundings`, `BodyPose` |
| **Motion state** | *What is it doing?* | Carried tick to tick | **Yes**: this is the whole rollback state | body position (backend), `KinematicState`, `MovementState`, `Stance`, `Stamina`, and the aim state `ViewAngles`, `ViewAngleOffset`, `AimRecoil` (in `core`; written by the aim stage and weapons, §7) |
| **Results** | *What happened, and what should others see?* | One tick | No, recomputed | `StepEvents`, `Locomotion`, `EyeHeight`, `MovementSpreadFactor`, facts |

The rule that keeps this design small: **only motion state carries information from one tick to the
next.** Anything that can be recomputed from the world, the intent or the configuration is
recomputed, so rollback has less to restore and a mispredicted value cannot leak into later ticks.

### 0.3 One tick at a glance

```
KritSimSystems::Movers     moving platforms move and record MoverDelta                  (Part 5)
KritSimSystems::Aim        ViewAngles = clamp(intent.aim + ViewAngleOffset)             (§6)
KritSimSystems::Movement
  Prepare     CharacterIntent + ButtonEdges + ActionGates + ViewAngles ──▶ MoveCommand
              ExternalImpulse ──▶ KinematicState.velocity
  Sense       backend position ──▶ BodyPose;  ride mover, depenetrate, probe ──▶ Surroundings
  Transition  Surroundings + MoveCommand + MovementState ──▶ ModeRequest   (special modes only, Part 4)
  Step        step(prev MotionState, ModeRequest, …) ──▶ StepResult ──▶ BodyPose, KinematicState, MovementState, StepEvents
  Apply       BodyPose, velocity, hull, yaw ──▶ backend body
  Report      StepEvents + motion state ──▶ Locomotion, EyeHeight, MovementSpreadFactor, facts
```

Each stage is a public `MovementSystems` set (arch P10), so a game can remove a stage or slot a
system between two of them. §1.5 describes the stages; §1.4 describes the data passed between them.

---

## Part 1. Basic movement: walking, falling, colliding

**Goal:** a character that walks on floors and slopes, climbs stairs, slides along walls and falls
off ledges. Two states only: `Ground` and `Air`. No jumping or crouching yet.

### 1.1 Spawning a character

```rust
commands.spawn((Character, CharacterController::default(), MovementProfileHandle(handle)));
```

`CharacterController` is the marker that opts an entity into movement. It `#[require]`s everything
else in this part with default values, so the line above is a complete, working character.

```rust
#[derive(Component, Default)]
#[require(CharacterHull, KinematicState, MovementState, BodyPose, Surroundings, MoveCommand,
          ModeRequest, StepEvents, Locomotion, EyeHeight, MovementSpreadFactor, ViewAngles,
          ViewAngleOffset)]
pub struct CharacterController {
    /// Replaces the profile asset for this one character (bosses, tests, debug tuning).
    pub profile_override: Option<MovementProfile>,
}
```

### 1.2 Configuration: what the character is

**`CharacterHull`**: the collision shape. Part 1 uses the standing fields only; Part 3 adds the
crouched and prone heights.

```rust
pub struct CharacterHull {
    pub shape: HullShape,         // Cylinder (default) | Capsule | Box
    pub radius: f32,              // box: half-extent
    pub standing_height: f32,
    pub eye_below_top: f32,       // eye distance below the top of the current hull
    pub skin_width: f32,          // gap kept between hull and walls, default 0.01 m
    // Part 3: crouched_height, prone_height
}
```

- **Origin at the feet.** The body origin is the bottom centre of the hull, so a hull height change
  (Part 3) never moves the feet and never needs a position correction while grounded.
- **Cylinder by default**, because parry's capsule casts are unreliable at edges (bevy_ahoy's note)
  and step-up assumes a flat bottom. A box gives GoldSrc feel.

**`MovementProfile`** and **`MovementStats`** split the tuning in two:

- `MovementProfile` is *how* the character moves: acceleration model, step rule, slope limit,
  timings. It is an immutable asset (RON, `assets` feature), referenced by `MovementProfileHandle` or
  replaced by `CharacterController::profile_override`. Nothing changes it at runtime.
- `MovementStats` is *how fast and how strong*: speeds, accelerations, jump speed. Its base values
  come from a separate `BaseMovementStats` component (with its own presets, §8.4), and `krit_stats`
  modifiers (item weight, a slowing effect, ADS) change them.
  Movement reads only the resulting `MovementStats`.

The full field lists and presets are in §8. Part 1 uses `walk_speed`, `ground_accel`, `air_accel`,
`gravity`, `friction`, `stop_speed`, `accel_model`, `air_wishspeed_cap`, `step_height`, `step_rule`,
`max_walk_slope_cos`, `ground_probe_distance`, `ground_snap_distance` and `unground_speed`.

### 1.3 Motion state: what it carries between ticks

In Part 1 the rolled-back state is the body position (held by the backend) plus:

```rust
#[derive(Component, Clone, Default, Reflect, PartialEq, MapEntities)]
#[reflect(SimState)]
pub struct KinematicState {
    /// World-space velocity, m/s. While standing on a mover it is relative to the mover (Part 5).
    pub velocity: Vec3,
    /// The entity stood on at the end of the last tick. Kept only so the next tick knows which
    /// mover to ride (Part 5); the ground itself is re-measured every tick.
    #[entities] pub ground: Option<Entity>,
    // Part 2: ticks_since_grounded, jump_buffer, jump_held
    // Part 3: stance_ticks
    // Part 4: mode_data
    // Part 5: friction_lock
    // §9.3:   stride, footsteps
}

/// The mode, which decides which physics runs. Part 1 uses only Ground and Air.
#[derive(Component, Clone, Copy, Default, Reflect, PartialEq, Eq, Debug)]
pub enum MovementState { Ground, #[default] Air, /* Part 4: Slide, Ladder, Swim, … */ }
```

**What is *not* stored**, and why: the ground normal, the surface under the feet, the water level and
ladder contact are all re-measured from the world at the start of every tick (§1.6), as Quake 3's
`PM_GroundTrace` and Source's `CategorizePosition` do. Storing them would add rollback state that
can only ever be stale.

### 1.4 The intermediate data, and why each piece exists

Five components exist only to hand data from one stage to the next within a single tick. They are
`Derived` (never replicated, never rolled back) and are fully overwritten or consumed every tick, so a stale
value can never survive into the next one.

#### `MoveCommand`: what the character asks for this tick, in world terms

```rust
#[derive(Component, Clone, Default)]
pub struct MoveCommand {
    /// Horizontal unit direction the character wants to move in, in world space. Zero for no input.
    pub wish_dir: Vec3,
    /// How hard it wants to move, 0..=1. A half-tilted stick gives 0.5; keyboards give 0 or 1.
    pub wish_amount: f32,
    /// Full view direction including pitch. Only swim, ladder and noclip use it (Part 4).
    pub look_dir: Vec3,
    /// Movement buttons held this tick, after `ActionGates` and forced/disabled masks.
    pub held: Buttons,
    /// Movement buttons that went down this tick (from core's `ButtonEdges`), after gates.
    pub pressed: Buttons,
    /// A game-supplied velocity that replaces wish acceleration this tick (§11, tier 3).
    pub velocity_override: Option<VelocityOverride>,
}
```

- **Built by** `Prepare`, from `CharacterIntent`, `ButtonEdges`, `ActionGates` and `ViewAngles.yaw`
  (§1.7.4 has the formula).
- **Read by** `Transition` and `Step`.
- **Why it exists:** `CharacterIntent` is local-space and ungated. Three separate concerns turn it
  into something physics can use: rotating `move_axis` by the facing, applying gates (a `MOVE` gate
  zeroes the wish, a `SPRINT` gate clears `SPRINT`), and pairing held buttons with their press
  edges. Doing that once, here, means no movement system ever reads `ActionGates` or `ViewAngles`
  directly, and tests can build a `MoveCommand` by hand without any input or aim setup.

The wish carries a *direction and amount*, not a speed, because the speed depends on the state
(walking, crouched, swimming) that the step decides.

#### `BodyPose`: the working copy of the body position

```rust
#[derive(Component, Clone, Default)]
pub struct BodyPose { pub position: Vec3 }
```

- **Built by** `Sense`, copied from the backend's body (avian `Position`), then adjusted by riding a
  mover and depenetration.
- **Updated by** `Step`. **Read by** `Apply`, which writes it back to the backend.
- **Why it exists:** two reasons. First, `krit_movement` cannot name avian's `Position` (it depends
  only on core), so it needs a position type of its own. Second, `Step` needs read access to the
  world (shape casts) while `Apply` needs write access to bodies; in avian those touch the same
  components. Working on a copy lets the read-only world query and the body writer live in
  different systems without borrow conflicts. The backend's body stays the single source of truth:
  `BodyPose` is overwritten from it at the start of every tick.

#### `Surroundings`: what is around the character at the start of the tick

```rust
#[derive(Component, Clone, Default)]
pub struct Surroundings {
    /// Walkable ground under the feet, if any. `None` means airborne.
    pub ground: Option<GroundHit>,
    /// True if depenetration could not free the hull. The step will not move the character.
    pub stuck: bool,
    /// Part 4: how deep the character is in water.
    pub water: WaterLevel,
    /// Part 4: the normal of a ladder the hull is touching, if any.
    pub ladder: Option<Dir3>,
}

pub struct GroundHit {
    pub entity: Entity,
    pub normal: Dir3,
    pub distance: f32,        // gap between the feet and the ground, ≤ ground_probe_distance
    pub surface: SurfaceInfo, // friction multiplier, flags, mover velocity (from the backend)
}
```

- **Built by** `Sense` (§1.6). **Read by** `Transition` and `Step`.
- **Why it exists:** the facts in it are needed by more than one stage (Part 4's transitions need
  water and ladder contact before the step runs), they must be measured *after* the body is in its
  final pre-step position (after riding and depenetration), and they are not rolled back. A
  component written once in `Sense` gives all readers the same answer from one set of casts.
- **Why not reuse the end of last tick's result:** the body may have moved since. Spawns, teleports,
  moving platforms, a backend correction and a rollback restore all change the position between
  ticks, and the step must start from the world as it is now.

#### `StepEvents`: what happened during the tick

```rust
#[derive(Component, Clone, Default)]
pub struct StepEvents {
    /// Vertical speed at touchdown, if the character landed this tick.
    pub landed: Option<f32>,
    /// Vertical distance moved by a step-up (positive) or ground snap (negative), for camera smoothing.
    pub step_height: f32,
    /// Every surface the slide touched: entity, normal, impact speed.
    pub contacts: SmallVec<[Contact; 8]>,
    /// Walkable ground at the end of the step (the re-probe), if any. `Surroundings.ground` is the
    /// start of the tick, which is `None` on a landing tick.
    pub ground: Option<GroundHit>,
    /// The mode at the start of the tick and at the end, if they differ.
    pub mode_changed: Option<ModeChange>,     // ModeChange { from, to }
    pub stance_changed: Option<StanceChange>, // Part 3
    pub jumped: bool,                         // Part 2
    pub footstep: bool,                       // §9.3
}
```

- **Built fresh by each `step` call** and returned in `StepResult` (§1.7.1); the `Step` system
  writes it to the component, and `Report` reads it.
- **Why it exists:** the step is a pure function and cannot send messages or trigger events. It
  records what happened here, and `Report` turns the record into facts through core's `emit_fact`,
  which also suppresses observer triggers while resimulating (arch §4.1–4.2). Keeping fact emission
  in one place means a custom step (Part 4, §11) gets facts for free by returning the same record.
- **Mode and stance changes are recorded here, not reconstructed by Report.** The step is the only
  writer of `MovementState` and `Stance`, so it knows both ends of the tick and records the change
  at most once, however many Transition systems weighed in.

#### `ModeRequest`: which mode Transition wants the step to run in

```rust
#[derive(Component, Clone, Default)]
pub struct ModeRequest(pub Option<ModeEntry>);

#[derive(Clone, PartialEq, Debug)]
pub struct ModeEntry {
    pub mode: MovementState,
    pub data: ModeData,        // Part 4: the mode's starting data (a mantle's path, a ladder's normal)
}
```

- **Written by** `Transition` systems (§4.2). **Taken by** the `Step` system, which passes it to
  `step` and leaves `None` behind.
- **Why it exists:** if Transition wrote `MovementState` directly, the mode would have two writers
  in one tick, and the step would never see the mode the tick started in. As a request, Transition
  can still decide everything (later systems overwrite earlier ones, so priority still works), but
  the step applies it, and `prev.mode` stays the true start of the tick. Carrying `ModeData` in the
  request does the same for `KinematicState.mode_data`, which would otherwise also have two writers.
- **Part 1 never writes one**: `Ground` and `Air` are settled by the step itself.

### 1.5 Schedule

All of it runs in `KritSimPhase::PrePhysics`, which `krit_avian` places before
`PhysicsSystems::First`.

```
KritSimSystems::Intent      (core)   edges, gates, forced/disabled buttons
KritSimSystems::Spawn       (match)
KritSimSystems::Movers      (game)   moving platforms move and write MoverDelta
KritSimSystems::Aim         MovementLook (§6)
   AimSystems::Ride           platform yaw → ViewAngleOffset
   AimSystems::Compose        ViewAngles = clamp(intent.aim + ViewAngleOffset)
   AimSystems::Recoil         AimRecoil decay
KritSimSystems::Movement    MovementCore and feature plugins
   MovementSystems::Prepare     build MoveCommand; apply ExternalImpulse
   MovementSystems::Sense       BodyPose from the backend; ride; depenetrate; build Surroundings
   MovementSystems::Transition  enter/leave special modes (Part 4); built-in, then game systems
   MovementSystems::Step        the pure step (§1.7), one call per character
   MovementSystems::Apply       BodyPose, velocity, hull and yaw → backend body
   MovementSystems::Report      Locomotion, EyeHeight, MovementSpreadFactor, facts, fall damage
```

`Movers` runs before `Aim` so a rotating platform's yaw reaches `ViewAngles` on the same tick
(Part 5). `Aim` runs before `Movement` because `Prepare` needs this tick's yaw.

**How the backend plugs in.** Backends depend only on `krit_core` (arch §3.1), so `krit_avian` cannot
put systems in `MovementSystems` and cannot name movement types. Instead `krit_core::physics` defines
a backend trait, and the movement plugin, generic over it, installs `Sense`, `Step` and `Apply`
itself:

```rust
// krit_core::physics (sketch: exact SystemParam bounds are an implementation detail)
pub trait PhysicsBackend: Send + Sync + 'static {
    /// Read-only world queries (shape casts, contents, surfaces). Implements CollisionQuery.
    type Query: ReadOnlySystemParam;
    /// Reads and writes the position, velocity and collider of character bodies.
    type Bodies: SystemParam;
}

// krit_fps (facade), with the `avian` feature
app.add_plugins(MovementPlugins::<krit_avian::Avian>::default());
```

`Sense` reads `Bodies` and `Query`; `Step` reads only `Query`; `Apply` writes only `Bodies`. That
split is what lets the world query and the body writer coexist (see `BodyPose` above).
`krit_ballistics` follows the same pattern with `SpatialQueryBackend`.

**Backend contract for `Apply`.** The backend places the body at `BodyPose.position` and must not
move it again in its own step: the character is kinematic and movement has already resolved its
collisions. The velocity is handed over so the backend can use it when the character hits other
bodies, not so the backend integrates it.

With avian this works as follows. Characters are `RigidBody::Kinematic`, so avian applies no forces,
gravity or contact response to them, and it uses their velocity only to push dynamic bodies they
touch. Avian's position integrator still adds `LinearVelocity × dt` to kinematic bodies, though, so
`krit_avian` also gives characters `CustomPositionIntegration`. With that marker, `Apply` can
write both `Position` and `LinearVelocity`, and the character moves exactly once per tick.

### 1.6 Sense: measuring the world

Runs once per character, in this order:

1. **Copy** the backend body position into `BodyPose`.
2. **Ride** a mover, if `KinematicState.ground` is one (Part 5). Does nothing in Part 1.
3. **Depenetrate.** `world.depenetrate(hull, pose)`. If the hull is still embedded after
   `max_depenetration_iterations`, set `Surroundings.stuck`. A stuck character keeps its velocity
   but the step does not move it, so it never tunnels through the geometry it is stuck in.
4. **Probe the ground** (below), giving `Surroundings.ground`.
5. **Contents** (Part 4): `world.contents()` at feet, waist and eye height gives `water` and
   `ladder`.

**Ground probe.** The same function is used here and again inside the step after the move:

```
hit = world.cast_shape(hull, pose, −Y × ground_probe_distance, filter)
ground = hit if  hit.normal.y >= max_walk_slope_cos
             && velocity.y <= unground_speed        // moving up fast = launched, not grounded
             && !hit.surface.flags.contains(NOT_WALKABLE)
```

- `ground_probe_distance` defaults to 0.05 m (Source traces 2 u). It is extended by a descending
  mover's downward speed × dt, so characters don't lose contact with a lift going down (bevy_ahoy).
- `max_walk_slope_cos` defaults to 0.7 (≈45.6°). Steeper surfaces are not ground: the character
  slides down them, which is what makes surf ramps work when air acceleration is high enough.
- `unground_speed` defaults to 3.56 m/s (Source `NON_JUMP_VELOCITY`, 140 u/s). It stops jump pads,
  explosions and fast elevators from re-grounding the character on the tick they launch it.

### 1.7 The step

The step is a **transition function**. It takes the motion state at the start of a tick and returns
the motion state at the end, plus a record of what happened. It reads nothing from the ECS and
writes nothing anywhere. Whatever calls it decides what to do with the result: write it to
components (the live tick), feed it straight back in (rollback resimulation, a server stepping bots,
a test running a trajectory), or throw it away (a prediction preview).

```
                 ┌──────────── Tuning ─────────────┐
                 │ profile, stats, hull            │
                 └────────────────┬────────────────┘
 MoveCommand ────────────────────┐ │
 ModeRequest ────────────────────┐│ │
 Surroundings ──────────────────┐││ │
 MotionState (prev) ───────────┐│││ │
 dt ──────────────────────────┐││││ │
                              ▼▼▼▼▼ ▼
                           step(world, …) ──▶ StepResult { state: MotionState, events: StepEvents }
```

The step is also the **only writer of `MovementState`**. `Transition` systems decide which mode a
character should be in, but they write a `ModeRequest` rather than the mode itself (§4.2); the step
applies the request first. So `prev.mode` is always the mode the tick started in, and the step can
report the change itself instead of something else remembering the starting mode.

#### 1.7.1 Inputs and outputs

```rust
pub fn step(
    world: &dyn CollisionQuery,  // scoped to this character: excludes its own body, applies its layers
    tuning: &Tuning,             // what the character is            (configuration, §1.2)
    command: &MoveCommand,       // what it wants this tick          (command, §1.4)
    request: Option<&ModeEntry>, // mode Transition asked for, if any (command, §4.2; always None in Part 1)
    sensed: &Surroundings,       // what is around it at tick start  (surroundings, §1.6)
    prev: &MotionState,          // what it was doing                (motion state, §1.3)
    dt: f32,                     // Time<Fixed>::delta_secs()
) -> StepResult;

/// Read-only borrows of the configuration. Not a component; built by whoever calls `step`.
pub struct Tuning<'a> {
    pub profile: &'a MovementProfile,
    pub stats: &'a MovementStats,
    pub hull: &'a CharacterHull,
}

/// Everything the step carries from one tick to the next, by value. This is exactly the rollback
/// state of §0.2: if a field is here, a snapshot holds it; if the step reads something from the
/// previous tick, it must be here, or resimulation drifts from the live result.
#[derive(Clone, PartialEq, Debug)]
pub struct MotionState {
    pub pose: BodyPose,               // body position, after Sense rode and depenetrated it
    pub kinematic: KinematicState,    // velocity, ground entity, timers
    pub mode: MovementState,          // the mode the tick starts in; Transition doesn't touch it
    // Part 3: pub stance: Stance,
    // Part 4: pub stamina: Option<Stamina>,   // None without MovementStamina
}

pub struct StepResult {
    pub state: MotionState,
    pub events: StepEvents,           // §1.4; built fresh by each call
}
```

`world` is scoped to one character before the call (it excludes the character's own collider and
applies its collision layers), so nothing inside the step names an `Entity` or builds a filter.

Where every value comes from, what the step calls it, and where the result goes on a live tick:

| Value | Comes from | In the step | Goes to |
|---|---|---|---|
| `CharacterIntent.move_axis` + `ViewAngles.yaw` | input / bot, aim stage | `command.wish_dir`, `command.wish_amount` (built in Prepare, §1.7.4) | – |
| `MovementStats` | `BaseMovementStats` + `krit_stats` modifiers | `tuning.stats.walk_speed`, `.ground_accel`, `.air_accel`, `.gravity_scale` | – |
| `MovementProfile` | asset or `CharacterController::profile_override` | `tuning.profile.*` | – |
| `CharacterHull` | spawn / archetype | `tuning.hull` (shape for every cast) | – |
| backend body position | avian `Position`, copied by Sense | `prev.pose.position` | `state.pose` → `BodyPose` → Apply → avian `Position` |
| `KinematicState` | last tick's result (or a rollback snapshot) | `prev.kinematic.velocity` | `state.kinematic` → `KinematicState`; Apply copies `velocity` → avian `LinearVelocity` |
| `MovementState` | last tick's result (or a rollback snapshot) | `prev.mode` | `state.mode` → `MovementState` → next tick's Transition and step |
| `ModeRequest` | Transition systems (§4.2) | `request` (taken by the Step system, so it never outlives the tick) | applied to `state.mode` and `kinematic.mode_data` |
| mode / stance change | the step, comparing `prev` with the final state | – | `events.mode_changed`, `events.stance_changed` → Report (`MovementStateChanged`, `StanceChanged`) |
| ground under the feet | Sense (§1.6) | `sensed.ground`, `sensed.stuck` | – (re-measured, never stored) |
| ground after the move | re-probed inside the step | – | `events.ground` → Report (`Locomotion.grounded`, `.ground_normal`, `Landed.surface`) |
| what happened | the step | – | `events` → `StepEvents` → Report (facts, `Locomotion`, `KinematicContacts`) |
| `dt` | `Time<Fixed>` | `dt` | – |

#### 1.7.2 The Part 1 step

Every helper it calls is a public library step in `krit_movement::kcc` (§11, tier 1) with the same
shape: plain values in, a new value out.

```rust
pub fn step(world: &dyn CollisionQuery, tuning: &Tuning, command: &MoveCommand,
            request: Option<&ModeEntry>, sensed: &Surroundings, prev: &MotionState,
            dt: f32) -> StepResult {
    let Tuning { profile, stats, hull } = *tuning;
    let mut events = StepEvents::default();

    // 0. Enter the mode Transition requested (§4.2). In Part 1 there is never a request.
    let mut current = prev.clone();
    if let Some(entry) = request {
        current.mode = entry.mode;
        // Part 4: current.kinematic.mode_data = entry.data.clone();
    }

    // Stuck (§1.6): keep the velocity, don't move. Same exit as §4.3's "end of tick".
    if sensed.stuck {
        events.ground = sensed.ground.clone();
        events.mode_changed = ModeChange::between(prev.mode, current.mode);
        return StepResult { state: current, events };
    }

    // "Grounded" for steps 1–4 is the ground measured at the start of the tick.
    let start_ground = sensed.ground.as_ref();
    let gravity = profile.gravity * stats.gravity_scale;
    let wish_speed = command.wish_amount * stats.walk_speed;
    let mut velocity = current.kinematic.velocity;

    // 1. Friction (§1.7.5). Arcade has its own deceleration instead.
    if let Some(ground) = start_ground
        && profile.accel_model != AccelModel::Arcade
    {
        velocity = kcc::friction(velocity, ground, profile, dt);
    }

    // 2. Accelerate (§1.7.6) toward wish_dir × wish_speed.
    let params = AccelParams::new(profile, stats, wish_speed, start_ground.is_some());
    velocity = kcc::accelerate(profile.accel_model, velocity, command.wish_dir, wish_speed, &params, dt);

    // 3. First half of gravity.
    if start_ground.is_none() {
        velocity.y -= gravity * dt * 0.5;
    }
    // Impact speed for a landing, taken before the move: the slide clips v.y against the floor.
    let fall_speed = -velocity.y;

    // 4. Move (§1.7.7, §1.7.8).
    let moved = match start_ground {
        Some(ground) => {
            let stepped = kcc::step_move(world, hull, profile, current.pose.position, velocity, dt, ground);
            kcc::snap_down(world, hull, profile, stepped)
        }
        None => kcc::slide(world, hull, profile, current.pose.position, velocity, dt, &[]),
    };
    let mut velocity = moved.velocity;

    // 5. Probe the ground again from where the move ended (same probe as Sense, §1.6).
    let end_ground = kcc::ground_probe(world, hull, profile, moved.position, velocity);

    // 6. Second half of gravity, or keep the velocity on the ground plane.
    match &end_ground {
        None => velocity.y -= gravity * dt * 0.5,
        Some(ground) => velocity = kcc::clip_velocity(velocity, ground.normal, 1.0),
    }

    // 7. Settle the mode, record a landing and any mode change since the start of the tick.
    let mode = if end_ground.is_some() { MovementState::Ground } else { MovementState::Air };
    if current.mode == MovementState::Air && mode == MovementState::Ground {
        events.landed = Some(fall_speed);
    }
    events.mode_changed = ModeChange::between(prev.mode, mode);   // Part 3: same for stance

    events.step_height = moved.step_height;
    events.contacts = moved.contacts;
    events.ground = end_ground.clone();

    StepResult {
        state: MotionState {
            pose: BodyPose { position: moved.position },
            kinematic: KinematicState {
                velocity,
                ground: end_ground.map(|ground| ground.entity),
                ..current.kinematic        // later parts' timers, advanced at the end of the step
            },
            mode,
        },
        events,
    }
}

impl ModeChange {
    /// `Some` only if the mode actually changed, so Report emits at most one fact per tick.
    pub fn between(from: MovementState, to: MovementState) -> Option<Self> {
        (from != to).then_some(Self { from, to })
    }
}
```

Because the change is measured from `prev` to the final state, intermediate values never leak out.
Leaving a ladder, Transition requests `Air`; if the walk physics then settles on `Ground`, Report sees
one `Ladder → Ground` change, not `Ladder → Air` followed by `Air → Ground`.

**`Ground` ↔ `Air` is settled by the step itself, not by `Transition`.** Whether the character ends
the tick on the ground is only known after it has moved, and the next tick's `Sense` re-measures it
anyway. `Transition` only handles the special modes of Part 4.

**Why gravity is split in two.** Over one tick the body moves `(v − g·dt/2)·dt`, which is the exact
displacement for constant gravity. A fall therefore follows the analytic parabola at any tick rate,
and Part 2's jump apex is `v₀²/2g` to within one tick's sampling error (≤ `g·dt²/8`).

#### 1.7.3 Who calls the step

`sense` and `step` are both pure, so every caller is a short loop around them. The `Sense` stage
(§1.6) is a thin system around `kcc::sense`:

```rust
/// §1.6 steps 2–5 as a function: ride, depenetrate, probe, contents.
pub fn sense(world: &dyn CollisionQuery, tuning: &Tuning, position: Vec3,
             kinematic: &KinematicState) -> (BodyPose, Surroundings);
```

**The live tick.** The `MovementSystems::Step` system gathers the components, calls `step` once per
character, and writes the result back. Apply and Report then read those components.

```rust
fn step_characters<B: PhysicsBackend>(
    world: B::Query,
    time: Res<Time<Fixed>>,
    profiles: Res<Assets<MovementProfile>>,
    mut characters: Query<(
        Entity,
        &CharacterController, &MovementProfileHandle, &MovementStats, &CharacterHull, // Tuning
        &MoveCommand, &mut ModeRequest, &Surroundings,                 // this tick's inputs
        &mut BodyPose, &mut KinematicState, &mut MovementState,        // MotionState, in and out
        &mut StepEvents,                                               // out
    )>,
) {
    let dt = time.delta_secs();
    for (entity, controller, handle, stats, hull, command, mut request, sensed,
         mut pose, mut kinematic, mut mode, mut events) in &mut characters {
        let profile = controller.profile_override.as_ref().unwrap_or_else(|| &profiles[&handle.0]);
        let tuning = Tuning { profile, stats, hull };
        let prev = MotionState { pose: pose.clone(), kinematic: kinematic.clone(), mode: *mode };
        let request = request.0.take();      // consumed here, so a request never leaks into the next tick

        let result = step(&world.for_character(entity), &tuning, command, request.as_ref(),
                          sensed, &prev, dt);

        *pose = result.state.pose;           // → Apply → backend Position
        *kinematic = result.state.kinematic; // → next tick; Apply → backend LinearVelocity
        *mode = result.state.mode;           // → Report, next tick's Transition
        *events = result.events;             // → Report
    }
}
```

The same `MotionState` value is what a rollback snapshot holds. With full-schedule rollback
(arch §4.2), the network layer restores these components and re-runs the schedule; this system
runs again unchanged.

**Resimulating one character** (client reconciliation without re-running the schedule, a server
stepping a bot ahead, a golden-trajectory test). `Sense` runs inside the loop, because each tick
starts from the position the previous tick produced:

```rust
let mut state = snapshot.clone();                       // MotionState at the snapshot tick
for command in &commands_since_snapshot {               // MoveCommand per tick
    let (pose, sensed) = kcc::sense(world, &tuning, state.pose.position, &state.kinematic);
    state.pose = pose;
    let request = kcc::transition(world, &tuning, command, &sensed, &state);   // built-in rules, §4.2
    state = step(world, &tuning, command, request.as_ref(), &sensed, &state, dt).state;
}
// `state` is the corrected present; write it to the components once.
```

`kcc::transition` covers the built-in transitions only. A game with its own transition systems
(§11, tier 2) either resimulates through the schedule or calls its own rules in the same place.

`world` answers with colliders where they are *now*, not where they were on the replayed tick.
Static geometry doesn't care. Remote characters are interpolated in the past anyway and the server
corrects any difference. Moving platforms (Part 5) avoid the problem by placing themselves as a pure
function of the tick, so a resim can ask where a platform was on any replayed tick.

#### 1.7.4 Wish direction (in `Prepare`)

`CharacterIntent.move_axis` is the character's local X/Z plane: `x` is right, `y` is local Z, so
**forward is −y**, matching Bevy's −Z forward. `krit_input` flips the binding's y to produce this;
bots and replays writing `CharacterIntent` directly use the same convention.

```rust
fn prepare_command(intent: &CharacterIntent, view: &ViewAngles, gates: &ActionGates) -> MoveCommand {
    let local = Vec3::new(intent.move_axis.x, 0.0, intent.move_axis.y);
    MoveCommand {
        // rotate_y(yaw) turns local −Z into (−sin yaw, 0, −cos yaw)
        wish_dir: local.normalize_or_zero().rotate_y(view.yaw_rads()),
        wish_amount: if gates.contains(ActionGates::MOVE) {
            0.0
        } else {
            intent.move_axis.length().min(1.0)
        },
        // … held, pressed, look_dir, velocity_override (§1.4)
    }
}
```

Pitch does not affect ground or air movement. Analogue sticks give slower walking for free because
`wish_amount` scales the target speed. With the `MOVE` gate set, gravity, friction and sliding still
run. `normalize_or_zero` matters: with no input, `normalize` would return NaN and poison the velocity
on every later tick.

#### 1.7.5 Friction

Quake/Source ground friction, on horizontal speed only:

```rust
pub fn friction(velocity: Vec3, ground: &GroundHit, profile: &MovementProfile, dt: f32) -> Vec3 {
    let speed = velocity.xz().length();
    if speed <= f32::EPSILON {
        return Vec3::new(0.0, velocity.y, 0.0);
    }
    let control = speed.max(profile.stop_speed);           // so slow movement stops in finite time
    let drop = control * profile.friction * ground.surface.friction * dt;
    let scale = (speed - drop).max(0.0) / speed;
    Vec3::new(velocity.x * scale, velocity.y, velocity.z * scale)
}
```

`ground.surface.friction` is the ground's `SurfaceInfo` multiplier (1.0 by default; ice is lower).

#### 1.7.6 Acceleration models

`profile.accel_model` selects one of three. The step picks the rate and cap for ground or air, so
the models themselves don't know which they are in:

```rust
pub struct AccelParams {
    pub accel: f32,       // stats.ground_accel or stats.air_accel (Arcade air: ground_accel × air_control)
    pub decel: f32,       // Arcade only: profile.decel, default accel (× air_control in the air)
    pub cap: Option<f32>, // ground: Some(wish_speed); air: profile.air_wishspeed_cap
}

impl AccelParams {
    pub fn new(profile: &MovementProfile, stats: &MovementStats, wish_speed: f32, grounded: bool) -> Self {
        match (grounded, profile.accel_model) {
            (true, _) => Self {
                accel: stats.ground_accel,
                decel: profile.decel.unwrap_or(stats.ground_accel),
                cap: Some(wish_speed),
            },
            (false, AccelModel::Arcade) => {
                let air_control = profile.air_control.unwrap_or(1.0);
                Self {
                    accel: stats.ground_accel * air_control,
                    decel: profile.decel.unwrap_or(stats.ground_accel) * air_control,
                    cap: None,
                }
            }
            (false, _) => Self { accel: stats.air_accel, decel: 0.0, cap: profile.air_wishspeed_cap },
        }
    }
}

pub fn accelerate(model: AccelModel, velocity: Vec3, wish_dir: Vec3, wish_speed: f32,
                  params: &AccelParams, dt: f32) -> Vec3 {
    match model {
        AccelModel::QuakeProjection => { /* below */ }
        AccelModel::Proper => { /* below */ }
        AccelModel::Arcade => { /* below */ }
    }
}
```

**`QuakeProjection`** (QuakeWorld, Quake 3, Source). Air-strafing and bunny-hop gain come from this:

```rust
let current = velocity.dot(wish_dir);
let capped = params.cap.map_or(wish_speed, |cap| wish_speed.min(cap)); // capped in the test …
let add = capped - current;
if add <= 0.0 {
    return velocity;
}
let amount = (params.accel * wish_speed * dt).min(add);                // … uncapped in the rate
velocity + wish_dir * amount
```

Turning so that `wish_dir` stays nearly perpendicular to `velocity` keeps `current` small, so `add`
stays positive while total speed grows. With no cap (Quake 3), gain relies on a low `air_accel`
instead.

**`Proper`** (Carmack's `#else` branch in Q3 `PM_Accelerate`: "avoids strafe jump maxspeed bug, but
feels bad"). Accelerates the velocity *vector* toward the target, so strafing never exceeds
`wish_speed`:

```rust
let horizontal = velocity.with_y(0.0);
let push = wish_dir * wish_speed - horizontal;
velocity + push.clamp_length_max(params.accel * wish_speed * dt)
```

**`Arcade`**. Moves horizontal velocity toward the target at a fixed rate in m/s², speeding up at
`accel` and stopping at `decel`. The step skips friction for it. This is the feel of most modern
console shooters and of the Bevy `fps_controller`.

```rust
let target = wish_dir * wish_speed;
let rate = if wish_speed > 0.0 { params.accel } else { params.decel };
let horizontal = velocity.with_y(0.0).move_towards(target, rate * dt);
Vec3::new(horizontal.x, velocity.y, horizontal.z)
```

#### 1.7.7 Collide-and-slide

`slide` is the `TryPlayerMove` / `PM_SlideMove` loop. It returns where the hull ended up, the
velocity left after clipping, and every surface it touched:

```rust
pub struct MoveResult {
    pub position: Vec3,
    pub velocity: Vec3,
    pub step_height: f32,                    // + from step_move, − from snap_down, else 0
    pub contacts: SmallVec<[Contact; 8]>,    // → events.contacts → KinematicContacts, CharacterPush
}

const MAX_BUMPS: usize = 4;
const MAX_CLIP_PLANES: usize = 5;

pub fn slide(world: &dyn CollisionQuery, hull: &CharacterHull, profile: &MovementProfile,
             from: Vec3, velocity: Vec3, dt: f32, extra_planes: &[Dir3]) -> MoveResult {
    let original = velocity;
    let mut result = MoveResult { position: from, velocity, step_height: 0.0, contacts: default() };
    let mut planes: SmallVec<[Dir3; MAX_CLIP_PLANES]> = extra_planes.into(); // ground plane, if grounded
    let mut time_left = dt;

    for _ in 0..MAX_BUMPS {
        let motion = result.velocity * time_left;
        let trace = world.cast_shape(hull, result.position, motion);
        if trace.start_solid {
            result.velocity = Vec3::ZERO;
            return result;
        }
        // Advance to the hit, backed off by skin_width along the motion (never past the start).
        result.position += motion * trace.fraction_with_skin(hull.skin_width);
        if trace.fraction >= 1.0 {
            break;
        }
        result.contacts.push(Contact {
            entity: trace.entity,
            normal: trace.normal,
            speed: -result.velocity.dot(*trace.normal),
        });
        time_left -= time_left * trace.fraction;
        if planes.len() == MAX_CLIP_PLANES {
            result.velocity = Vec3::ZERO;
            break;
        }
        planes.push(trace.normal);
        result.velocity = clip_to_planes(result.velocity, &planes, profile.overbounce);
        if result.velocity.dot(original) <= 0.0 {   // never move backwards (Q3)
            result.velocity = Vec3::ZERO;
            break;
        }
    }
    result
}

/// A velocity that slides along every touched plane, or zero in a corner.
fn clip_to_planes(velocity: Vec3, planes: &[Dir3], overbounce: f32) -> Vec3 {
    for (i, n_i) in planes.iter().enumerate() {
        if velocity.dot(**n_i) >= 0.0 {
            continue;                                // not moving into this plane
        }
        let mut clipped = clip_velocity(velocity, *n_i, overbounce);
        for (j, n_j) in planes.iter().enumerate() {
            if j == i || clipped.dot(**n_j) >= 0.0 {
                continue;
            }
            clipped = clip_velocity(clipped, *n_j, overbounce);
            if clipped.dot(**n_i) >= 0.0 {
                continue;
            }
            // Pressed into both: slide along the crease between them.
            let crease = n_i.cross(**n_j).normalize_or_zero();
            clipped = crease * crease.dot(velocity);
            let third_blocks = planes.iter().enumerate()
                .any(|(k, n_k)| k != i && k != j && clipped.dot(**n_k) < 0.0);
            if third_blocks {
                return Vec3::ZERO;                   // a corner: stop
            }
        }
        return clipped;
    }
    velocity
}

pub fn clip_velocity(velocity: Vec3, normal: Dir3, overbounce: f32) -> Vec3 {
    let into = velocity.dot(*normal);
    if into >= 0.0 { velocity } else { velocity - *normal * into * overbounce }
}
```

`profile.overbounce` is 1.0 (Source) or 1.001 (Quake 3's `OVERCLIP`); the tiny extra stops the
character grinding along a wall it is pressed into.

#### 1.7.8 Step-up and ground snap

`step_move` is Source's `StepMove`, the variant bevy_ahoy also uses. It tries the move flat and
raised by `step_height`, and keeps one:

```rust
pub fn step_move(world: &dyn CollisionQuery, hull: &CharacterHull, profile: &MovementProfile,
                 from: Vec3, velocity: Vec3, dt: f32, ground: &GroundHit) -> MoveResult {
    let flat = kcc::slide(world, hull, profile, from, velocity, dt, &[ground.normal]);
    if profile.step_rule == StepRule::WhenBlocked && flat.contacts.is_empty() {
        return flat;                                 // Quake 3: only step when the flat move hit something
    }

    // Up by step_height, across, then back down by as much plus the probe distance.
    let up = world.cast_shape(hull, from, Vec3::Y * profile.step_height);
    let raised = from + Vec3::Y * profile.step_height * up.fraction;
    let across = kcc::slide(world, hull, profile, raised, velocity, dt, &[]);
    let drop = (raised.y - from.y) + profile.ground_probe_distance;
    let down = world.cast_shape(hull, across.position, Vec3::NEG_Y * drop);
    let landed = across.position - Vec3::Y * drop * down.fraction;

    let rejected = down.fraction >= 1.0                              // nothing to stand on
        || down.normal.y < profile.max_walk_slope_cos                // too steep
        || world.surface(down.entity).flags.contains(SurfaceFlags::NO_STEP)
        || profile.min_step_ledge_space   // None: any ledge will do (Quake, Source)
            .is_some_and(|space| !kcc::has_ledge_space(world, hull, landed, velocity, space));
    if rejected {
        return flat;
    }

    let stepped = MoveResult { position: landed, step_height: landed.y - from.y, ..across };
    let went = |r: &MoveResult| r.position.xz().distance_squared(from.xz());
    if profile.step_rule == StepRule::FartherHorizontal && went(&flat) >= went(&stepped) {
        return flat;                                 // Source: keep whichever went farther in XZ
    }
    stepped
}
```

`snap_down` is Source's `StayOnGround`. The step only calls it when the character was grounded at
the start of the tick. If the move left it in the air, it casts down by `ground_snap_distance`
(default: `step_height`) and, on walkable ground, moves down onto it. That keeps the character on
stairs and downward slopes instead of skipping off them:

```rust
pub fn snap_down(world: &dyn CollisionQuery, hull: &CharacterHull, profile: &MovementProfile,
                 moved: MoveResult) -> MoveResult {
    if kcc::ground_probe(world, hull, profile, moved.position, moved.velocity).is_some() {
        return moved;                                // still on the ground
    }
    let trace = world.cast_shape(hull, moved.position, Vec3::NEG_Y * profile.ground_snap_distance);
    if trace.fraction >= 1.0 || trace.normal.y < profile.max_walk_slope_cos {
        return moved;                                // a real ledge: let it fall
    }
    let drop = profile.ground_snap_distance * trace.fraction;
    MoveResult {
        position: moved.position - Vec3::Y * drop,
        step_height: moved.step_height - drop,       // negative: camera smooths the drop
        ..moved
    }
}
```

Step-up only runs while grounded, unless the profile sets `step_in_air` (arcade feel; off in all
lineage presets).

### 1.8 Apply and Report

**Apply** writes `BodyPose.position`, the world-space velocity, the body's yaw rotation (from
`ViewAngles.yaw`, so rollback restores rotation from yaw alone) and, from Part 3, the hull collider.

**Report** turns the tick's results into what other crates read (§9):

- `Locomotion`: world-space velocity, `grounded`, ground normal, `last_step_height` (from
  `events.step_height`), `stuck`;
- `EyeHeight = standing_height − eye_below_top` (Part 3 makes it follow stance);
- `MovementSpreadFactor`;
- `KinematicContacts` from `events.contacts`;
- facts: `Landed` (fall speed from `events.landed`, surface from `events.ground`; full shape in
  §9.2), and `MovementStateChanged` from `events.mode_changed`.

That completes Part 1: the character walks, climbs steps, slides along walls, stays glued to stairs
and falls off ledges.

---

## Part 2. Jumping

**Goal:** jump, with the rules that decide when a jump is allowed (held vs pressed, buffering, coyote
time), and landing consequences.

### 2.1 Added state

```rust
pub struct KinematicState {
    // … Part 1 fields …
    /// Ticks since the character was last grounded; 0 while grounded. Drives coyote time.
    pub ticks_since_grounded: u16,
    /// Ticks left in the jump buffer; 0 = no buffered jump.
    pub jump_buffer: u16,
    /// A jump was taken and the button has not been released since (`RequireRelease`).
    pub jump_held: bool,
}
```

**Every timer is a tick count**, never a `Stopwatch` or seconds accumulator. Profiles are written in
seconds and converted with `ticks(seconds, dt) = ceil(seconds / dt)` where they are used. This keeps
the step free of float time accumulation and makes rollback restore timers exactly. bevy_ahoy's
frame-time stopwatches are the counter-example ([`research/bevy-ecosystem.md`](../research/bevy-ecosystem.md)
§6.3).

### 2.2 Where the jump goes, and why first

The jump check is inserted **at the very start of the step, before friction**:

0. **Jump check** (§2.3). On a jump: set the vertical velocity and mark it as a jump tick. The move,
   re-probe and settle that follow treat the character as airborne.
1. **Friction**, only if the character was grounded *at the start of the tick* (from `Sense`), and
   skipped on a jump tick when `skip_friction_on_jump_tick` is set. The flag is what stops it: tick-start
   grounding alone would still run friction on the jump tick.
2. … the rest of Part 1, except that step 4 does not `snap_down` on a jump tick, and the
   re-probe in step 5 never reports ground on a jump tick.

The order is part of the contract. A replacement controller that wants the same feel must match it,
because feel lives in ordering, not constants (lineage pitfall 3): bunny-hopping only works because
a jump taken on the landing tick happens before friction can eat any speed.

### 2.3 Jump rules

```
can_jump = (grounded || ticks_since_grounded <= coyote_ticks)
        && stance allows it            // Part 3: never prone; crouched only if `crouch_jump`
        && stamina >= jump_stamina_cost  // Part 4, with MovementStamina

held    = command.held.contains(JUMP)
pressed = command.pressed.contains(JUMP)
wants = match jump_mode {
    RequireRelease   => held && !jump_held,              // Quake 3
    PressEdge        => pressed,                         // Source
    AutoHop          => held,
    Buffered { .. }  => pressed || jump_buffer > 0,
}
```

A `MOVE` gate needs no check here: `Prepare` has already removed `JUMP` from `command`.

- **`RequireRelease`** (Quake 3 `PMF_JUMP_HELD`, "don't pogo stick"): the button must have been
  released since the last *jump*. `jump_held` is set by a jump and cleared on any tick the button is
  up. Releasing in the air and pressing again jumps on the landing tick.
- **`PressEdge`** (Source, `m_nOldButtons & IN_JUMP`): the press must happen on the tick the jump is
  taken. Holding jump through a landing never jumps, so bunny-hopping needs a press timed to the
  landing tick (or a scroll-wheel bind).
- **`AutoHop`**: holding jump re-jumps on every landing (CS:GO-style servers, many modern games).
- **`Buffered { window }`**: a press up to `window` seconds before landing is kept in `jump_buffer`
  and taken on the landing tick.
- **Coyote time** (`coyote_time`, 0 in the lineage presets, ~0.1 s in `Arcade`): a jump is still
  allowed for a few ticks after walking off a ledge. Not after a jump: taking a jump sets
  `ticks_since_grounded` past the window.
- **Jump velocity.** `jump_impulse` is `Set` (Quake 3, Source: `v.y = max(v.y, jump_speed)`) or `Add`
  (QuakeWorld: `v.y += jump_speed`). `jump_speed` is a stat (§8.4). A RON stats file can give
  `jump_height` instead, and the loader derives `jump_speed = sqrt(2·g·h)` using the profile's gravity.
- **Bunny-hop control.** `skip_friction_on_jump_tick` (true in `Quake` and `Source`) lets a perfectly
  timed jump keep all its speed. Setting it false is the simplest anti-bhop rule; `bhop_speed_cap`
  (scale `v.xz` down to the cap on jump, CS:GO style) is the other.
- **No hidden boosts.** There is no HL2-style forward jump boost in the core step. A game that wants
  one adds a plugin that writes `ExternalImpulse`, capped along the velocity direction (lineage
  pitfall 11, accelerated back-hopping).
- **On a jump:** set `events.jumped`, spend stamina, set `jump_held`, clear `jump_buffer`.

At the end of the step, the timers advance: `ticks_since_grounded` resets to 0 if grounded and counts
up otherwise; `jump_buffer` counts down, or is filled on a press while airborne in `Buffered` mode;
`jump_held` clears if `JUMP` is not held.

### 2.4 Landing and fall damage

Landing was already detected in Part 1 (`events.landed`). Part 2 adds what follows from it:

- `Report` emits `Jumped` from `events.jumped`, and a `NoiseEvent` for jumps and landings.
- **Fall damage** (`FallDamage` plugin): on `Landed`, if `fall_speed > fall_safe_speed`, send a
  `DamageRequest` with `damage_type: Fall` and
  `amount = fall_damage_curve(fall_speed)` (linear by default, lethal at `fall_fatal_speed`; Source's
  `DAMAGE_FOR_FALL_SPEED`). The target is the character, there is no instigator (`krit_damage`'s
  `LastHitBy` credits whoever knocked them off), and the flags include `NO_KNOCKBACK`. Every peer
  emits the request, but only the authority's `Damage` set processes it (arch P12). Landing in water
  at `Waist` or deeper (Part 4) cancels fall damage.

---

## Part 3. Crouching and stance

**Goal:** crouch and prone, with a hull that changes size safely, an eye height that follows it, and
crouch-jumping.

### 3.1 Stance is not a mode

`Stance` says how tall the character is; `MovementState` says which physics runs. They are separate
because they combine: a character can be crouched in `Air` (crouch-jump), in `Slide`, or on the
`Ground`. Making `Crouch` a `MovementState` would need `CrouchAir`, `CrouchSlide` and so on.

```rust
// krit_core::locomotion
#[derive(Component, Clone, Copy, Default, Reflect, PartialEq, Eq, Debug)]
pub enum Stance { #[default] Standing, Crouched, Prone }
```

### 3.2 Added data

```rust
pub struct CharacterHull {
    // … Part 1 fields …
    pub crouched_height: f32,
    pub prone_height: f32,
}

pub struct KinematicState {
    // … Parts 1–2 …
    /// Ticks spent in the current stance transition. 0 = settled. The eye height is
    /// interpolated by stance_ticks / transition_ticks.
    pub stance_ticks: u16,
}
```

`Stance` itself is motion state (rolled back) and joins `MotionState`. It lives in `krit_core` because
`krit_kamera`, `krit_weapons` and hurtbox poses read it and may only depend on core.

### 3.3 Where stance goes in the step

The stance update is inserted **before the jump check**, so the jump rules and the move both see
this tick's stance and hull (Source runs `Duck()` before `FullWalkMove` for the same reason):

- **−1. Stance** (§3.4).
- 0. Jump check (Part 2), now gated by stance.
- 1.–7. Part 1, with the state speed chosen by stance: `walk_speed`, `crouch_speed` or `prone_speed`.

### 3.4 Stance rules

- **Crouching down.** `stance_ticks` counts up over `ticks(crouch_time)`. The *collision* hull
  switches to crouched only when the count completes (Source `FinishDuck`); until then the character
  keeps the larger hull, so it can never shrink into a space it cannot get out of. Crouch speed
  applies from the switch onward.
- **Standing up.** Cast the standing hull at the current pose. If blocked, stay crouched and try
  again next tick (the button is still released, so nothing extra is stored). If clear, switch the
  hull back immediately and let the eye rise over `ticks(uncrouch_time)`.
- **In the air**, `crouch_in_air` decides:
  - `ShrinkTop` (Quake 3): the top comes down, the feet stay put;
  - `TuckLegs` (Source): the feet come up by `standing − crouched` and the origin moves up with them,
    so the character clears obstacles that much higher (crouch-jumping). First cast to check the
    space above is free; if not, fall back to `ShrinkTop`.
- **Prone** works the same way with `prone_height` and `prone_time`, adds a `FIRE` gate during the
  transition, and forbids jumping. A prone hull is still upright (a short cylinder); a lying-down
  hull that rotates with yaw is left to a custom state (§13).
- A completed stance change is recorded so `Report` emits `StanceChanged` (comparing with
  `prev.stance`; recorded in `events.stance_changed`).

### 3.5 Eye height and the hull in Apply

- `EyeHeight = visual_height − eye_below_top`, where `visual_height` interpolates between the old and
  new stance heights by `stance_ticks`. `Report` recomputes it every tick from `Stance` and
  `stance_ticks`, so it never needs replicating.
- `Apply` gives the backend the hull for the current `Stance`; the backend swaps the body's collider
  when it changes. Because the origin is at the feet, no position fix-up is needed.
- Hurtboxes are separate colliders on their own layer (arch §6.3) and follow `Stance` through
  `HurtboxPose`, not the movement hull.

---

## Part 4. Sprint, stamina and the other modes

**Goal:** sprinting, stamina, and the modes that replace walk/air physics entirely: slide, ladder,
swim, mantle, attached, noclip and game-defined modes.

### 4.1 Sprint and stamina

**Sprint is a flag, not a mode**: it only changes speed, gates and stamina. The step computes it
each tick, and `Report` publishes it as `Locomotion.sprinting`. Sprint is on when:

- `SPRINT` is in `command.held` (so the `SPRINT` gate is clear),
- the character is grounded (or airborne with `sprint_in_air`),
- `move_axis.y ≥ sprint_min_forward` (default 0.5: no sprinting sideways or backwards),
- stance is `Standing`,
- and stamina is available.

While sprinting, the state speed is `sprint_speed` and movement contributes the `ADS` gate. Cancel
rules ("firing cancels sprint") live in `ActionRules` (arch §5.6), not here.

**`Stamina`** (`MovementStamina` plugin) is motion state: `{ current, regen_delay_ticks }`. The step
drains it while sprinting and on jumps, and regenerates it after `stamina_regen_delay`. Without the
plugin, `MotionState.stamina` is `None` and sprint is unlimited.

### 4.2 Modes and the Transition stage

The full `MovementState`:

```rust
// krit_core::locomotion
pub enum MovementState {
    Ground, Air,                // walk physics (Parts 1–3); settled by the step itself
    Slide, Ladder, Swim, Mantle,
    Attached { seat: Entity },
    Noclip,
    Custom(CustomMovementId),   // a game's own mode (§11, tier 2)
}
```

Modes that last several ticks need their own data, which lives in `KinematicState.mode_data`:

```rust
pub enum ModeData {
    None,
    Slide  { ticks: u16 },
    Ladder { normal: Dir3 },
    Mantle { start: Vec3, top: Vec3, ticks: u16, duration: u16 },
}
```

**`Transition`** decides which mode the step runs in this tick. It reads `Surroundings`,
`MoveCommand` and the current `MovementState`, and may run its own casts (mantle detection). It
never writes `MovementState`: it writes a `ModeRequest` (§1.4), with the mode's starting data, and
the step applies it before anything else (§1.7.2). Built-in transitions run in one system, in
priority order:

`Attached` > `Noclip` > `Mantle` (in progress) > `Ladder` > `Swim` > `Slide` > walk (`Ground`/`Air`)

```rust
// The built-in rules as a pure function, so a resimulation loop can call them too (§1.7.3).
pub fn transition(world: &dyn CollisionQuery, tuning: &Tuning, command: &MoveCommand,
                  sensed: &Surroundings, current: &MotionState) -> Option<ModeEntry>;

// MovementTransitionSystems::Builtin is a thin system around it.
fn builtin_transitions(world: B::Query, mut q: Query<(…, &mut ModeRequest)>) {
    for (…, mut request) in &mut q {
        request.0 = kcc::transition(&world.for_character(entity), &tuning, command, sensed, &current);
    }
}
```

Leaving a special mode requests `Air`; the walk physics then settles `Ground` or `Air`. Game systems
placed `.after(MovementTransitionSystems::Builtin)` in the same set see the built-in request and can
replace it or clear it. Whatever request is left when the set ends is the one the step applies, and
the step records one `events.mode_changed` from the tick's starting mode to its final one, so there
is exactly one `MovementStateChanged` per tick however many systems weighed in.

| State | Entered when | Physics | Leaves when | Gates | Plugin |
|---|---|---|---|---|---|
| `Ground` / `Air` | default | Parts 1–3 | a special mode is entered | – | `MovementCore` |
| `Slide` | crouch **pressed** while sprinting on the ground with `speed ≥ slide_min_speed` | §4.4 | `speed < slide_exit_speed`, `slide_max_time`, crouch released, or jump (keeps speed) | `SPRINT` | `MovementSlide` |
| `Ladder` | `sensed.ladder` and moving into it (`dot(wish_dir, −normal) > 0`), or airborne and touching it | §4.5 | input away from it, reached the top, or grounded while moving down | `ADS`, optionally `FIRE` | `MovementLadder` |
| `Swim` | `sensed.water ≥ Waist` | §4.6 | `water < Waist` | `SPRINT` | `MovementSwim` |
| `Mantle` | jump pressed (or held, with `auto_mantle`) with a valid ledge ahead | §4.7 | path finished | `FIRE`, `ADS`, `SWITCH` | `MovementMantle` |
| `Attached` | an `AttachRequest { seat }` (seats, turrets, vehicles, cutscenes) | none; pose follows the seat | `DetachRequest` | per seat | `MovementCore` |
| `Noclip` | `NoclipRequest`, only with `NoclipAllowed` (authority-granted) | §4.8 | `NoclipRequest` again | – | `MovementCore` |
| `Custom(id)` | a game system in `Transition` | the game's own step | the game's own system | game's choice | game |

### 4.3 The complete step

With every part in place, the step is:

```
−2. apply ModeRequest: mode, mode_data; on entry, e.g. slide_boost (§4.2)
if sensed.stuck: skip to "end of tick"
−1. stance                                    (Part 3)
dispatch on MovementState:
  Ground | Air | Slide:
     0. jump check                            (Part 2)
     1. friction (slide_friction in Slide)    (Part 1, 5: skipped while friction_lock > 0)
     2. accelerate, or VelocityOverride        (Part 1, §11; Slide: no wish accel)
     3. first half of gravity
     4. move: step_move + snap_down | slide
     5. probe ground again
     6. second half of gravity | clip to ground
     7. settle Ground/Air, record landing
  Ladder | Swim | Mantle | Noclip:  that mode's step (§4.5–4.8)
  Attached | Custom:                skipped (the seat or the game moves the character)
end of tick:
  leaving a mover → inherit its velocity      (Part 5)
  advance timers: ticks_since_grounded, jump_buffer, jump_held, friction_lock, stance, mode, stride
  sprint flag, stamina drain/regen
  record mode_changed / stance_changed from prev to final
```

`Report` runs for every mode, including `Custom`, so `Locomotion`, facts and spread keep working.

### 4.4 Slide

Ground physics with `slide_friction` instead of `friction` and no wish acceleration. On entry,
`slide_boost` is added along `v`. Slopes accelerate the slide because gravity is clipped to the
ground plane rather than removed. Stance is forced to `Crouched` for the duration.

### 4.5 Ladder

- Found by `ContentsFlags::LADDER` in a thin probe in front of the hull, or by a `Ladder { normal }`
  component on a sensor volume (for levels without contents brushes). `Sense` writes the normal to
  `Surroundings.ladder`.
- Follows Source's `LadderMove`: forward input climbs up when looking level or up and down when
  looking down past `ladder_look_down_pitch`; strafe moves along the ladder plane; there is no
  gravity and no friction. Speed is `ladder_speed`.
- Jump pushes off along the ladder normal at `ladder_jump_speed`.
- Reaching the top (no ladder contact at the feet while moving up) gives a small upward-and-forward
  pop so the character ends on the floor above instead of catching on the lip.

### 4.6 Swim

- `WaterLevel::{None, Feet, Waist, Eyes}` comes from `contents(WATER)` at three points (feet, hull
  middle, eye), like the lineage's `WL_Feet/Waist/Eyes`.
- The wish uses `command.look_dir`, including pitch; `JUMP` held adds upward wish, `CROUCH` held
  adds downward wish. `swim_speed`, `water_accel` and `water_friction` replace the ground values.
  With no input the character sinks at `water_sink_speed`, or floats at the surface if `buoyant`.
- At `Waist` with a wall ahead and jump held, an optional water jump launches the character out
  (Source `WaterJump`).
- `Locomotion.water_level` lets `krit_kamera` apply underwater effects and lets a game run oxygen.

### 4.7 Mantle

Detection runs in `Transition` only when a jump would otherwise happen (or every tick in the air with
`auto_mantle`), and uses three casts:

1. **Wall:** cast the hull forward by `mantle_reach` along the horizontal view direction. A hit with
   a near-horizontal normal is a candidate wall.
2. **Top:** cast a thin shape down from `max_mantle_height` above the feet, just past the wall. A
   walkable hit between `min_mantle_height` and `max_mantle_height` above the feet is the ledge top.
3. **Clearance:** cast the standing hull (falling back to the crouched hull, which forces
   `Crouched`) at the ledge top. It must fit.

On success, Transition requests `ModeEntry { mode: Mantle, data: ModeData::Mantle { start, top,
ticks: 0, duration } }`, and the step stores the data in `mode_data` when it applies the request. The
pose follows a fixed two-segment path (up to the ledge height, then forward onto it) as a function of
`ticks / duration`, so it is deterministic and rollback-safe without storing anything else.
Collision is not checked along the path because clearance was checked up front. The mode ends in
walk physics. `mantle_duration` comes from the profile, never from an animation length (arch §8:
the server does not animate).

### 4.8 Noclip

Velocity accelerates toward `look_dir × wish_amount × noclip_speed`, with `JUMP`/`CROUCH` for
up/down. No collision, no gravity, no ground. Fall damage and footsteps are suppressed. It is a
debugging and level-editing tool, so it is authority-granted (`NoclipAllowed`), and `krit_net`
treats a client's noclip request like any other intent.

---

## Part 5. Forces from the world

### 5.1 `ExternalImpulse`

```rust
// krit_core
#[derive(Component, Clone, Default, Reflect, PartialEq)]
pub struct ExternalImpulse {
    pub velocity: Vec3,        // summed velocity change to apply next step
    pub friction_lock: u16,    // ticks; the max of all contributions
    pub mode: ImpulseMode,     // Add (knockback, explosions) | SetVertical (jump pads)
}
```

- Ballistics, damage (`DamagePipelineSystems::Knockback`) and game code (jump pads, wind) add to it.
  Those run post-physics, so an impulse written on tick N is applied by movement on tick N+1. The
  one-tick delay is deterministic, so client and server agree on it.
- `Prepare` adds it to `KinematicState.velocity`, raises `KinematicState.friction_lock` to its value,
  and clears the component. Because this happens before `Sense`, a vertical component above
  `unground_speed` already makes the ground probe report airborne on the same tick.
- `friction_lock` (new `KinematicState` field, counted down at the end of the step) is Quake 3's
  `PMF_TIME_KNOCKBACK`: rocket jumps and knockback are not eaten by ground friction on the next few
  ticks. Q3 locks for 50–200 ms depending on the knockback.
- Velocity is the character's; there is no mass. Damage scales knockback by its own rules before
  writing (`SelfDamagePolicy.knockback_scale`).

### 5.2 Moving platforms

- A mover (platform, elevator, door, train) is any entity a game moves in `KritSimSystems::Movers`.
  It records what it did this tick in `MoverDelta { translation, rotation }` and its velocity in its
  backend body (exposed through `SurfaceInfo.linear_vel` / `angular_vel`).
- **Riding** happens in `Sense` step 2: if `KinematicState.ground` has a `MoverDelta`, add its
  translation, plus the displacement from rotating the character's position about the mover's
  centre, to `BodyPose`. The character's own `velocity` stays relative to the mover, so walking on a
  train feels like walking on the ground.
- Yaw is carried by the aim stage (§6, `AimSystems::Ride`), so the view turns with the platform.
  This is why `Movers` runs before `Aim`.
- **Leaving** a mover by jumping or walking off adds the mover's velocity at that point to
  `velocity` (`inherit_platform_velocity`, on by default), so momentum carries over.

### 5.3 Contacts and pushing dynamic bodies

- Every slide hit is in `events.contacts`. `Report` writes the deduplicated list to
  `KinematicContacts` (a read model) so game code can react to "touched a hazard" without its own
  casts.
- For contacts with dynamic bodies, `Report` emits `CharacterPush { character, other, point,
  impulse }`, with `impulse = −normal × impact_speed × push_strength`. The backend applies it after
  its step. The character is not slowed by the body (it is kinematic); props with the
  `NOT_PUSHABLE` surface flag behave as walls.
- Characters do not push each other. Character–character contact is a wall to the slide, so
  prediction never depends on another player's input.

---

## 6. Reference: the aim stage (`MovementLook`)

Runs in `KritSimSystems::Aim`, before movement, because `Prepare` uses `ViewAngles.yaw`. It is the
only writer of `ViewAngles` (arch §6.2).

1. **Ride** (`AimSystems::Ride`): if the character's ground entity has a `MoverDelta` with a yaw
   rotation, add that yaw to `ViewAngleOffset`.
2. **Compose** (`AimSystems::Compose`):
   ```
   yaw   = intent.aim.yaw.wrapping_add(offset.yaw)
   pitch = intent.aim.pitch.saturating_add(offset.pitch).clamp(-PITCH_LIMIT, PITCH_LIMIT)
   ```
   then apply `AimLimits` if present (seats and turrets), and write `ViewAngles`.
3. **`LOOK` gate.** While `LOOK` is blocked, the stage keeps `ViewAngles` where it was by rewriting
   `ViewAngleOffset = previous ViewAngles − intent.aim` every tick. When the gate clears, aim resumes
   from where it froze instead of snapping to wherever the mouse went. This is the same mechanism as
   forced facing, so it needs nothing new.
4. **Recoil** (`AimSystems::Recoil`): `AimRecoil` decays toward zero at the weapon's recovery rate,
   which weapons write into an `AimRecoilRecovery` component on the Character. Movement never
   inspects the weapon. `AimRecoil` is never folded into `ViewAngles`; `AimRay` adds it.
5. Optionally, as an anti-cheat check on the authority, `max_turn_rate` clamps how far
   `ViewAngles.yaw` may change in one tick (networked-shooter research §6.1). Off by default.

The stage writes neither `LocalAim` nor `intent.aim`.

## 7. Reference: every component on a Character

| Component | Kind (§0.2) | Crate | Holds | Written by | `SimStateClass` |
|---|---|---|---|---|---|
| `CharacterController` | config | movement | Marker, `profile_override` | game / spawn | `Replicated` |
| `MovementProfileHandle` | config | movement | `Handle<MovementProfile>` | game / archetype | `Replicated` |
| `CharacterHull` | config | movement | Shape and sizes (§1.2, §3.2) | game / archetype | `Replicated` |
| `BaseMovementStats` / `MovementStats` | config | movement | Speeds, accelerations, jump speed (§8.1) | `krit_stats` recompute | `Derived` |
| `MoveCommand` | command | movement | Gated, world-space wish and buttons (§1.4) | `Prepare` | `Derived` |
| `VelocityOverride` | command | movement | Game-supplied velocity (§11) | game | game's choice |
| `BodyPose` | surroundings | movement | Working copy of the body position (§1.4) | `Sense`, `Step` | `Derived` |
| `Surroundings` | surroundings | movement | Ground, water, ladder, stuck (§1.4) | `Sense` | `Derived` |
| `KinematicState` | motion | movement | Velocity, ground entity, timers, mode data (§7.1) | `Prepare`, `Sense`, `Step` | `OwnerPredicted` |
| `MovementState` | motion | **core** | The mode (§4.2) | `Step` only (Transition writes `ModeRequest`) | `OwnerPredicted` |
| `Stance` | motion | **core** | `Standing`, `Crouched`, `Prone` (§3) | `Step` | `OwnerPredicted` |
| `Stamina` | motion | movement | `current`, `regen_delay_ticks` (§4.1) | `Step` | `OwnerPredicted` |
| `ViewAngles` | motion | core | Actual facing | aim stage only | `OwnerPredicted`, interpolated for others |
| `ViewAngleOffset` | motion | core | Sim-imposed facing (spawn, teleport, platforms) | any sim system | `OwnerPredicted` |
| `AimRecoil` | motion | core | Shot-affecting recoil | weapons add, aim stage decays | `OwnerPredicted` |
| `StepEvents` | result | movement | What happened this tick (§1.4) | `Step` (from `StepResult`) | `Derived` |
| `ModeRequest` | command | movement | Mode (and its data) Transition wants this tick (§1.4) | `Transition`; taken by `Step` | `Derived` |
| `Locomotion` | result | **core** | Read model for other crates (§9.1) | `Report` | `Derived` |
| `EyeHeight` | result | core | Eye height above the feet | `Report` | `Derived` |
| `MovementSpreadFactor` | result | core | 0..1 movement inaccuracy for weapons | `Report` | `Derived` |
| `KinematicContacts` | result | movement | Entities touched this tick | `Report` | `Derived` |

`KinematicState` is private to `krit_movement`. Other crates read the `Locomotion` summary instead,
which also means a replacement movement crate only has to fill one small read model.

### 7.1 `KinematicState`, complete

```rust
#[derive(Component, Clone, Default, Reflect, PartialEq, MapEntities)]
#[reflect(SimState)]
pub struct KinematicState {
    pub velocity: Vec3,                 // Part 1. Relative to the mover while riding one
    #[entities] pub ground: Option<Entity>, // Part 1. For riding movers next tick
    pub ticks_since_grounded: u16,      // Part 2. Coyote time
    pub jump_buffer: u16,               // Part 2
    pub jump_held: bool,                // Part 2. RequireRelease
    pub stance_ticks: u16,              // Part 3. Progress of the current stance transition
    pub mode_data: ModeData,            // Part 4. Per-mode data
    pub friction_lock: u16,             // Part 5. Knockback friction skip
    pub stride: f32,                    // §9.3. Distance walked since the last footstep
    pub footsteps: u32,                 // §9.3. Cumulative footstep counter
}
```

## 8. Reference: profiles and stats

A preset is three values defined together: a `MovementProfile` (how the character moves, §8.3), a
`BaseMovementStats` (how fast and how strong before modifiers, §8.4) and a `CharacterHull` (§8.5).
Each is an associated constant on its own type (`MovementProfile::SOURCE`,
`BaseMovementStats::SOURCE`, `CharacterHull::SOURCE`), so a game can mix them, for example a `Source`
profile with its own speeds.

### 8.1 `MovementStats`

| Field | `StatId` | Base |
|---|---|---|
| `walk_speed`, `sprint_speed`, `crouch_speed`, `prone_speed`, `swim_speed`, `ladder_speed` | `move.speed.*` | `BaseMovementStats` |
| `ground_accel`, `air_accel` | `move.accel.*` | `BaseMovementStats` |
| `jump_speed` | `move.jump` | `BaseMovementStats` (a RON file may give `jump_height` instead, §2.3) |
| `gravity_scale` | `move.gravity_scale` | `BaseMovementStats`, 1.0 in every preset |
| `max_stamina`, `stamina_regen` | `move.stamina.*` | `BaseMovementStats` |

The profile holds no speeds. `BaseMovementStats` has the same fields as `MovementStats` and is never
modified; `krit_stats` recomputes `MovementStats` from it and the current modifiers (arch P7). Two
items that each multiply speed by 0.9 give 0.81 (`krit_stats` multiplies; arch §5.2).

### 8.2 `MovementProfile` fields

All durations are in seconds; the step converts them to ticks (§2.1).

| Group | Fields | Part |
|---|---|---|
| Gravity and air | `gravity`, `air_wishspeed_cap: Option<f32>`, `air_control` (Arcade only) | 1 |
| Ground | `friction`, `stop_speed`, `accel_model`, `decel` (Arcade only) | 1 |
| Collision | `step_height`, `step_rule`, `step_in_air`, `min_step_ledge_space`, `max_walk_slope_cos`, `ground_probe_distance`, `ground_snap_distance`, `unground_speed`, `overbounce`, `max_depenetration_iterations` | 1 |
| Jump | `jump_mode`, `jump_impulse`, `skip_friction_on_jump_tick`, `coyote_time`, `bhop_speed_cap`, `crouch_jump` | 2 |
| Fall | `fall_safe_speed`, `fall_fatal_speed`, `fall_damage_curve`, `land_effect_speed` | 2 |
| Stance | `crouch_time`, `uncrouch_time`, `prone_time`, `crouch_in_air` | 3 |
| Sprint and stamina | `sprint_min_forward`, `sprint_in_air`, `sprint_drain`, `jump_stamina_cost`, `stamina_regen_delay` | 4 |
| Modes | slide (`slide_min_speed`, `slide_exit_speed`, `slide_boost`, `slide_friction`, `slide_max_time`), ladder (`ladder_jump_speed`, `ladder_look_down_pitch`), swim (`water_accel`, `water_friction`, `water_sink_speed`, `buoyant`), mantle (`mantle_reach`, `min/max_mantle_height`, `mantle_duration`, `auto_mantle`), `noclip_speed` | 4 |
| Platforms and pushing | `inherit_platform_velocity`, `push_strength` | 5 |
| Spread | `spread_speed_ref: Option<f32>`, `spread_air` (§9.1) | – |

### 8.3 `MovementProfile` presets

How to read the preset tables (§8.3–§8.5):

- **†** marks a lineage constant, converted at 1 u = 0.0254 m. Everything else is a chosen starting
  value to tune, not taken from any reference.
- **`Quake` follows Quake 3** wherever QuakeWorld differs; QW values are shown in brackets for
  reference only.
- *Italic* values belong to a plugin the preset usually leaves off (§8.6). They are there only so
  the constant is complete and the plugin behaves sensibly if a game turns it on.
- **–** means the field is ignored by the preset's `accel_model`. The constant sets it to 0.

| Field | `Quake` | `Source` | `Arcade` | `Tactical` | `Immersive` |
|---|---|---|---|---|---|
| **Gravity and air** | | | | | |
| `gravity` (m/s²) | 20.32† | 20.32† (HL2: 15.24†) | 20 | 20 | 18 |
| `air_wishspeed_cap` (m/s) | none† [QW: 0.76†] | 0.76† | none | none | none |
| `air_control` (× `ground_accel`) | – | – | 0.3 | – | 0.1 |
| **Ground** | | | | | |
| `accel_model` | `QuakeProjection` | `QuakeProjection` | `Arcade` | `Proper` | `Arcade` |
| `friction` | 6† [QW: 4†] | 4† | – | 6 | – |
| `stop_speed` (m/s) | 2.54† | 2.54† | – | 2.54 | – |
| `decel` (m/s²) | – | – | 50 | – | 25 |
| **Collision** | | | | | |
| `step_height` (m) | 0.46† | 0.46† | 0.4 | 0.4 | 0.4 |
| `step_rule` | `WhenBlocked`† | `FartherHorizontal`† | `FartherHorizontal` | `FartherHorizontal` | `FartherHorizontal` |
| `step_in_air` | false | false | true | false | false |
| `min_step_ledge_space` (m) | 0 (no check) | 0 (no check) | 0.1 | 0.1 | 0.1 |
| `max_walk_slope_cos` | 0.7† | 0.7† | 0.7 | 0.7 | 0.7 |
| `ground_probe_distance` (m) | 0.05 | 0.05 (2 u†) | 0.05 | 0.05 | 0.05 |
| `ground_snap_distance` (m) | 0.46 | 0.46 | 0.4 | 0.4 | 0.4 |
| `unground_speed` (m/s) | 3.56 | 3.56† | 3.56 | 3.56 | 3.56 |
| `overbounce` | 1.001† | 1.0† | 1.0 | 1.0 | 1.0 |
| `max_depenetration_iterations` | 4 | 4 | 4 | 4 | 4 |
| **Jump** | | | | | |
| `jump_mode` | `RequireRelease`† | `PressEdge`† | `Buffered { window: 0.1 }` | `RequireRelease` | `Buffered { window: 0.15 }` |
| `jump_impulse` | `Set`† [QW: `Add`†] | `Set`† | `Set` | `Set` | `Set` |
| `skip_friction_on_jump_tick` | true† | true† | – | false | – |
| `coyote_time` (s) | 0† | 0† | 0.1 | 0 | 0.1 |
| `bhop_speed_cap` (m/s) | none | none | none | none | none |
| `crouch_jump` | true | true | true | false | true |
| **Fall** | | | | | |
| `fall_safe_speed` (m/s) | none (facts only) | 14.7† | none | 12 | 12 |
| `fall_fatal_speed` (m/s) | none | 26.0† | none | 22 | 22 |
| `fall_damage_curve` | *`Linear`* | `Linear`† | *`Linear`* | `Linear` | `Linear` |
| `land_effect_speed` (m/s) | 5.08 | 5.08† | 5.08 | 5.08 | 5.08 |
| **Stance** | | | | | |
| `crouch_time` (s) | 0† | 0.4† (HL2) | 0.15 | 0.3 | 0.25 |
| `uncrouch_time` (s) | 0† | 0.2† (HL2) | 0.15 | 0.3 | 0.25 |
| `prone_time` (s) | *0.8* | *0.8* | *0.8* | 0.8 | *0.8* |
| `crouch_in_air` | `ShrinkTop`† | `TuckLegs`† | `TuckLegs` | `ShrinkTop` | `TuckLegs` |
| **Sprint and stamina** | | | | | |
| `sprint_min_forward` | *0.5* | *0.5* | 0.5 | 0.5 | *0.5* |
| `sprint_in_air` | *false* | *false* | true | false | *false* |
| `sprint_drain` (/s) | *15* | *15* | *15* | 15 | *10* |
| `jump_stamina_cost` | *15* | *15* | *15* | 15 | 10 |
| `stamina_regen_delay` (s) | *1.0* | *1.0* | *1.0* | 1.0 | 1.5 |
| **Slide** | | | | | |
| `slide_min_speed` (m/s) | *6.0* | *6.0* | 6.0 | *4.5* | *4.0* |
| `slide_exit_speed` (m/s) | *3.0* | *3.0* | 3.0 | *2.5* | *2.0* |
| `slide_boost` (m/s) | *2.0* | *2.0* | 2.0 | *1.0* | *0.5* |
| `slide_friction` | *1.0* | *1.0* | 1.0 | *1.5* | *2.0* |
| `slide_max_time` (s) | *1.0* | *1.0* | 1.0 | *0.8* | *0.8* |
| **Ladder** | | | | | |
| `ladder_jump_speed` (m/s) | *6.86* | 6.86† | *5.0* | *4.0* | 4.0 |
| `ladder_look_down_pitch` (°) | *30* | 30 | *30* | *30* | 30 |
| **Swim** | | | | | |
| `water_accel` | *4*† | 10† | *8* | *6* | 4 |
| `water_friction` | *1*† | 1† | *1* | *1* | 2 |
| `water_sink_speed` (m/s) | *1.52*† | 1.52† | *1.0* | *1.5* | 1.0 |
| `buoyant` | *false* | false | *true* | *false* | true |
| **Mantle** | | | | | |
| `mantle_reach` (m) | *0.6* | *0.6* | 0.6 | *0.6* | 0.6 |
| `min_mantle_height` (m) | *0.5* | *0.5* | 0.5 | *0.5* | 0.5 |
| `max_mantle_height` (m) | *1.6* | *1.6* | 1.8 | *1.2* | 1.6 |
| `mantle_duration` (s) | *0.4* | *0.4* | 0.35 | *0.6* | 0.6 |
| `auto_mantle` | *false* | *false* | true | *false* | false |
| **Noclip** | | | | | |
| `noclip_speed` (m/s) | 10 | 10 | 10 | 10 | 10 |
| **Platforms and pushing** | | | | | |
| `inherit_platform_velocity` | true | true | true | true | true |
| `push_strength` | 1.0 | 1.0 | 1.0 | 1.0 | 1.0 |
| **Spread** | | | | | |
| `spread_speed_ref` (m/s) | none (= `walk_speed`) | none | none | none | none |
| `spread_air` | 1.0 | 1.0 | 1.0 | 1.0 | 1.0 |

Rows with the same value in every column (collision tolerances, noclip, platforms, spread) are
engine defaults rather than feel. A game is unlikely to tune them per preset, but they stay in the
profile so a single character can still override them.

### 8.4 `BaseMovementStats` presets

`ground_accel` and `air_accel` use the units of the profile's `accel_model`. For `QuakeProjection`
and `Proper` they are rates (per second, scaled by `wishspeed`). For `Arcade` they are m/s², and
the air rate is `ground_accel × air_control`, so `air_accel` is ignored.

| Field | `Quake` | `Source` | `Arcade` | `Tactical` | `Immersive` |
|---|---|---|---|---|---|
| `walk_speed` (m/s) | 8.13† | 8.13† (`sv_maxspeed`; HL2 walk: 4.83†) | 7.0 | 5.0 | 4.5 |
| `sprint_speed` (m/s) | *8.13* | *8.13* (HL2 suit sprint†) | 9.5 | 7.0 | *6.5* |
| `crouch_speed` (m/s) | 2.03† | 2.71† | 3.5 | 2.5 | 2.2 |
| `prone_speed` (m/s) | *1.0* | *1.0* | *1.0* | 1.0 | *0.8* |
| `swim_speed` (m/s) | *4.06*† | 6.50† | *4.5* | *3.0* | 3.0 |
| `ladder_speed` (m/s) | *5.08* | 5.08† | *4.0* | *2.5* | 2.5 |
| `ground_accel` | 10† | 10† | 60 (m/s²) | 10 | 20 (m/s²) |
| `air_accel` | 1.0† [QW: 0.7†] | 10† | – | 1 | – |
| `jump_speed` (m/s) | 6.86† | 6.81† (HL2: 4.06†) | 6.93 (≈ 1.2 m) | 4.90 (≈ 0.6 m) | 4.65 (≈ 0.6 m) |
| `gravity_scale` | 1.0 | 1.0 | 1.0 | 1.0 | 1.0 |
| `max_stamina` | *100* | *100* | *100* | 100 | 100 |
| `stamina_regen` (/s) | *20* | *20* | *20* | 20 | 15 |

The jump heights in brackets are `jump_speed² / 2g` with the preset's gravity.

### 8.5 `CharacterHull` presets

All presets use the default `Cylinder` shape (§1.2); `Box` with the same radius gives the lineage
AABB feel. `eye_below_top` is measured from the top of the current hull, so the crouched eye is
`crouched_height − eye_below_top`.

| Field | `Quake` | `Source` | `Arcade` | `Tactical` | `Immersive` |
|---|---|---|---|---|---|
| `radius` (m) | 0.38† | 0.41† | 0.40 | 0.35 | 0.35 |
| `standing_height` (m) | 1.42† | 1.83† | 1.80 | 1.80 | 1.75 |
| `crouched_height` (m) | 1.02† | 0.91† | 1.10 | 1.20 | 1.10 |
| `prone_height` (m) | *0.60* | *0.60* | *0.60* | 0.60 | *0.60* |
| `eye_below_top` (m) | 0.15† (eye 1.27) | 0.20† (eye 1.63 / 0.71) | 0.15 | 0.15 | 0.15 |
| `skin_width` (m) | 0.01 | 0.01 | 0.01 | 0.01 | 0.01 |

### 8.6 Plugins usually on

Every preset assumes `MovementCore`, `MovementLook` and `MovementCrouch`. On top of those:

| `Quake` | `Source` | `Arcade` | `Tactical` | `Immersive` |
|---|---|---|---|---|
| – | ladder, swim, fall damage | slide, mantle, sprint | sprint, stamina, prone, fall damage | ladder, swim, mantle, stamina |

The tick rate is part of the feel: Quake friction is linear per tick, so 30 Hz and 128 Hz feel
slightly different with the same numbers. Presets are tuned and tested at the default tick rate.

## 9. Reference: outputs and facts

### 9.1 Read models (components, rewritten every tick by `Report`)

| Component | Contents | Read by |
|---|---|---|
| `Locomotion` (core) | world-space `velocity`, `grounded`, `ground_normal`, `sprinting`, `water_level`, `last_step_height`, `footsteps`, `stuck` | kamera (bob, step smoothing, FOV), viewmodel (sway), feedback, AI, game code |
| `MovementState`, `Stance` (core) | §4.2, §3.1 | weapons (per-stance spread), kamera, hurtbox poses, animation |
| `EyeHeight` (core) | §3.5 | `AimRay`, kamera, interaction |
| `MovementSpreadFactor` (core) | 0..1 | weapons only |
| `KinematicContacts` (movement) | entities touched this tick | game code |

`MovementSpreadFactor`:

```
speed_term = clamp(|v.xz| / spread_speed_ref, 0, 1)     // spread_speed_ref defaults to walk_speed
air_term   = if grounded { 0 } else { spread_air }       // default 1.0
factor     = max(speed_term, air_term)
```

Stance is *not* folded in; weapons read `Stance` for per-stance base spread, so the two effects are
tuned separately.

### 9.2 Facts

All facts go through core's `emit_fact`: a `Message` for sim consumers, and an `EntityEvent` on the
Character for game code and presentation, skipped while resimulating (arch §4.1–4.2).

| Fact | Source in `StepEvents` | Typical consumers |
|---|---|---|
| `Jumped { tick }` | `jumped` | kamera, audio, AI |
| `Landed { tick, fall_speed, surface }` | `landed` | kamera (landing dip above `land_effect_speed`), audio, fall damage |
| `MovementStateChanged { from, to }` | `mode_changed` | animation, game code |
| `StanceChanged { from, to }` | `stance_changed` | hurtboxes, animation |
| `NoiseEvent` | `jumped`, `landed`, `footstep` | AI, stealth |
| `CharacterPush` | `contacts` with dynamic bodies | backend |
| `DamageRequest` (fall) | `landed` | damage |

### 9.3 Footsteps

Footsteps are simulated, not animated: `KinematicState.stride` accumulates horizontal distance while
grounded, and each time it passes the stance's stride length the step increments `footsteps` and sets
`events.footstep` (quieter crouched, none prone or in noclip). Presentation plays sounds from the
cumulative `Locomotion.footsteps` counter, so rollback never doubles a footstep.

## 10. Reference: plugins

`MovementPlugins<B>` contains these, each disableable with `.disable::<X>()`:

| Plugin | Adds | Depends on | Part |
|---|---|---|---|
| `MovementCore` | sets, the six stages, Ground/Air/Attached/Noclip, collide-and-slide, step, snap, jump, impulses, platforms, read models, facts | – | 1, 2, 5 |
| `MovementLook` | the aim stage (§6) | – | – |
| `MovementCrouch` | crouch stance and hull changes | core | 3 |
| `MovementProne` | prone stance | crouch | 3 |
| `MovementSprint` | sprint flag, gates | core | 4 |
| `MovementStamina` | `Stamina`, drain and regen; without it sprint is unlimited | sprint (optional) | 4 |
| `MovementSlide` | `Slide` | crouch, sprint | 4 |
| `MovementLadder` | `Ladder` | core | 4 |
| `MovementSwim` | `Swim`, `WaterLevel` | core | 4 |
| `MovementMantle` | `Mantle` | core | 4 |
| `FallDamage` | fall `DamageRequest`s | core | 2 |
| `MovementDebug` (feature `debug`) | gizmos for the hull, casts, ground normal and contacts | core | – |

Disabling a plugin removes its transitions, so a mode it owns is never entered and the profile
fields it uses are ignored.

## 11. Reference: extending it, in three tiers

Following the Unity Character Controller split ([`research/unity-dots-online-fps.md`](../research/unity-dots-online-fps.md) §14):

**Tier 1: library steps.** Every building block the built-in step uses is `pub` in
`krit_movement::kcc`, takes plain data and `&dyn CollisionQuery`, and has no ECS access:

```rust
pub fn sense(world, tuning, position: Vec3, kinematic: &KinematicState) -> (BodyPose, Surroundings);  // §1.7.3
pub fn depenetrate(world, hull, position: Vec3, profile) -> Depenetration;     // { position, stuck }
pub fn ground_probe(world, hull, profile, position: Vec3, velocity: Vec3) -> Option<GroundHit>;
pub fn friction(velocity: Vec3, ground: &GroundHit, profile, dt) -> Vec3;      // §1.7.5
pub fn accelerate(model, velocity, wish_dir, wish_speed, params: &AccelParams, dt) -> Vec3;  // §1.7.6
pub fn slide(world, hull, profile, from: Vec3, velocity: Vec3, dt, extra_planes: &[Dir3]) -> MoveResult;  // §1.7.7
pub fn clip_velocity(velocity: Vec3, normal: Dir3, overbounce: f32) -> Vec3;
pub fn step_move(world, hull, profile, from: Vec3, velocity: Vec3, dt, ground: &GroundHit) -> MoveResult;  // §1.7.8
pub fn snap_down(world, hull, profile, moved: MoveResult) -> MoveResult;
pub fn wish(move_axis: Vec2, yaw: u32) -> (Vec3, f32);   // (wish_dir, wish_amount); yaw is the raw u32, converted to radians inside (§1.7.4)
```

A game writing its own controller (a hover vehicle, a wall-runner) calls these and gets the same
collision behaviour as the built-in one.

**Tier 2: custom modes.** A game adds a mode without forking:

```rust
const WALL_RUN: CustomMovementId = CustomMovementId(1);

// MovementSystems::Transition, after the built-in transitions
fn enter_wall_run(mut q: Query<(&MovementState, &mut ModeRequest, &Surroundings, &KinematicContacts, &MoveCommand)>) {
    for (mode, mut request, sensed, contacts, command) in &mut q {
        let built_in_wants_something = request.0.is_some();
        if *mode == MovementState::Air && !built_in_wants_something && /* touching a wall, holding forward */ {
            request.0 = Some(ModeEntry { mode: MovementState::Custom(WALL_RUN), data: ModeData::None });
        }
    }
}

// MovementSystems::Step, after the built-in step. The built-in step applies the request, records
// the mode change, and does not move characters in Custom modes.
fn wall_run_step(world: CollisionParam<B>, mut q: Query<(&MovementState, &mut BodyPose, &mut KinematicState, &mut StepEvents, &mut WallRunState, …)>) {
    // use kcc::slide, kcc::ground_probe … and update BodyPose, velocity and StepEvents
    // (add to StepEvents rather than replacing it, so the recorded mode change survives)
}
```

Leaving the custom mode works the same way: the game's Transition system requests `Air` (or another
mode), and the built-in step takes over on that tick.

A custom mode's data lives in the game's own components (`WallRunState`), tagged
`#[reflect(SimState)]`, so it rolls back with everything else. This is why the extension point is "a
mode id plus your own systems" rather than a `dyn` trait registry: a trait object could not read the
game's components, and its internal state would be invisible to rollback. `Apply` and `Report` still
run, so the custom step only has to update `BodyPose`, the velocity and `StepEvents`.

**Tier 3: the velocity slot.** For one-off motion inside the built-in walk modes, insert
`VelocityOverride { velocity, mode: Replace | Add, ticks }`. `Prepare` copies it into
`MoveCommand.velocity_override` and counts `ticks` down; the step uses it in place of wish
acceleration and still slides, steps and grounds. Dashes, grapple pulls and scripted shoves use this.

**Grounding rules.** Data in the profile (`max_walk_slope_cos`, `unground_speed`, probe distance)
plus per-collider `SurfaceInfo.flags` (`NOT_WALKABLE`, `NO_STEP`, `SLIPPERY`, `NOT_PUSHABLE`). A
game that needs a per-surface rule beyond flags gives the collider its own walkable limit in
`SurfaceInfo`. This is the `GroundingPolicy` hook named in §5.4 of the summary.

## 12. Reference: networking and determinism

**Rolled back** (owner-predicted) and replicated (others), versus **recomputed every tick** (never
sent): this is the §0.2 split, made concrete.

| Rolled back | Recomputed every tick |
|---|---|
| backend body position (avian `Position`) | `BodyPose`, `Surroundings`, `MoveCommand`, `ModeRequest`, `StepEvents` |
| `KinematicState` | ground normal, water level, ladder contact |
| `MovementState`, `Stance`, `Stamina` | `EyeHeight`, `Locomotion`, `MovementSpreadFactor`, `KinematicContacts` |
| `ViewAngles`, `ViewAngleOffset`, `AimRecoil` | `MovementStats` (recomputed from modifiers) |

- Remote characters receive position, `ViewAngles`, `MovementState`, `Stance` and velocity,
  interpolated. They are not stepped from stale intents (`krit_net`'s `Simulate` filter).
- Every rolled-back component derives `PartialEq`; `KinematicState` and positions compare with a
  small tolerance so float noise in casts doesn't trigger needless rollbacks.
- `KinematicState.ground` holds an `Entity`, so it derives `MapEntities` (bevy_ahoy's state does
  not, which breaks replication).
- Nothing in the step reads `Time`, frame `dt`, `Entity` order or a resource of per-entity state.
  `dt` comes from `Time<Fixed>`. Characters are stepped independently, so iteration order cannot
  change results.
- If `krit_net` quantizes replicated positions or velocities, `Report` applies the same quantization
  at the end of every step on every peer (lineage pitfall 1: Quake 3's `SnapVector`).
- Cross-platform bit-identical results also need deterministic trig and casts (avian's
  `enhanced-determinism`, `libm` for `sin`/`cos`). Without them, client and server on different
  platforms correct each other slightly more often; same-platform play is unaffected.
- Spawns and teleports set a `Discontinuity` marker for one tick, so lag compensation never rewinds
  across them and kamera snaps instead of smoothing (lineage pitfall 7).

## 13. Reference: testing

Unit tests call `step` directly with a `TestWorld`: an in-crate `CollisionQuery` over analytic shapes
(planes, boxes, ramps, stairs, water and ladder volumes) behind a `testing` feature, and a
hand-built `MoveCommand` and `Surroundings`. A `testing::tick()` helper runs sense + transition +
step in order for multi-tick tests. No physics engine is involved, so tests are fast, exact, and keep
the crate in the logic tier.

**Known-number tests**, grouped by the part they cover:
- *Part 1:* ground speed converges to `walk_speed` and never exceeds it; stopping distance from top
  speed matches the closed form of the friction formula; with `QuakeProjection` and an air cap,
  ideal strafing gains speed every tick, and with `Proper` it never exceeds `wishspeed`; a 0.45 m
  step is climbed and a 0.47 m step blocks (step height 0.46 m); a 44° ramp is walkable and a 47°
  ramp slides; walking down stairs stays grounded every tick.
- *Part 2:* jump apex ≈ `jump_speed² / 2g`, within `g·dt²/8`; a perfectly timed re-jump loses no
  horizontal speed with `skip_friction_on_jump_tick`, and one tick of friction without it; with
  `RequireRelease` and `PressEdge`, holding jump through a landing does not jump again.
- *Part 3:* a `TuckLegs` crouch-jump clears an obstacle `standing − crouched` higher than
  `ShrinkTop`; standing up under a low ceiling stays crouched, and stands on the first tick there is
  room.

**Trajectory (golden) tests.** Scripted intent sequences (circle-strafe, bhop chain, stair run, ledge
mantle) are stepped for a few hundred ticks and the full pose/velocity trace is compared against a
stored file. These catch the ordering changes that constant checks miss.

**Determinism and rollback tests.** Run the same sequence twice and require bit-identical state;
then save state at tick N, step to N+k, restore N, resimulate, and require the same result.

**Backend parity test** (in `krit_avian`): the golden trajectories, replayed against real avian
colliders through `AvianCollisionQuery`, stay within a small tolerance of the `TestWorld` result.

## 14. Decisions that resolve earlier contradictions

Earlier drafts of this document, the §5.4 summary and `architecture.md` disagreed on these points.
This is what was decided, and the summary and architecture now follow it.

| Topic | Was | Now | Why |
|---|---|---|---|
| Crouch / prone | `MovementState` variants (summary, code) | `Stance` values, orthogonal to the mode (§3.1) | Crouch-jump and crouch-slide must be expressible |
| `MovementState`, `Stance`, `Locomotion` | owned by movement | in `krit_core` | kamera, weapons and feedback read them and may only depend on core |
| `KinematicState` contents | velocity, `grounded`, ground normal, `last_step_height`, coyote timer | velocity, ground entity, tick timers, mode data (§7.1) | Only motion state is rolled back (§0.2); ground facts are re-measured, `last_step_height` is a per-tick result |
| Ground detection timing | inside the step, after `Transition` had already used it | in `Sense`, before `Transition`; re-probed inside the step after the move (§1.6) | Transitions need this tick's ground and water |
| Who sets `MovementState` | `Transition` | only the step: it settles `Ground`/`Air` after the move and applies Transition's `ModeRequest` (§1.7, §4.2) | `Ground`/`Air` is only known after moving; one writer means the step sees the tick's starting mode and records the change itself (no `TickStart`-style snapshot) |
| Step outputs | `MoveOutput` component *and* `StepReport` return value *and* `(state, MoveOutput)` | step returns `StepResult { state: MotionState, events: StepEvents }`; the caller writes it to components or feeds it back in (§1.7) | One transition function serves the live tick, resimulation, bots and tests |
| Step signature | `step(state, intent, stats, &impl CollisionQuery, dt)` / `step(ctx: &StepContext, …)` / `step(…, motion: &mut Motion, events: &mut StepEvents, …)` | `step(world, tuning, command, request, sensed, prev: &MotionState, dt) -> StepResult` (§1.7.1) | Each argument is one kind of data from §0.2; nothing is mutated in place |
| Profile → ticks | a per-tick `Prepare` job *and* a once-on-load conversion | converted where used, `ticks(seconds, dt)` (§2.1) | No cached derived profile to keep in sync |
| Stance progress | `stance_progress: u16` documented as "0 to 1" | `stance_ticks: u16`, a tick count (§3.2) | An integer cannot hold a 0..1 fraction |
| `jump_mode` | `RequireRelease \| AutoHop \| Buffered` | adds `PressEdge` (Source) | Differs from `RequireRelease` when jump is held through a landing |
| Order of `Movers` and `Aim` | `Aim` then `Movers` (architecture §4 text) | `Movers` then `Aim` (as `KritKore` already chains them) | Platform yaw reaches `ViewAngles` on the same tick |
| Stuck handling | "skip collision response" | don't move the character this tick (§1.6) | Skipping collision while embedded is how tunnelling happens |
| Fact name | `StateChanged` | `MovementStateChanged`; new `StanceChanged` | Two kinds of change, two facts |
| `CollisionQuery` | generic `&impl` | object-safe, trace-shaped hits (`fraction`, `normal`, `entity`, `start_solid`), plus `SurfaceInfo.flags` and `MoverDelta` | Tests and custom steps pass any implementation |

New core types this design needs: `PhysicsBackend`, `ExternalImpulse { velocity, friction_lock,
mode }`, `AimRecoilRecovery`, `AimLimits` (yaw/pitch clamp relative to a seat, mirrored by
`krit_input` like `PITCH_LIMIT`), `Discontinuity`, `WaterLevel`, `ContentsFlags::LADDER`,
`MovementState`, `Stance`, `Locomotion`.

## 15. Implementation status

The code in `crates/krit_movement` is an early prototype. Where it differs from this document:

| Code today | This document |
|---|---|
| `KritMovement` adds `KritAimPlugin` and `KritMovementPlugin` | `MovementPlugins<B>` with the plugins in §10 |
| `MovementSystems::{Prepare, Sense, Transition, Step, Apply, Report}` | same; matches |
| `AimSystems::{Ride, Compose, Recoil}` | same; matches |
| `StepContext { intent, transform }`, `StepReport { transform }` | `Tuning` + `MoveCommand` + `ModeRequest` + `Surroundings` + `MotionState` in, `StepResult` out (§1.7.1) |
| `handle_movement` writes `Transform` directly | `Apply` writes the backend body; `Transform` is presentation |
| `step` adds `move_axis` as `(x, 0, y)` with no yaw, speed or `dt` | wish from `move_axis` and yaw (§1.7.4); matches (`normalize_or_zero`, forward is −y) |
| `KinematicState { velocity, grounded, ground_normal: Vec2, last_step_height }` | §7.1 (and a ground normal would be a `Dir3`, not a `Vec2`) |
| `MovementState::{Ground, Air, Crouch, NoClip}` in `krit_movement` | §4.2 in `krit_core`; `Crouch` is a `Stance`; `Noclip` |
| `CharacterController {}`, `MovementProfile {}` empty | §1.1, §8.2 |
| the core crate is named `krit_kore` | the docs call it `krit_core` |

## 16. Open questions

- **Platform yaw and `LocalAim`.** Carrying yaw through `ViewAngleOffset` is exact, but the camera's
  pending term (arch §5.12) shows mouse input on top of the *last tick's* yaw. On a fast-spinning
  platform the camera may visibly step at tick rate. Does kamera need to extrapolate
  `ViewAngleOffset` between ticks?
- **Stamina as a generic pool.** Stamina, jetpack fuel, weapon heat and shields are all
  `ResourcePool { current, max, regen, regen_delay }` (FPS Microgame research §13). Should that type
  live in `krit_stats`, with movement only owning the drain rules?
- **Prone hull.** An upright short cylinder lets players poke through low walls they appear to lie
  behind. A yaw-aligned box is better, but changes shape when turning, which can embed the hull.
- **Character–character collision.** Treating other characters as walls is simple and predictable,
  but feels sticky in crowded co-op. Is a soft separation (a small `ExternalImpulse` between
  overlapping characters, authority-only) worth adding?
- **Surf.** The `Source` preset surfs on steep ramps, as in CS:S. Should there be an explicit `Surf`
  preset with tuned `air_accel` and `overbounce`, or is that left to games?
- **`Arcade` model and slopes.** `Arcade` moves toward a target velocity, which on a slope fights
  gravity in a way the Quake models don't. It needs a slope rule (project the target onto the ground
  plane) and its own golden tests.
