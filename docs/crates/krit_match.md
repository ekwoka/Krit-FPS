# 5.11 `krit_match`: match flow

> Part of the [crate architecture plan](../architecture.md) (§5). Section references (§) point to
> that document unless stated otherwise.

**Owns:**
- `MatchPhase` as Bevy `States` with `SubStates`: `WaitingForPlayers`, `Warmup`, `PreRound`,
  `InRound`, `RoundEnd`, `PostMatch`. Systems that join a phase late use phase-active run conditions,
  not only `OnEnter`;
- `MatchExperience`, an asset that bundles a mode's plugins' run conditions, rules, loadouts and
  character archetype, so one build can host several modes (Lyra experiences);
- `CharacterArchetype`, an asset naming the components and definitions a spawned Character gets
  (Lyra `PawnData`);
- `Teams` / roster, assigning `Team` to Players before spawn: least-populated team, ties broken by
  `SimRng`;
- `Scoreboard` (per-Player `Score` components, plus a team score resource);
- `SpawnPoint { team, weight, bots_allowed }` and the `SpawnSelector` trait. The default filters by
  team and occupancy, prefers the farthest half from the player's death point and from enemies, then
  picks with `SimRng`. `SpawnExhaustion` (`Queue`, `Telefrag`, `Overlap`) decides what happens when
  every point is blocked;
- `RespawnPolicy` (`Timer`, `Wave` or `None`); respawn timers are `RespawnAt { tick }` components on
  the Player (P11), not messages;
- `RespawnInputLatch`: a freshly spawned Character ignores buttons held through the spawn until they
  are released (Q3 `PMF_RESPAWNED`);
- `PickupSpawner { definition, respawn_ticks }`, which re-checks overlapping players on respawn;
- `OutOfWorld { kill_height }`, which emits a `DamageRequest` of type `World`;
- `MatchRules` (score limit, time limit, rounds);
- `FriendlyFirePolicy`, written to the core resource.

**Consumes:** `Died`, player join/leave, and objective events from modes.

**Produces:**
- `SpawnCharacter { player, point, archetype, loadout }`, realised in `KritSimSystems::Spawn` at the
  start of the next tick, which then sends `GrantLoadout`;
- `DespawnCharacter`, realised in `Despawn` after `LifeState::Dying` ends; the corpse can be handed
  off to a presentation-only ragdoll;
- `PhaseChanged` and `MatchEnded { result }`;
- `ActionGates` blocks during freeze time and post-match.

All of `Rules`, `Spawn` and `Despawn` run under `has_authority`.

**Boundary:** match never touches health, weapons or movement directly. It changes the world only
through gates, policies and spawn/loadout requests.

**`krit_modes`:** each mode is a feature-gated plugin made of rule systems in `KritSimSystems::Rules`
plus objective components:
- `Deathmatch` / `TeamDeathmatch`;
- `CaptureTheFlag`, using interaction plus carry;
- `Domination`, with capture zones;
- `SearchAndDestroy`;
- `Elimination`;
- `Survival` / `Waves`, for single-player and co-op;
- `Objectives`, a generic objective list (reach, collect, interact, kill target) for single-player
  campaigns.

Single-player campaigns usually use `krit_match` only for spawns, respawn and objectives, or skip it.
