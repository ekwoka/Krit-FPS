# 5.3 `krit_input`: device → intent bridge

> Part of the [crate architecture plan](../architecture.md) (§5). Section references (§) point to
> that document unless stated otherwise.

**Owns:**
- the BEI `InputAction` types (`Movement`, `Jump`, `Crouch`, `Sprint`, `RotateCamera`, `Fire`,
  `AltFire`/`Aim`, `Reload`, `Interact`, `InteractAlt`, `Confirm`, `Cancel`, `SwitchSlot(n)`,
  `NextItem`, `PrevItem`, `Melee`, `Throw`, `Lean`);
- default input contexts and bindings, including a policy for one gamepad button shared by reload and
  interact;
- `InputSettings` (sensitivity, invert, ADS sensitivity mode, toggle-vs-hold per action);
- the client-side aim accumulator systems. The `LocalAim` type lives in `krit_core` so
  `krit_kamera` can read it without importing this crate. It is not sim state and never replicated,
  and it holds only the player's own look input (§6.2);
- frame-to-tick button latching (§4);
- `InputGlyphs`, a read model naming the current binding for each action, for prompts.

**Consumes:** BEI `Action<T>` component values for continuous axes (read in a system), and BEI
events for discrete actions.

**Produces:** writes `CharacterIntent` on the **Character**, with absolute aim and held buttons. In a
networked game the per-tick command is sent from the Player (P5) and routed to the Character (§5.16).
Toggle/hold is already resolved into "wants crouch" state.

**Plugins:**
- `KritInputActions`: the action types plus contexts;
- `KritInputAccumulation`: frame-rate aim accumulation and button latching, the default;
- `KritInputPerTick`: evaluates BEI in `FixedPreUpdate`, for networked games;
- `KritInputSettings`.

**Customization:** replace the whole crate with your own `CharacterIntent` writer, for example
leafwing, a bot or a replay. No other Krit crate imports `krit_input`.

**Boundary:** this is the *only* crate that depends on `bevy_enhanced_input`. Items push their own
input contexts on equip by listening to `krit_equipment`'s `Equipped` event in an *input-side*
observer, rather than equipment importing BEI. The UE5 First Person template does the same with its
weapon-owned `FireMappingContext`.

**Aim accumulation:**
1. **Every frame** (`PreUpdate`, after BEI): read `Action<RotateCamera>` in a system, scale it by
   `InputSettings`, and add it to `LocalAim`. Yaw wraps. Pitch is clamped against `PITCH_LIMIT` in
   `ViewAngles` space (§6.2). Mouse deltas are already distances, so they are not multiplied by
   `dt`. Gamepad sticks are rates, so their bindings carry a delta-time BEI modifier, and this system
   handles both devices the same way.
2. **Every tick, before the simulation** (`FixedPreUpdate`): write the quantized `LocalAim` into
   `CharacterIntent.aim`. If several ticks run in one frame they share that aim. If no tick runs,
   `LocalAim` keeps accumulating and nothing is lost. Writing in the fixed schedule, rather than
   once per frame, keeps `intent.aim` equal to the aim the latest tick actually used. The camera's
   pending term (§5.12) depends on that.

**Server-forced aim:** `krit_input` does nothing special. The simulation expresses forced facing
(spawn, teleport, scripted look) through `ViewAngleOffset`, and `LocalAim` is never rebased. This is
Q3's `delta_angles`.
