# 5.10 `krit_interaction`: use / hold-to-use

> Part of the [crate architecture plan](../architecture.md) (§5). Section references (§) point to
> that document unless stated otherwise.

**Owns:**
- `Interactor` (range, focus query from `AimRay`): a ray first, then a forgiving shape cast, then a
  walk up `ChildOf` to the nearest `Interactable` (Source's use search);
- `Interactable` with one or more `InteractionVerb { button: Interact | InteractAlt, prompt_key,
  mode: Press | Hold { duration } | Toggle | Continuous, max_users }`. Availability, duration and
  prompt text can change with object state;
- `InteractionFocus` (the read model for the prompt UI);
- hold-progress state, with cancel rules (move, out of range, damage, fire, target state changed, for
  example the downed player died), stored in sim, never in a HUD timer.

**Requirements and costs** use an attempt/veto pattern: interaction emits `InteractAttempt`; any crate
may answer with `InteractVeto { reason }` in the same tick (missing key, stat too low). On success,
`InteractCompleted` lets those crates charge costs. This keeps interaction free of inventory and stats
imports.

**Consumes:** `CharacterIntent.buttons` (`INTERACT`, `INTERACT_ALT`), `ViewAngles`/`EyeHeight`,
`SpatialQueryBackend`, `ActionGates` and `InteractVeto`.

**Produces:** `InteractAttempt`, `InteractStarted`, `InteractCompleted` and `InteractCancelled`, as
messages and as `EntityEvent`s on *both* the interactable and the interactor.

**Boundary:** doors, pickups (`PickupRequest`), revives (`ReviveRequest`), carrying physics props and
objective plant/defuse are small adapters in the crates that care about them. Interaction knows about
none of them.
