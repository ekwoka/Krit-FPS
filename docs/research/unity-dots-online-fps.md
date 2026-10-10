# Research: Unity DOTS OnlineFPS (Character Controller samples) & Netcode for Entities FPS templates

> Scope: the **OnlineFPS** sample in `Unity-Technologies/CharacterControllerSamples` (read from source),
> the **Unity Character Controller** package (`com.unity.charactercontroller`) processor pattern, and
> the Netcode for Entities (N4E) concepts it relies on. The **Competitive Action Multiplayer**
> template and **Megacity Metro** are covered only briefly, because their source is not public
> (see §1.3).
> All type names below were read from the C# source at the commit cloned on 2026-10-11, unless a
> line explicitly says *unverified*.

---

## 1. Overview

### 1.1 What it is

OnlineFPS is Unity's reference **server-authoritative, client-predicted** arena FPS built on
Entities + Unity Physics + Netcode for Entities + the ECS Character Controller. It is small, at
about 6.9k lines of C# across 78 files. It covers moving, looking, three hitscan weapons
(machine gun, railgun, shotgun), two projectile weapons (rocket launcher, plasma gun), health,
death, a timed respawn, spectators, thin clients and name tags. It has **no** inventory, weapon
switching, reload/ammo, teams, score or match phases.

| Item | Value (from `OnlineFPS/Packages/manifest.json`, `NetCodeConfig.asset`) |
|---|---|
| `com.unity.netcode` | 1.14.1 |
| `com.unity.charactercontroller` | 1.4.5 |
| `com.unity.entities.graphics` | 1.4.21 |
| `SimulationTickRate` / `NetworkTickRate` | 60 / 60 |
| `PredictedFixedStepSimulationTickRatio` | 1 (physics runs once per sim tick) |
| `MaxSimulationStepsPerFrame` / `MaxSimulationStepBatchSize` | 1 / 4 |
| `InterpolationTimeNetTicks` | 2 |
| `MaxPredictAheadTimeMS` | 500 |
| `TargetCommandSlack` | 2 |

### 1.2 Version drift you will hit when reading other write-ups

- **`KinematicCharacterAspect` is gone from the sample.** In Character Controller 1.4.1 every
  `KinematicCharacterAspect` method moved into static `KinematicCharacterUtilities` functions,
  because Entities deprecated `IAspect`. Processors now hold a `KinematicCharacterDataAccess` field
  instead of the aspect. The aspect remains only for backwards compatibility, and it is ignored on
  editor 6000.5+ ([CC 1.4 changelog](https://docs.unity3d.com/Packages/com.unity.charactercontroller@1.4//changelog/CHANGELOG.html),
  [forum: aspects-less approach](https://discussions.unity.com/t/new-aspects-less-approach-in-com-unity-charactercontroller-1-4-1/1693777)).
  The sample's own `character-and-camera.md` still says "`FirstPersonCharacterAspect.VariableUpdate`",
  which is stale. The code uses `FirstPersonCharacterProcessor`.
- **Weapon type names changed between sample versions.** The current code has `BaseWeapon`,
  `WeaponControl`, `WeaponProjectileEvent`, `RaycastWeapon`, `PrefabWeapon`,
  `RaycastWeaponVisualProjectileEvent`, `WeaponShotSimulationOriginOverride` and
  `WeaponShotIgnoredEntity`. Older 1.0-era names circulate online, such as a "WeaponShotSimulation"
  type or "StandardRaycastWeapon". I did not verify them against old source, and this document does
  not rely on them. The task brief's "WeaponShotSimulation" survives today only as
  `WeaponShotSimulationOriginOverride`.
- The input component is **`FirstPersonPlayerCommands : IInputComponentData`** in the sample. The
  networking tutorial calls the same thing `FirstPersonPlayerInputs`.

### 1.3 Related Unity projects (secondary)

- **Competitive Action Multiplayer template** (`com.unity.template.multiplayer-netcode-for-entities`
  1.0, Dec 2024): an FPS loop of "fire a weapon at other players, die, re-spawn, and restart", with
  the "Networked Character Controller", "shotgun spread, the hit scan weapon, the projectiles, and the
  explosives", and "lag compensation". It adds UGS Relay/Lobby/Multiplay, a `GameManager`,
  `GameConnection`, `ClientConnectionSystem` and `ServerBootstrap`
  ([manual](https://docs.unity3d.com/Packages/com.unity.template.multiplayer-netcode-for-entities@1.0/manual/index.html),
  [announcement](https://discussions.unity.com/t/competitive-action-multiplayer-template-now-released-netcode-for-entities/1573137)).
  The weapon set matches OnlineFPS exactly, and the docs list the OnlineFPS sample under
  "Additional Resources". **Inference (unverified):** its gameplay layer is derived from OnlineFPS.
  The source ships only through Unity Hub, so I did not read it. It is "no longer supported in
  6000.3", and the newer **Multiplayer FPS template** (`com.unity.template.multiplayer-fps@1.0`)
  replaces it ([manual](https://docs.unity3d.com/Packages/com.unity.template.multiplayer-fps@1.0/manual/index.html)).
- **Megacity Metro**: a 64+ player N4E vehicle shooter. Public material covers infrastructure
  (Game Server Hosting, Matchmaker, Vivox), not gameplay architecture
  ([Unity blog](https://unity.com/blog/games/megacity-competitive-action-multiplayer-sample-game)).
  I did not use it for any claim below.

### 1.4 Source links

- Repo: <https://github.com/Unity-Technologies/CharacterControllerSamples> (`OnlineFPS/Assets/Scripts/**`)
- Sample docs: `_Documentation/Samples/OnlineFPSSample/{game-management,character-and-camera,weapons}.md`
- Netcode tutorial: `_Documentation/Tutorial/tutorial-netcodecharacters*.md`

---

## 2. Structure

### 2.1 Folder layout (`OnlineFPS/Assets/Scripts`)

| Folder | Contents |
|---|---|
| `Player/` | `FirstPersonPlayer`, `FirstPersonPlayerCommands` (`IInputComponentData`), `FirstPersonPlayerNetworkInput`; `FirstPersonPlayerInputsSystem`, `…VariableStepControlSystem`, `…FixedStepControlSystem` |
| `Character/` | `FirstPersonCharacterComponent` (tuning + replicated angles), `FirstPersonCharacterControl` (per-tick control), `FirstPersonCharacterView`, `OwningPlayer`, `CharacterInitialized`; `FirstPersonCharacterProcessor` (`IKinematicCharacterProcessor<FirstPersonCharacterUpdateContext>`); systems for physics update, variable update, predicted/interpolated rotation rebuild, presentation-only roll |
| `Camera/` | `MainEntityCamera` (tag + FoV on the view entity), `MainGameObjectCamera` (managed `Camera`), `MainCameraSystem` |
| `Weapons/` | `BaseWeapon`, `WeaponControl`, `WeaponOwner`, `ActiveWeapon`, `WeaponProjectileEvent` buffer, `RaycastWeapon`, `PrefabWeapon`, `RaycastWeaponVisualProjectileEvent` buffer, `WeaponVisualFeedback`, `CharacterWeaponVisualFeedback`, `WeaponShotSimulationOriginOverride`, `WeaponShotIgnoredEntity` buffer; `Projectiles.cs` (`PrefabProjectile`, `RaycastProjectile`, `RaycastVisualProjectile`, `ProjectileSpawnId`); systems `ActiveWeaponSystem`, `WeaponsSimulationSystem`, `ProjectileSimulationSystem`, visuals systems, `ProjectileClassificationSystem` |
| `Weapons/Specific/` | per-type behaviour: `Rocket`, `ProjectileBullet`, `BulletShotVisuals`, `LazerShotVisuals`, `MachineGunVisuals` + systems |
| `Misc/` | `Health`, `CharacterDeathSystem` (server + client), `DelayedDespawn`(+System), `SpawnPoint`, `SpectatorSpawnPoint`, spectator controller, `VFXSystem` (`VFXManager<T>` request queues), `NameTagUpdateSystem`, `InputDeltaUtilities`, `GhostVariants` |
| `GameManagement/` | `GameResources` (baked singleton) + `GameResourcesWeapon` buffer, `ServerGameSystem`, `ClientGameSystem`, `GameWorldSystem`, `GameSession`, `GameManager` (MonoBehaviour), `GameBootstrap : ClientServerBootstrap`, RPCs `JoinRequest`, `RespawnMessageRequest` |

### 2.2 Worlds

`GameBootstrap : ClientServerBootstrap` plus `GameManager`/`GameSession` create **one to many ECS
worlds per process**: client, server, thin clients and a local "offline" world. A `GameSession`
groups the worlds that play together, so that subscenes load into all of them and all of them are
torn down together. Systems choose their worlds with `[WorldSystemFilter(...)]`
(`ClientSimulation`, `ServerSimulation`, `ThinClientSimulation`). Inside shared jobs, behaviour
branches on `state.WorldUnmanaged.IsServer()`.

### 2.3 System groups and data flow

```mermaid
flowchart TD
  subgraph CLIENT_ONLY_INPUT["GhostInputSystemGroup (client only)"]
    PIS[FirstPersonPlayerInputsSystem<br/>devices → FirstPersonPlayerCommands<br/>on Player, WithAll GhostOwnerIsLocal]
  end

  subgraph PRED["PredictedSimulationSystemGroup (client+server; re-runs on rollback; partial ticks on client)"]
    BR[BuildCharacterPredictedRotationSystem<br/>OrderFirst: LocalTransform.Rotation ← CharacterYDegrees]
    AW[ActiveWeaponSystem<br/>OrderFirst: on ActiveWeapon change → parent to socket,<br/>shot origin = ViewEntity, WeaponOwner, ignore self]
    subgraph FIX["PredictedFixedStepSimulationSystemGroup"]
      FSC[FirstPersonPlayerFixedStepControlSystem<br/>OrderFirst: Commands.Move/Jump → CharacterControl.MoveVector/Jump]
      subgraph PHYS["PhysicsSystemGroup → KinematicCharacterPhysicsUpdateGroup"]
        CPU[FirstPersonCharacterPhysicsUpdateSystem<br/>Processor.PhysicsUpdate: KCC steps]
      end
    end
    VSC[FirstPersonPlayerVariableStepControlSystem<br/>look delta = Commands.LookYawPitch − LastProcessed<br/>→ CharacterControl.LookYawPitchDegreesDelta<br/>Commands.Shoot/Aim + InterpolationDelay → WeaponControl on active weapon]
    CVU[FirstPersonCharacterVariableUpdateSystem<br/>Processor.VariableUpdate: integrate yaw/pitch,<br/>clamp pitch; view entity rotation]
    subgraph WPG["WeaponPredictionUpdateGroup"]
      WSS[WeaponsSimulationSystem<br/>BaseWeaponSimulationJob → WeaponProjectileEvent buffer<br/>RaycastWeaponSimulationJob: lag-comp raycast + server damage<br/>PrefabWeaponSimulationJob: predicted projectile spawn]
    end
    subgraph PPG["ProjectilePredictionUpdateGroup"]
      PSS[ProjectileSimulationSystem → HasHit/HitEntity]
      RKT[RocketSimulationSystem / ProjectileBulletSimulationSystem<br/>server: Health -= dmg, AoE OverlapSphere]
    end
    CDS[CharacterDeathServerSystem server only<br/>Health.IsDead → RPC RespawnMessageRequest,<br/>CharacterSpawnRequest entity, DelayedDespawn on]
  end

  subgraph SIM["SimulationSystemGroup (non-predicted, once per frame)"]
    SGS[ServerGameSystem: joins, CharacterSpawnRequest countdown,<br/>spawn Character+Weapon at random SpawnPoint]
    CGS[ClientGameSystem: join RPC, character setup, respawn screen]
    WVG[WeaponVisualsUpdateGroup client: Initialize/Finalize LastVisual*,<br/>BaseWeaponShotVisuals recoil, RaycastWeaponProjectileVisuals,<br/>CharacterWeaponVisualFeedback bob/recoil/FoV]
    PVG[ProjectileVisualsUpdateGroup client: Rocket/Bullet VFX requests]
    INT[BuildCharacterInterpolatedRotationSystem client, remote chars<br/>+ FirstPersonCharacterPresentationOnlySystem view roll]
    DDS[DelayedDespawnSystem: hide/disable collision, server destroys after N ticks]
    CDC[CharacterDeathClientSystem: death VFX once]
    VFX[VFXSystem OrderLast: flush VFXManager queues → VFX Graph]
    GWS[GameWorldSystem OrderLast: ECS → managed UI bridge]
  end

  subgraph PRES["PresentationSystemGroup"]
    MCS[MainCameraSystem: GameObject Camera ← LocalToWorld of MainEntityCamera + CurrentFoV]
  end

  PIS --> BR --> AW --> FSC --> CPU --> VSC --> CVU --> WSS --> PSS --> RKT --> CDS
  CDS -. RPC/request entity .-> SGS
  SGS -. spawns Character ghost, sets Player.ControlledCharacter .-> BR
  WSS -. TotalShotsCount / visual events (ghost fields) .-> WVG
  RKT -. HasHit .-> PVG
  CDS -. Health replicated .-> CDC
  WVG --> VFX
  PVG --> VFX
  INT --> MCS
  CVU --> MCS
```

Notes on ordering. These are the attributes in the source; arrows above are only approximately
linear.

- `FirstPersonPlayerVariableStepControlSystem` is `[UpdateAfter(BuildCharacterPredictedRotationSystem)]`,
  `[UpdateBefore(WeaponPredictionUpdateGroup)]` and `[UpdateBefore(FirstPersonCharacterVariableUpdateSystem)]`.
- `FirstPersonCharacterVariableUpdateSystem` is `[UpdateAfter(PredictedFixedStepSimulationSystemGroup)]`.
  **This means look/rotation for tick N is integrated *after* the KCC physics step of tick N.**
  `FixedStepControl` therefore builds `MoveVector` from the *previous* tick's yaw.
- I found **no explicit constraint** between `WeaponPredictionUpdateGroup` and
  `FirstPersonCharacterVariableUpdateSystem`. Whether a shot uses this tick's or last tick's view
  rotation depends on Unity's default sort order. This is an ambiguity, not a proven bug.
- `CharacterDeathServerSystem` is `[UpdateAfter(ProjectilePredictionUpdateGroup)]` and runs only in
  the server world.
- All visual groups are `[UpdateBefore(TransformSystemGroup)]` in the normal `SimulationSystemGroup`,
  in client worlds only. They run once per rendered frame, never during rollback.

---

## 3. Subsystem deep-dives

### 3.1 Input

**Three hops: device → Player command → Character/Weapon control.**

1. `FirstPersonPlayerInputsSystem` runs in `[GhostInputSystemGroup]`, on clients only, and only for
   `GhostOwnerIsLocal` players. It writes `FirstPersonPlayerCommands`:
   ```csharp
   public struct FirstPersonPlayerCommands : IInputComponentData {
       public float2 MoveInput;
       public float2 LookYawPitchDegrees;   // ACCUMULATED, wraps at ±2000 (not a delta)
       public InputEvent JumpPressed;
       public InputEvent ShootPressed;
       public InputEvent ShootReleased;
       public bool AimHeld;
   }
   ```
   - **Look is an accumulated absolute value, not a delta.** `InputDeltaUtilities.AddInputDelta`
     does `fmod(input + delta, 2000)`. Consumers recover the delta with
     `GetInputDelta(current, previous)`, which handles wrap-around. `previous` is
     `FirstPersonPlayerNetworkInput.LastProcessedLookYawPitchDegrees`, a `[GhostField]` sent
     `OnlyPredictedClients` and therefore restored on rollback. If a tick's command is lost,
     repeated or re-simulated, rotation is never double-applied or dropped, because the delta is
     always "total now minus total last processed".
   - Sensitivity is applied **here**, before accumulation. Gamepad stick values are multiplied by
     `deltaTime` so they become deltas.
   - One-shot buttons use N4E **`InputEvent`**. Per the docs, it guarantees "one-off events … are
     synchronized properly with the server and registered exactly once, even when the exact input
     tick where the input event was first registered is dropped"
     ([command stream docs](https://docs.unity3d.com/Packages/com.unity.netcode@1.4/manual/command-stream.html)).
     Internally it is a counter, not a bool (my understanding of the implementation; the docs
     describe the guarantee only).
   - N4E sends each command with the previous three for redundancy, delta-compressed (same page).
2. `IInputComponentData` code generation turns the struct into a per-tick command buffer. During
   prediction, the component on the Player holds **that tick's** input, on both server and client.
3. **Routing to the body and the weapon** (both systems run on client and server inside prediction):
   - `FirstPersonPlayerFixedStepControlSystem` (in `PredictedFixedStepSimulationSystemGroup`,
     `OrderFirst`) turns `MoveInput` into a world-space `FirstPersonCharacterControl.MoveVector`,
     using the character's current rotation, and copies `Jump = JumpPressed.IsSet`.
   - `FirstPersonPlayerVariableStepControlSystem` writes `FirstPersonCharacterControl.LookYawPitchDegreesDelta`.
     It also copies `ShootPressed`/`ShootReleased`/`AimHeld` plus
     **`CommandDataInterpolationDelay.Delay` → `WeaponControl.InterpolationDelay`** on the
     character's `ActiveWeapon.Entity`. The lag-compensation input therefore travels with the
     weapon intent.
   - The comment in the tutorial explains why there are two systems: fixed-rate inputs must be read
     in the fixed group "in case your framerate is lower than the fixed step rate".

### 3.2 Movement / character

- **Entities:** the Character ghost carries the KCC components baked by
  `KinematicCharacterUtilities.BakeCharacter`: `KinematicCharacterProperties`,
  `KinematicCharacterBody`, `PhysicsCollider`, and the buffers `KinematicCharacterHit`,
  `StatefulKinematicCharacterHit`, `KinematicCharacterDeferredImpulse` and
  `KinematicVelocityProjectionHit`. On top of these it has `FirstPersonCharacterComponent`,
  `FirstPersonCharacterControl`, `ActiveWeapon`, `OwningPlayer`, `Health`, `DelayedDespawn` (disabled)
  and `CharacterInitialized` (enableable).
- **Processor pattern.** `FirstPersonCharacterProcessor : IKinematicCharacterProcessor<FirstPersonCharacterUpdateContext>`
  is a short-lived struct built per entity inside an `IJobEntity`. It holds
  `KinematicCharacterDataAccess` plus `RefRW`s to the game components. `PhysicsUpdate` is
  *user-composed* from library steps:
  `Update_Initialize → Update_ParentMovement → Update_Grounding → [HandleVelocityControl: game code]
  → Update_PreventGroundingFromFutureSlopeChange → Update_GroundPushing → Update_MovementAndDecollisions
  → Update_MovingPlatformDetection → Update_ParentMomentum → Update_ProcessStatefulCharacterHits`.
  Policy callbacks the library calls back into are `UpdateGroundingUp`, `CanCollideWithHit`,
  `IsGroundedOnHit`, `OnMovementHit`, `OverrideDynamicHitMasses` and `ProjectVelocityOnHits`. Each
  has a `Default_*` implementation. The `…UpdateContext` struct carries `ComponentLookup`s and
  singletons that callbacks need (here `WeaponControl` and `WeaponVisualFeedback`).
  ([CC character update docs](https://docs.unity3d.com/Packages/com.unity.charactercontroller@1.4/manual/concepts-character-update.html))
- **Two update rates.** `PhysicsUpdate` runs at the fixed physics rate and covers velocity,
  collision and grounding. `VariableUpdate` runs every prediction tick, *including client partial
  ticks*, and covers rotation only. The KCC package's
  `AddVariableRateRotationFromFixedRateRotation` exists for platform rotation. It is commented out
  in this sample because rotation is reduced to a Y angle.
- **Velocity control** uses `CharacterControlUtilities.StandardGroundMove_Interpolated`,
  `StandardJump`, `StandardAirMove`, `AccelerateVelocity` (gravity) and `ApplyDragToVelocity`.
  There is a guard, `MovementWouldHitNonGroundedObstruction`, that stops air-climbing slopes.
- **Deferred impulses.** Character-to-dynamic-body pushes are written to a
  `KinematicCharacterDeferredImpulse` buffer and applied after the parallel job, which avoids write
  races on other bodies.
- **Replicated movement state** (`GhostVariants.cs`): a `KinematicCharacterBody_GhostVariant` with
  only `RelativeVelocity` (quantization 1000) and `IsGrounded`, sent `OnlyPredictedClients`.
  `LocalTransform` is set to `DontSerializeVariant` globally, then overridden per prefab with a
  specific variant hash. I did not resolve the hash; it is presumably position-only. Rotation is
  **reconstructed** from `CharacterYDegrees`. `CharacterInterpolation` (render smoothing between
  fixed steps) exists only on `PredictedClient`.

### 3.3 Camera / view

- **View entity.** `FirstPersonCharacterComponent.ViewEntity` is a child entity with
  `FirstPersonCharacterView { CharacterEntity }`.
- **Simulation state.** `CharacterYDegrees` and `ViewPitchDegrees` are `[GhostField(Quantization=1000,
  Smoothing=InterpolateAndExtrapolate)]`, sent to **all** clients, so remote players' pitch is
  visible. `FirstPersonCharacterUtilities.ComputeFinalRotationsFromRotationDelta` integrates the
  yaw/pitch delta, clamps pitch to `[MinViewAngle, MaxViewAngle]`, and outputs the character
  rotation, `canceledPitchDegrees` and `ViewLocalRotation`. **Roll is passed as 0**, with the comment
  "don't include roll angle in simulation".
- **Rebuilding.** `BuildCharacterPredictedRotationSystem` (prediction, `OrderFirst`) rebuilds
  `LocalTransform.Rotation` from `CharacterYDegrees` at the start of each predicted tick, so
  rollback restores rotation correctly. `BuildCharacterInterpolatedRotationSystem` (client,
  non-predicted, `WithNone<GhostOwnerIsLocal>`) does the same for remote characters, and also sets
  the view entity pitch.
- **Presentation-only.** `FirstPersonCharacterPresentationOnlySystem` adds strafe-tilt roll
  (`ViewRollAmount`/`ViewRollSharpness`) to the view entity. `CharacterWeaponVisualFeedbackSystem`
  computes `MainEntityCamera.CurrentFoV` from the aim FoV ratio plus recoil FoV kick.
  `MainCameraSystem` (`PresentationSystemGroup`) copies the view entity's `LocalToWorld` and the FoV
  onto a managed `UnityEngine.Camera`.
- `ClientGameSystem.HandleCharacterSetup` adds `MainEntityCamera` to the local character's view
  entity and switches the local body to shadow-only rendering. Remote characters get a `NameTagProxy`.

### 3.4 Inventory / equipment

There is effectively none. `ServerGameSystem.HandleCharacters` instantiates **one random weapon
prefab** from the `GameResourcesWeapon` buffer and sets `Character.ActiveWeapon.Entity`.
`ActiveWeaponSystem` (prediction, `OrderFirst`) detects `Entity != PreviousEntity`, where
`PreviousEntity` is a non-ghost local field. It then does the following:

- parents the weapon to `WeaponAnimationSocketEntity`;
- points `WeaponShotSimulationOriginOverride.Entity` at the character's `ViewEntity`;
- sets `WeaponOwner`;
- appends the weapon to the character's `LinkedEntityGroup`, so the weapon dies with the body;
- adds the character to the weapon's `WeaponShotIgnoredEntity` buffer, so the owner cannot shoot
  itself.

Un-equipping is a literal `// TODO: Un-setup previous weapon`.

### 3.5 Weapons / firing

`WeaponsSimulationSystem` lives in `WeaponPredictionUpdateGroup`, inside `PredictedSimulationSystemGroup`,
on client and server.

- **`BaseWeaponSimulationJob`** (every predicted tick, parallel):
  - `projectileEvents.Clear()` is called first. **The event buffer is a per-tick scratch queue on
    the weapon entity.**
  - `ShootPressed` sets `IsFiring`. `ShotTimer += dt`, clamped to `delayBetweenShots + dt`. The
    comment explains that the clamp limits shots per tick and "prevents needlessly dirtying the
    timer ghostfield (saves bandwidth)".
  - `while (IsFiring && ShotTimer > delay) { TotalShotsCount++; ShotTimer -= delay; if (!Automatic) IsFiring = false; }`
    allows several shots per tick.
  - `ShootReleased` or non-auto clears `IsFiring`.
  - For each new shot and each pellet (`ProjectilesPerShot`): `TotalProjectilesCount++`,
    `Random.CreateFromIndex(TotalProjectilesCount)` for spread, then a `WeaponProjectileEvent { ID,
    SimulationPosition, SimulationDirection, VisualPosition }` is appended.
    **Spread is seeded only by the weapon's cumulative projectile counter.**
  - The simulation origin is the view entity (eye); the visual origin is the weapon's `ShotOrigin`
    (muzzle). World transforms are computed by **walking `Parent`/`LocalTransform` by hand**
    (`TransformHelpers.ComputeWorldTransformMatrix`), because `LocalToWorld` is stale inside the
    prediction loop.
- **Resolvers are selected by marker component**, as separate jobs over the same buffer:
  - `RaycastWeaponSimulationJob` (`RaycastWeapon`) runs only when
    `networkTime.IsFirstTimeFullyPredictingTick`.
  - `PrefabWeaponSimulationJob` (`PrefabWeapon`) has the same guard. It `ECB.Instantiate`s a
    projectile ghost with a `ProjectileSpawnId { WeaponEntity, SpawnId = event.ID }` and a
    `VisualOffset = VisualPosition − SimulationPosition`. The visual offset lerps to zero over
    `VisualOffsetCorrectionDuration` in `PrefabProjectileVisualsSystem`, which writes `LocalToWorld`
    directly. Bullets visually leave the muzzle but converge onto the eye ray.
- **Weapon data is split across components:**
  - `BaseWeapon` holds config (`Automatic`, `FiringRate`, `SpreadRadians`, `ProjectilesPerShot`,
    `ShotOrigin`), ghost state (`ShotTimer`, `IsFiring`, `TotalShotsCount`, `TotalProjectilesCount`)
    and local-only presentation cursors (`LastVisualTotal*Count` + `*Initialized`).
  - `RaycastProjectile { Range, Damage }` lives **on the projectile prefab** referenced by the
    weapon. The weapon is a launcher; the projectile prefab carries damage.

### 3.6 Hit detection & lag compensation

- **Hitscan:**
  - `PhysicsWorldHistory.GetCollisionWorldFromTick(ServerTick, weaponControl.InterpolationDelay,
    ref PhysicsWorld, out CollisionWorld)` returns the collision world the shooter saw.
  - `CastRay` collects **all** hits. `GetClosestValidWeaponRaycastHit` keeps the closest hit whose
    material `IsCollidable` and whose entity is not in `WeaponShotIgnoredEntity`.
  - The filter is `CollisionFilter.Default // Todo; customizable`.
  - Hits are taken against the movement capsule itself. There are no hitboxes, zones or headshots.
- **Lag compensation** is enabled through `NetCodePhysicsConfig.EnableLagCompensation`, which
  provides the `PhysicsWorldHistorySingleton`. The interpolation delay is in ticks and "internally
  clamped to the current collision history size"
  ([N4E physics](https://docs.unity3d.com/Packages/com.unity.netcode@1.7/manual/physics.html),
  [API](https://docs.unity3d.com/Packages/com.unity.netcode@1.0/api/Unity.NetCode.PhysicsWorldHistorySingleton.html)).
  N4E 1.3 fixed an off-by-one: history is now cloned after `BuildPhysicsWorld` for the tick
  ([changelog 1.3](https://docs.unity3d.com/Packages/com.unity.netcode@1.3/changelog/CHANGELOG.html)).
  The whole world is rewound, not just hurtboxes.
- **Projectiles are *not* lag-compensated.** `ProjectileSimulationsJob` casts a ray along the
  per-tick displacement against the *current* `PhysicsWorld`. That cast also runs only on
  `IsFirstTimeFullyPredictingTick`, while position integration runs on every (re)simulated tick.
  `HasHit`/`Velocity`/`LifetimeCounter` are ghost fields, so the server corrects them.

### 3.7 Damage / health

- `Health { MaxHealth; [GhostField(Quantization=100)] CurrentHealth; IsDead() => CurrentHealth <= 0 }`.
- Damage is **`HealthLookup[hit].CurrentHealth -= dmg` directly inside the resolver jobs**, guarded
  by `IsServer`. This happens in `RaycastWeaponSimulationJob`, `ProjectileBulletSimulationJob` and
  `RocketSimulationJob`.
  - The jobs are scheduled with `.Schedule` (single-threaded), which avoids write races.
  - There is no damage event, no instigator, no armor and no friendly fire.
  - Clients **never predict health**; they see the replicated value.
- **Rocket AoE.** Direct-hit damage is applied, then `OverlapSphere(DamageRadius)` applies
  `MaxRadiusDamage * (1 − dist/radius)` to every `Health` in range. There is **no occlusion test**.
  The directly hit entity also takes splash damage, so it is hit twice.

### 3.8 Death, despawn, respawn, spawns

- `CharacterDeathServerSystem` (server, prediction group, after projectiles) polls
  `Health.IsDead()` on characters that do not have `DelayedDespawn` enabled. It then:
  1. sends `RespawnMessageRequest { Start = true, CountdownTime }` as an RPC to the owner's
     connection (looked up via `ServerGameSystem.Singleton.ConnectionEntityMap`);
  2. creates a **request entity** `CharacterSpawnRequest { ForConnection, Delay = RespawnTime }`;
  3. enables `DelayedDespawn`.
- `DelayedDespawn : IComponentData, IEnableableComponent` has `[GhostEnabledBit]` and
  `{ uint Ticks; byte HasHandledPreDespawn; }`. On the first frame it is enabled, it disables
  collision response (`CollisionResponsePolicy.None`) and, on clients, hides rendering. The server
  destroys the entity after `GameResources.DespawnTicks`. **The corpse lingers for N ticks, so every
  client receives the final state before the ghost disappears.**
- `CharacterDeathClientSystem` (client + server, non-predicted) spawns death VFX once, guarded by
  the local `HasProcessedDeath` byte.
- `ServerGameSystem.HandleCharacters` (non-predicted `SimulationSystemGroup`, server):
  - counts down `CharacterSpawnRequest.Delay -= DeltaTime`;
  - then picks a random `SpawnPoint` (empty tag) `LocalToWorld` with ±0.1 jitter, from a server
    `Random` **seeded from `DateTime.Now.Millisecond`**;
  - instantiates `CharacterGhost`, sets `GhostOwner`, `OwningPlayer`, and
    `Player.ControlledCharacter`;
  - instantiates a weapon and sets `ActiveWeapon`;
  - sends `RespawnMessageRequest { Start = false }`.
  It also runs `PhysicsCollider.MakeUnique` on new characters.

### 3.9 Game management / singletons

- `GameResources` is a baked singleton holding `ClientServerTickRate`, `JoinTimeout`,
  `DespawnTicks`, `PolledEventsTicks`, `RespawnTime`, `PlayerGhost`, `CharacterGhost`,
  `SpectatorPrefab` and `ForceOnlyFirstWeapon`. It comes with a `GameResourcesWeapon` buffer of
  weapon prefabs. It is loaded into every world of a session from a dedicated subscene.
- Per-system singletons are defined as nested structs:
  - `ServerGameSystem.Singleton` (listen endpoint, RNG, `AcceptJoins`, `ConnectionEntityMap`);
  - `ClientGameSystem.Singleton`;
  - `GameWorldSystem.Singleton` (player name, `SwitchBool CrosshairActive`,
    `SwitchBool RespawnScreenActive`, `RespawnScreenTimer`). This is the **ECS → managed UI read
    model**, and it is pushed to `GameManager` only for the "main" client world.
- Connection lifecycle uses tag components on the connection entity: `PendingClient` (with join
  timeout), then `JoinedClient { PlayerEntity }`, then `NetworkStreamInGame`. The Player ghost is
  added to the connection's `LinkedEntityGroup`, so a disconnect destroys it.

### 3.10 UI / feedback

- Visual recoil, bob, aim FoV and FoV kick live in `CharacterWeaponVisualFeedback` (on the
  character) and are driven by `WeaponVisualFeedback` (tuning on the weapon). This is
  **presentation-only**: there is no aim-affecting recoil anywhere in the sample.
- Cosmetic shot effects come from cumulative counters:
  - `InitializeWeaponLastShotVisualsSystem` (`OrderFirst`) snaps `LastVisualTotalShotsCount =
    TotalShotsCount` **the first time a ghost is seen**, so "false visual feedbacks when a ghost is
    re-spawned due to relevancy" do not appear;
  - visual systems loop `for i in LastVisual..Total`;
  - `Finalize…` (`OrderLast`) advances the cursor.
- **Two shot-visual sync modes** on `RaycastWeapon.VisualsSyncMode`:
  - `Precise`: the server and owner append `RaycastWeaponVisualProjectileEvent { Tick, DidHit,
    EndPoint, HitNormal }` to a ghost buffer with `[GhostComponent(OwnerSendType =
    SendToNonOwner)]`, pruned once older than `PolledEventsTicks`. Remote clients replay events
    whose `Tick > LastProcessedProjectileVisualEventTick`.
  - `BandwidthEfficient`: no buffer. Remote clients reconstruct each projectile from the
    `TotalProjectilesCount` delta, the counter-seeded spread and the *latest interpolated* weapon
    transform, then raycast locally. The cost is a single `uint`; precision is lost.
- VFX are **request queues**. `VFXManager<T>.AddRequest(...)` (NativeArray + count) lives in
  `VFXSparksSingleton`/`VFXExplosionsSingleton`. `VFXSystem` (`OrderLast`) uploads the queue to a
  `GraphicsBuffer` and fires one VFX Graph event per batch. One-shot guards are local bytes on the
  emitting entity (`Rocket.HasProcessedHitVFX`, `ProjectileBullet.HasProcessedHitVFX`).

### 3.11 Networking (ghost configuration summary)

| Ghost / component | Mode / send | What is replicated |
|---|---|---|
| Player | OwnerPredicted (`DefaultGhostMode: 2` in prefab YAML; I believe 2 = OwnerPredicted, which the enum order Interpolated/Predicted/OwnerPredicted implies; not verified against the enum source) | `FirstPersonPlayer.Name`, `.ControlledCharacter`; `FirstPersonPlayerNetworkInput.LastProcessedLookYawPitchDegrees` (OnlyPredictedClients) |
| Character | OwnerPredicted, `HasOwner` | `CharacterYDegrees`, `ViewPitchDegrees` (interp+extrap), KCC `RelativeVelocity`/`IsGrounded` (OnlyPredictedClients), `LocalTransform` (variant override), `Health.CurrentHealth`, `ActiveWeapon.Entity`, `OwningPlayer`, `DelayedDespawn` enabled bit |
| Weapon | OwnerPredicted | `BaseWeapon.{ShotTimer, IsFiring, TotalShotsCount, TotalProjectilesCount}`, `RaycastWeaponVisualProjectileEvent` buffer (SendToNonOwner) |
| Rocket/Plasma projectile | OwnerPredicted, predicted-spawned | `PrefabProjectile.{Velocity, LifetimeCounter, HasHit, VisualOffset}`, `ProjectileSpawnId` (SendToOwner) |

Mechanisms worth naming:

- **Partial ticks.** Clients run the prediction group with a fractional `deltaTime` every frame
  and roll back to the last full tick before continuing
  ([intro to prediction](https://docs.unity3d.com/Packages/com.unity.netcode@1.4/manual/intro-to-prediction.html)).
  This is how look feels per-frame responsive.
- **Rollback scope.** Only entities with `PredictedGhost` + enabled `Simulate` re-simulate. Every
  predicted job has `[WithAll(typeof(Simulate))]`. Interpolated ghosts have `Simulate` disabled, so
  remote characters are *not* KCC-simulated on clients.
- **Restore semantics.** On rollback, "only the fields which are actually serialized as part of the
  snapshot are copied back, not the full component, thus preserving any non-GhostField state"
  ([deepwiki summary of N4E](https://deepwiki.com/needle-mirror/com.unity.netcode/6.1-client-side-prediction)).
  The sample relies on this for `LastVisual*`, `PreviousEntity`, `HasProcessed*` and
  `LastProcessedProjectileVisualEventTick`.
- **`IsFirstTimeFullyPredictingTick`** "guards one off operations (like the instantiation of
  predicted ghost spawns, and V/SFX on the client) so that they only happen once"
  ([spawning docs](https://docs.unity3d.com/Packages/com.unity.netcode@1.4/manual/ghost-spawning.html)).
- **Predicted spawn matching.** `ProjectileClassificationSystem` (in `GhostSimulationSystemGroup`,
  client) matches each incoming server ghost of `GhostSpawnBuffer.Type.Predicted` to the local
  `PredictedGhostSpawn` with the same ghost type **and the same `ProjectileSpawnId`**.

---

## 4. Mapping to the Krit plan

| DOTS OnlineFPS concept | Krit crate / type / principle | Verdict |
|---|---|---|
| `FirstPersonPlayer.ControlledCharacter` + `OwningPlayer` | `krit_core::entity` `Controls` ↔ `ControlledBy`, P5 | **Agreement** |
| `FirstPersonPlayerCommands : IInputComponentData` **on the Player** | `CharacterIntent` **on the Character** (§5.3, §6.1) | **Divergence** (see §6.1 below) |
| `LookYawPitchDegrees` accumulated + `LastProcessed…` ghost field | `CharacterIntent.look_delta: Vec2` | **Divergence / gap** |
| `InputEvent` (counted one-shot) | `pressed`/`released: Buttons` edges (§6.1, §7.2) | **Gap**: bool edges are lossy |
| `FirstPersonCharacterControl { MoveVector, LookYawPitchDegreesDelta, Jump }` | `CharacterIntent` | **Agreement** (DOTS has two layers, Krit has one) |
| `WeaponControl` on active weapon, written by player control system | `ItemIntent` on active item, written in `Equipment` (§5.6) | **Agreement** |
| `WeaponControl.InterpolationDelay` | `ShotRequest` has no view-time field | **Gap** |
| `CharacterYDegrees`/`ViewPitchDegrees` on the character; view entity derived | `ViewAngles` on Character, P4; kamera copies | **Agreement** |
| Roll excluded from sim; presentation-only roll system | `krit_kamera` `LeanRoll`, P2 | **Agreement** |
| Look integrated in `VariableUpdate` after physics | `KritSimSystems::Aim` before `Movement` | **Divergence** (Krit's order is better) |
| Look per-frame through partial ticks | Bevy has no partial ticks; look applied at tick | **Gap** (responsiveness) |
| `IKinematicCharacterProcessor` + `KinematicCharacterUtilities.Update_*` steps | `krit_movement` pure `step(state, intent, stats, &impl CollisionQuery, dt)` | **Partial agreement**; DOTS is more granular |
| `KinematicCharacterDeferredImpulse` | `ExternalImpulse` (§5.4) | **Gap** (reverse direction: character → dynamic bodies) |
| `BaseWeapon` fire loop w/ `ShotTimer` + `while` | `FireRate` sub-tick accumulator (§5.7) | **Agreement**; adopt the clamp |
| `DynamicBuffer<WeaponProjectileEvent>` (cleared each tick) | `ShotRequest` `Message` (§6.3) | **Agreement** (P3) |
| `RaycastWeapon` / `PrefabWeapon` marker resolvers | `Hitscan`/`Projectile` markers, `BallisticProfile` (P6) | **Agreement** |
| Shot sim origin = view entity; visual origin = muzzle; `VisualOffset` convergence | `AimRay` from eye; `ShotRequest.muzzle` cosmetic (§5.13) | **Agreement**; adopt convergence |
| `Random.CreateFromIndex(TotalProjectilesCount)` | `SimRng::for_shot(tick, shooter, index)` | **Divergence** (seed inputs) |
| `WeaponShotIgnoredEntity` buffer | not specified | **Gap** |
| `PhysicsWorldHistory.GetCollisionWorldFromTick` | `RewindProvider` (core) → `krit_net` | **Agreement** on the seam; **gap** on projectiles |
| Resolver writes `Health` directly, server-only | `DamageRequest` → pipeline → `DamageApplied`/`Died` (§5.9) | **Divergence** (Krit is better) |
| `Health.IsDead()` polling | `LifeState` + `Died { killer, … }` | **Divergence** (Krit is better) |
| Rocket AoE `OverlapSphere`, no occlusion | `Explosions` with occlusion (§5.8) | **Agreement with Krit's stricter goal** |
| `DelayedDespawn` (enableable, N ticks, disable collision/render) | Lifecycle set; "damage does not despawn" | **Gap**: no linger/pending-despawn contract |
| `CharacterSpawnRequest` request entity with countdown | `SpawnCharacter` `Message`, `RespawnPolicy::Timer` | **Gap**: timers must live in components |
| `SpawnPoint` tag + random pick (DateTime seed) | `SpawnPoint { team, weight }` + `SpawnSelector` | **Agreement** on the seam; Krit must use `SimRng` |
| `ActiveWeapon` + `ActiveWeaponSystem` setup on change | `krit_equipment` `ActiveItem`, `EquippedBy`, `ViewSocket` | **Agreement** (Krit is far richer) |
| `LinkedEntityGroup` weapon ↔ character | inventory on Character vs Player (§12) | **Informs open question** |
| `BaseWeapon.TotalShotsCount` + `LastVisual…` + Initialize/Finalize | `WeaponFired.shot_count_total` (§8) | **Agreement**; **gap**: first-observation init |
| `RaycastWeaponVisualProjectileEvent` (Precise) vs counter (BandwidthEfficient) | `HitResult`/`ImpactEvent` to presentation | **Gap** for `krit_net` replication modes |
| `IsFirstTimeFullyPredictingTick` | not in plan | **Gap** (critical) |
| `Simulate` tag (interpolated remotes not simulated) | not in plan | **Gap** (critical) |
| `WorldSystemFilter` + `IsServer()` branches | not in plan | **Gap**: authority policy |
| `ProjectileSpawnId` + `ProjectileClassificationSystem` | `ShotId` exists | **Gap**: prespawn matching key |
| `VFXManager<T>` request queues + `HasProcessed*` flags | `krit_feedback` `PlayCue` | **Agreement** |
| `GameWorldSystem.Singleton` `SwitchBool` crosshair/respawn screen | `krit_feedback` HUD read models | **Agreement** |
| `GameResources` baked singleton | `MatchRules` + definition assets | **Agreement** |
| Hits on movement capsule, no zones | `Hurtbox` on separate layer (§6.3) | **Divergence** (Krit is better) |
| `LocalToWorld` stale in prediction → manual parent walk | `AimRay::from(&Transform, &ViewAngles, &EyeHeight)` | **Agreement**; validates the plan |

---

## 5. Where it agrees with the plan

1. **P5, Player ≠ Character, with a relationship.** The Player holds `Name` and
   `ControlledCharacter`, survives death, and owns the command stream. The Character is destroyed
   (via `DelayedDespawn`) and re-instantiated by `ServerGameSystem`. The N4E tutorial's
   optimisation page even names the trade-off: merging them saves a ghost, but costs "the
   convenience of being able to easily switch a player's controlled character or have the character
   entity be destroyed without destroying player data". Krit's `Controls` ↔ `ControlledBy` is the
   Bevy-relationship version of `ControlledCharacter`/`OwningPlayer`, and it is better because the
   back-link is automatic.
2. **P1, intent is the only gameplay input.** No predicted system reads devices; only
   `FirstPersonPlayerInputsSystem` in `GhostInputSystemGroup` does. Even the F8 auto-move test bot
   just writes `MoveInput`. This is exactly the bot/replay/network symmetry P1 claims.
3. **P4, aim lives on the character.** `ViewPitchDegrees` and `CharacterYDegrees` are character
   state and replicated. The camera (`MainCameraSystem`) only copies the view entity's
   `LocalToWorld`. Roll is explicitly kept out of simulation. This confirms the plan's §7.4 refactor
   of `krit_kamera::rotate_camera`.
4. **§5.6 intent routing to the active item.** `FirstPersonPlayerVariableStepControlSystem` copies
   `Shoot*`/`Aim` into `WeaponControl` on `ActiveWeapon.Entity`, and the weapon never looks up its
   holder's input. This is `ItemIntent` almost verbatim.
5. **P3/§6.3, shot event buffer between firing and resolution.** `BaseWeaponSimulationJob` only
   produces `WeaponProjectileEvent`s. Separate resolver jobs, picked by marker
   (`RaycastWeapon`/`PrefabWeapon`), consume them. "Weapons decide *that*; ballistics decides *what
   it hits*" is literally the code structure. Weapon and projectile behaviour is split into
   per-type components (`Rocket`, `ProjectileBullet`) whose systems run after the generic
   `ProjectileSimulationSystem`, which is P6.
6. **§5.7 sub-tick fire accumulator.** The `while (ShotTimer > delay)` loop handles more than one
   shot per tick, matching the plan's 1200 RPM @ 60 Hz requirement.
7. **§5.13 shots trace from the eye.** `WeaponShotSimulationOriginOverride` is set to the view
   entity. `ShotOrigin` (muzzle) is used only for visuals and `VisualOffset`.
8. **§8 cumulative counters for cosmetics.** `TotalShotsCount`/`TotalProjectilesCount` are ghost
   fields, and `LastVisual*` cursors are local. This is the pattern §8 cites, and it is verified.
9. **§8 timings from data, not animation.** Fire rate, respawn time and despawn ticks are all data
   in `BaseWeapon`/`GameResources`.
10. **P2 presentation split.** All visual systems are in client-only, non-predicted groups before
    `TransformSystemGroup`. VFX are queued requests flushed `OrderLast`. Simulation never touches
    VFX. The one leak is noted in §7.
11. **§6.2 / AimRay helper.** The sample's need to hand-walk `Parent` chains (`LocalToWorld` is
    stale mid-prediction) is the strongest argument for Krit's choice to compute `AimRay` from
    `Transform + ViewAngles + EyeHeight`. In Bevy, `GlobalTransform` is likewise propagated in
    `PostUpdate` and is stale inside `FixedUpdate`.
12. **§5.14 feedback read models.** `GameWorldSystem.Singleton { CrosshairActive,
    RespawnScreenActive, RespawnScreenTimer }` is a HUD read model consumed by managed UI.
    `krit_feedback` is the same idea, generalised.
13. **§5.16, movement and weapons predicted; health authoritative.** This matches exactly what
    OnlineFPS predicts (character, weapon fire state, projectiles) and what it keeps server-only
    (health writes, death, spawning).

---

## 6. Where it runs counter (and who is right for Bevy + fixed tick + net-ready)

### 6.1 Where the network input lives: Player vs Character

DOTS puts the **tick-indexed command** on the **Player**, then copies it each predicted tick into
`FirstPersonCharacterControl` on `ControlledCharacter` and `WeaponControl` on the active weapon.
Krit has `krit_input` write `CharacterIntent` directly on the Character.

- **DOTS has the better argument for networking.** The command stream (N4E's input buffer; in
  lightyear, the `ActionState`/input buffer entity) must be **stable across respawns**. A new
  Character entity starts with an empty buffer, while the server keeps receiving inputs for the
  connection. In DOTS the Player ghost is created at join and linked to the connection's
  `LinkedEntityGroup`, so the input target never changes.
- **Krit's model is better for bots/AI**, which drive a body directly with no Player.
- **Resolution.** Keep `CharacterIntent` on the Character as the sim contract. Add a core
  `PlayerCommand` (or reuse `CharacterIntent` as the payload) on the **Player**, and a routing system
  in `KritSimSystems::Intent` that copies Player → `Controls` target, as DOTS's control systems do.
  Bots skip the Player hop. `krit_net` replicates only the Player-side command.

### 6.2 Look as delta vs accumulated absolute

Krit sends `look_delta`. DOTS sends a wrapped running total and derives the delta against a
replicated `LastProcessed` value. Under prediction with input repeat (the server re-uses the last
input when one is late) or redundant resends, **a delta gets applied twice; a total cannot.** On
rollback, `LastProcessed` is restored with the rest of the predicted state, so re-simulation
reproduces the same deltas. **DOTS is right.** Its cost is one wrap-around helper and one extra
replicated `Vec2` on the Player. Sensitivity and ADS multipliers must be applied *before*
accumulation, on the input side. This also argues against DOTS's own choice of applying
`LookSensitivityMultiplierWhileAiming` inside `VariableUpdate`.

### 6.3 Button edges as bools vs counted events

Krit's `pressed`/`released: Buttons` are per-tick bools, and §7.2 already worries about edges lost
between frames and ticks. N4E's `InputEvent` solves both the frame/tick mismatch and the network
drop with one design, a monotonically increasing count per action. **DOTS is right.** In Bevy, a
`u8` wrapping press counter per button in the Player command, plus `last_seen_count` in predicted
sim state, gives "pressed this tick = count changed". This is robust to multi-tick frames
(§7.2's double-tick bug), to zero-tick frames, and to packet loss.

### 6.4 Order of aim vs movement

DOTS integrates look in `VariableUpdate` **after** the physics step, so movement on tick N uses
tick N−1's yaw. This is an artefact of DOTS's fixed/variable split and partial ticks. Krit's
`Intent → Aim → Movement` makes move direction use the current yaw. **Krit is better** for a
deterministic, single-rate tick; it also matches Source/Quake. Keep it.

### 6.5 Responsiveness: partial ticks vs nothing

On clients, DOTS gets per-frame look by running the predicted group with **partial ticks** every
frame and rolling them back. Bevy's `FixedUpdate` has no partial ticks, and lightyear does not
emulate them as far as I know (unverified). If Krit applies look only at tick rate, a 60 Hz sim on
a 144 Hz display gives stepped, laggy aim.

- Partial ticks cost a re-simulation every frame. Neither approach is free, but the Bevy-native
  answer is better.
- That answer: **`krit_kamera` renders `ViewAngles` plus the *pending, not-yet-ticked* look delta**
  that `krit_input` has accumulated. This is purely presentational, and it is reconciled when the
  tick consumes it. Movement transforms use `Transform` interpolation, which DOTS also does via
  `CharacterInterpolation`.
- The plan should state this explicitly in §4 (frame/tick bridge) and §5.12.

### 6.6 Damage directly in resolvers

DOTS mutates `Health` inside hitscan and projectile jobs. There is no attribution, no armor and no
friendly fire, and every new weapon type re-implements "apply damage". **Krit's
`DamageRequest → pipeline → Died` is clearly better** for the "use it piece by piece" goal.

The DOTS approach does show one real constraint. Damage writers were forced single-threaded
(`.Schedule`, not `.ScheduleParallel`) to avoid races on `Health`. Krit's `Message` queue
naturally serialises writes into `krit_damage`'s `Apply` stage, so that problem disappears.

### 6.7 Hit volumes

DOTS hits the KCC capsule (`CollisionFilter.Default`). Krit's `Hurtbox` on its own layer, with zone
multipliers, is necessary for headshots and limb damage, and it lets lag compensation rewind only
hurtboxes instead of cloning the whole collision world each tick. **Krit is better.** It is more
work, but `PhysicsWorldHistory` cloning the full world is a known memory/CPU cost at scale.

### 6.8 Spawn RNG and timers

DOTS seeds spawn selection from `DateTime.Now.Millisecond` and counts respawn delay with
`DeltaTime` on a request entity, in the non-predicted group. That is acceptable because it is
server-only and authoritative, but it is non-reproducible. Krit's §8 rule ("randomness only from
`SimRng`") is stricter and better for replays and tests. Keep it, and extend `SimRng` with a
non-shot stream such as `SimRng::for_spawn(tick, player)`.

### 6.9 Weapon lifetime tied to body

DOTS appends the weapon to the character's `LinkedEntityGroup`, so the weapon dies with the body,
and gives a fresh random weapon on respawn. This is the "inventory on Character" answer to §12. It
is simple, but it only works because the sample has no inventory. Krit's proposal (Character by
default, `PersistentInventory` opt-in) is compatible. The ECS-to-ECS lesson is that **whichever
entity owns the items must also own their despawn**. In Bevy, model this as a relationship with
`linked_spawn` (the equivalent of `LinkedEntityGroup`) from Character → items, and switch it to
Player → items under `PersistentInventory`.

---

## 7. Ideas worth adopting / gaps in the plan

Ordered by importance.

1. **Simulation-eligibility filter (`krit_core`).** DOTS gates every predicted job with
   `WithAll<Simulate>`, so interpolated remote characters are **not** KCC/weapon-simulated on
   clients. Krit's domain systems as written would simulate remote characters from stale intents.
   - Add `krit_core::SimulationDisabled` (marker) or an enableable-style gate.
   - Every sim query adds `Without<SimulationDisabled>`, and `krit_net` inserts it on
     interpolated/remote entities.
   - Write this in §8 as a rule: "all sim queries respect the core sim filter".
2. **Rollback-aware side effects (`krit_core`, §8).** DOTS performs hit detection, predicted
   spawns and VFX only when `IsFirstTimeFullyPredictingTick`.
   - Add `krit_core::SimPass { first_time: bool }` (resource, default `true`). `krit_net` sets it
     to false during re-simulation.
   - Rules: spawning (projectiles), `ImpactEvent`/`HitResult` emission to presentation, and
     `EntityEvent` triggers for cosmetics must check it.
   - Message stages that are themselves predicted state, such as `ShotRequest` → hitscan, need a
     decision: re-resolve on rollback (correct) or skip (DOTS, cheaper). Note this in §12.
3. **Authority policy (`krit_core`).** DOTS uses `WorldSystemFilter` and `IsServer()` to keep
   damage writes, death and spawning server-only.
   - Add a `SimAuthority` resource (`Standalone | Server | Client`) and run conditions
     `has_authority()`/`is_client_predicting()`.
   - `krit_damage` (`Apply`/`Resolve`), `krit_match` (spawns, score) and `krit_inventory`
     (grants) run only with authority. Standalone means everything runs.
   - Without this, `krit_net` must reach into domain crates, which violates §5.16's boundary.
4. **Network input on the Player, with an accumulated look and counted presses (`krit_core`
   intent, `krit_input`).** See §6.1–6.3.
   - Concretely, `CharacterIntent.look_delta` becomes the *derived* per-tick value.
   - The replicated command carries `look_total: Vec2` (wrapped) and `press_counts: [u8; N]`.
   - Add `LastProcessedLook` and `LastSeenPress` as predicted state on the Player or Character.
5. **`ShotRequest.view_delay` / `view_tick` (`krit_core::combat`).** DOTS copies
   `CommandDataInterpolationDelay` into `WeaponControl.InterpolationDelay`, and the resolver passes
   it to `GetCollisionWorldFromTick`. `RewindProvider` needs this per shot. Add
   `pub view_tick: Option<u32>` (`None` = no rewind) to `ShotRequest`, filled from a core
   `InterpolationDelay` component that `krit_net` maintains on the Player/Character.
6. **Shot ignore list (`krit_core::combat`, `krit_ballistics`).** DOTS keeps
   `WeaponShotIgnoredEntity` on weapons and copies it into projectiles. Krit should filter hits
   whose `Hurtbox.owner == instigator` by default, plus an optional `ignore: SmallVec<[Entity; 2]>`.
   This is needed for self-hits and for projectiles spawned inside the shooter's capsule.
7. **Prespawn key for projectiles (`krit_ballistics`, `krit_net`).** DOTS matches predicted
   projectile ghosts by `ProjectileSpawnId { WeaponEntity, SpawnId }`. Krit already has
   `ShotId (shooter, weapon, shot_index)`. Make it the documented prespawn hash, and put it on the
   projectile entity as a component, so lightyear's pre-spawn matching (exact API to verify) can
   use it.
8. **Spread seed from a cumulative counter (`krit_core::rng`, `krit_weapons`).** DOTS seeds from
   the weapon's `TotalProjectilesCount` alone. A remote client can then rebuild pellet directions
   from one replicated `u32`, without knowing the tick. Keep tick-free seeding as an option:
   `SimRng::for_projectile(stable_weapon_id, projectile_index)`.
   - Gap: entity ids differ between client and server, so a **network-stable weapon id** is needed
     (a `NetId`-like component that `krit_net` provides, falling back to `Entity` bits offline).
   - Also track `total_projectiles` separately from `total_shots`, as DOTS does, because pellets
     multiply.
9. **First-observation init for cosmetic cursors (`krit_feedback`, `krit_viewmodel`).** DOTS's
   `Initialize…LastVisual…` system prevents a burst of fake muzzle flashes when a ghost becomes
   relevant again or is first seen. Add to §8: "presentation cursors initialise to the current
   counter on first sight (`Added<…>`)".
10. **Two replication modes for shot cosmetics (`krit_net` + `krit_feedback`).** DOTS has
    `Precise` (a bounded, tick-stamped event buffer, `SendToNonOwner`, pruned by age) and
    `BandwidthEfficient` (counter + deterministic reconstruction). Make this a per-weapon setting in
    `krit_net`, with `HitResult`/`ImpactEvent` as the "precise" payload.
11. **Pending despawn / linger (`krit_match` lifecycle, `krit_damage`).** DOTS's `DelayedDespawn`
    keeps the corpse for `DespawnTicks`, with collision off and rendering hidden, so all clients see
    `Health ≤ 0` and play death effects before the ghost disappears. Add a core `PendingDespawn
    { ticks_left }` that `Lifecycle` honours, and a `LifeState::Dead` gate. Without it, a networked
    despawn on the death tick races the final snapshot.
12. **Respawn timers as components, not messages (`krit_match`).** Bevy `Message`s live about two
    frames, while DOTS's `CharacterSpawnRequest` entity persists with a `Delay`. `RespawnPolicy::Timer`
    should store `RespawnAt { tick }` on the **Player** (P5), and `Rules` emits `SpawnCharacter`
    when the tick is reached. Use ticks, not seconds (§8).
13. **Projectile visual convergence (`krit_viewmodel` or `krit_feedback`).** Adopt
    `PrefabProjectile.VisualOffset` + `VisualOffsetCorrectionDuration`: the projectile renders from
    the muzzle and lerps onto the true eye-ray position. This belongs in presentation only, applied
    to the render transform. Krit's `ShotRequest.muzzle` already carries the data.
14. **Movement processor hooks (`krit_movement`).** The DOTS split is library steps
    (`Update_Grounding`, `Update_MovementAndDecollisions`, …) plus callbacks (`CanCollideWithHit`,
    `IsGroundedOnHit`, `OnMovementHit`, `ProjectVelocityOnHits`, `OverrideDynamicHitMasses`) plus a
    user-written velocity-control slot. That split is how one KCC serves the FPS, the platformer and
    the tutorial characters. Expose the same three tiers:
    - `pub fn` steps;
    - a `CharacterProcessor` trait with `Default_*`-style provided methods;
    - a velocity hook per `MovementState`.

    This supersedes a monolithic `step()`. It also gives a clean place for `MovementProfile`
    presets, and for the bevy_ahoy adapter question in §12.
15. **Character → dynamic body impulses (`krit_movement`, `krit_avian`).** DOTS defers these via
    `KinematicCharacterDeferredImpulse`. Add an outbound `CharacterPushImpulse` message alongside
    the inbound `ExternalImpulse`, applied by the backend after the KCC step.
16. **Minimal replicated KCC state (`krit_net`, §5.16).** DOTS replicates only `RelativeVelocity` +
    `IsGrounded` (+ parent fields if moving platforms are used), with position separate and
    rotation rebuilt from a yaw float. Krit's `KinematicState` should mark which fields are state
    (velocity, grounded, ground entity, coyote timers) and which are derived (ground normal, which
    can be recomputed). Also, **replicate `ViewAngles` to all clients** (interpolated) for
    third-person aim pitch, not just to the owner.
17. **Rotation reconstruction at tick start (`krit_movement`).** DOTS's
    `BuildCharacterPredictedRotationSystem` (`OrderFirst`) rebuilds `Transform.rotation` from
    `CharacterYDegrees` each tick, so rollback restores rotation from one float. In Krit, make
    `ViewAngles.yaw` the source of truth, and have `KritSimSystems::Aim` (or a first sub-set) write
    `Transform.rotation` from it. Never let the reverse direction happen.
18. **ECS → UI change flags (`krit_feedback`).** DOTS's `SwitchBool` pairs a value with a
    "changed" bit that the UI consumes. In Bevy, `Res<T>::is_changed()` plus read-model resources
    already do this. No new type is needed, but the design is validated.

**Input for §12 Open questions:**

- *Inventory on Player or Character?* DOTS chose Character with linked lifetime (§6.9). That
  supports Krit's proposed default.
- *New open question:* are hit resolution and projectile collision re-run on rollback, or only on
  the first prediction (DOTS)? Are projectiles lag-compensated (DOTS: no)?
- *New open question:* does Krit need per-frame look presentation (§6.5)? Recommendation: yes, in
  `krit_kamera`.
- *New open question:* is Player ↔ Character merge allowed as an optimisation? DOTS documents it as
  one. Krit's `Controls` relationship to self would make it trivial; confirm queries tolerate
  `Controls(self)`.

---

## 8. Pitfalls / lessons

1. **Non-replicated fields are not rolled back.** N4E restores only serialized fields. DOTS
   deliberately keeps presentation cursors (`LastVisual*`, `HasProcessed*`) and setup-detection
   state (`ActiveWeapon.PreviousEntity`) in non-ghost fields. In Bevy/lightyear, the analogue is
   which components are registered for rollback. Krit should *separate* presentation cursors into
   their own components (never registered for rollback), instead of mixing them into sim
   components as `BaseWeapon` does. DOTS's mixing works only because of N4E's field-level restore.
2. **One-shots inside prediction need a "first time" guard**, or rollback spawns duplicate
   projectiles and VFX (see §7.2).
3. **Transforms are stale inside the fixed/predicted loop.** DOTS hand-walks `Parent` chains to get
   the eye position. In Bevy, `GlobalTransform` is propagated in `PostUpdate`, so never read it in
   sim. Use `AimRay::from(Transform, ViewAngles, EyeHeight)` on the root.
4. **Relevancy respawns replay counters.** Initialise visual cursors on first observation.
5. **Ordering holes show up as one-tick lag.** OnlineFPS leaves weapon sim vs aim integration
   implicitly ordered. Krit's single chained `KritSimSystems` avoids this, but only if sub-crates
   never add sim systems outside a set. Enforce this in review and in a debug ambiguity check.
6. **The bandwidth comments are real design drivers.** Examples are clamping `ShotTimer` so it
   stops dirtying, replicating yaw as a quantized float instead of a quaternion, `OnlyPredictedClients`
   for velocity, and `SendToNonOwner` for visual events. Krit components should keep "state" fields
   small and quantizable (angles as `f32`, not `Quat`), so `krit_net` can do the same.
7. **AoE without occlusion, plus direct hit on top of splash**, is a classic rocket double-damage
   and through-wall bug. Krit's explicit occlusion and pellet/shot grouping (`group: Option<ShotId>`)
   should also cover "direct + splash" de-duplication policy.
8. **Sim reading visual tuning.** DOTS's character `VariableUpdate` reads
   `WeaponVisualFeedback.LookSensitivityMultiplierWhileAiming`, a presentation component, so the
   server needs visual data. Put ADS sensitivity in `krit_input` (pre-accumulation) instead.
9. **Sample omissions are not design endorsements.** OnlineFPS has no weapon switching ("TODO"),
   reload, ammo, teams, score, hitboxes or kill attribution. Do not treat its minimalism as evidence
   that those seams are unnecessary.
10. **N4E needs at least one predicted ghost for physics to run on clients**, and physics moves into
    the prediction loop, re-running about 22 times per frame at 300 ms RTT
    ([N4E physics](https://docs.unity3d.com/Packages/com.unity.netcode@1.7/manual/physics.html),
    [prediction](https://docs.unity3d.com/Packages/com.unity.netcode@1.4/manual/prediction-n4e.html)).
    Rollback cost scales with everything in the predicted set. Keep Krit's predicted set (movement +
    weapons) minimal, as §5.16 already says, and keep KCC queries cheap.

---

## 9. Sources

Primary (source code, read locally from a clone):
- <https://github.com/Unity-Technologies/CharacterControllerSamples>, in particular:
  - `OnlineFPS/Assets/Scripts/Player/{FirstPersonPlayer.cs, FirstPersonPlayerSystems.cs}`
  - `OnlineFPS/Assets/Scripts/Character/{FirstPersonCharacterComponent.cs, FirstPersonCharacterProcessor.cs, FirstPersonCharacterSystems.cs, FirstPersonCharacterUtilities.cs}`
  - `OnlineFPS/Assets/Scripts/Weapons/{Weapons.cs, Projectiles.cs, ActiveWeaponSystem.cs, WeaponSystems.cs, WeaponUtilities.cs, ProjectileSystems.cs, CharacterWeaponVisualFeedbackSystem.cs}`, `Weapons/Specific/{Rocket*.cs, ProjectileBullet*.cs}`
  - `OnlineFPS/Assets/Scripts/Misc/{Health.cs, CharacterDeathSystem.cs, DelayedDespawn*.cs, SpawnPoint.cs, InputDeltaUtilities.cs, GhostVariants.cs, VFXSystem.cs}`
  - `OnlineFPS/Assets/Scripts/GameManagement/{GameResources.cs, ServerGameSystem.cs, ClientGameSystem.cs, GameWorldSystem.cs}`
  - `OnlineFPS/Assets/Data/Netcode/NetCodeConfig.asset`, `OnlineFPS/Assets/Prefabs/Ghosts/*.prefab`, `OnlineFPS/Packages/manifest.json`
  - `_Documentation/Samples/OnlineFPSSample/{game-management,character-and-camera,weapons}.md`
  - `_Documentation/Tutorial/tutorial-netcodecharacters*.md` (prediction, input util, optimisation)

Official docs:
- N4E physics & lag compensation: <https://docs.unity3d.com/Packages/com.unity.netcode@1.7/manual/physics.html>
- `PhysicsWorldHistorySingleton` API: <https://docs.unity3d.com/Packages/com.unity.netcode@1.0/api/Unity.NetCode.PhysicsWorldHistorySingleton.html>
- N4E 1.3 changelog (history off-by-one fix): <https://docs.unity3d.com/Packages/com.unity.netcode@1.3/changelog/CHANGELOG.html>
- Intro to prediction (partial ticks, `IsFirstTimeFullyPredictingTick`): <https://docs.unity3d.com/Packages/com.unity.netcode@1.4/manual/intro-to-prediction.html>
- Prediction in N4E: <https://docs.unity3d.com/Packages/com.unity.netcode@1.4/manual/prediction-n4e.html>
- Command stream / `IInputComponentData` / `InputEvent`: <https://docs.unity3d.com/Packages/com.unity.netcode@1.4/manual/command-stream.html>
- Ghost spawning (predicted spawns): <https://docs.unity3d.com/Packages/com.unity.netcode@1.4/manual/ghost-spawning.html>
- Character Controller update concepts: <https://docs.unity3d.com/Packages/com.unity.charactercontroller@1.4/manual/concepts-character-update.html>
- Character Controller 1.4 changelog (aspect removal): <https://docs.unity3d.com/Packages/com.unity.charactercontroller@1.4//changelog/CHANGELOG.html>
- Competitive Action Multiplayer template: <https://docs.unity3d.com/Packages/com.unity.template.multiplayer-netcode-for-entities@1.0/manual/index.html>
- Multiplayer FPS template: <https://docs.unity3d.com/Packages/com.unity.template.multiplayer-fps@1.0/manual/index.html>

Secondary:
- Aspects-less CC 1.4.1 thread: <https://discussions.unity.com/t/new-aspects-less-approach-in-com-unity-charactercontroller-1-4-1/1693777>
- Template release thread: <https://discussions.unity.com/t/competitive-action-multiplayer-template-now-released-netcode-for-entities/1573137>
- N4E prediction internals (deepwiki, non-official): <https://deepwiki.com/needle-mirror/com.unity.netcode/6.1-client-side-prediction>
- Megacity multiplayer blog: <https://unity.com/blog/games/megacity-competitive-action-multiplayer-sample-game>
