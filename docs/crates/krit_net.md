# 5.16 `krit_net` (later): networking integration

> Part of the [crate architecture plan](../architecture.md) (§5). Section references (§) point to
> that document unless stated otherwise.

**Owns:**
- registration of every `#[reflect(SimState)]` component by `SimStateClass` (§3.1); nothing is listed
  by hand;
- setting `SimAuthority` and `SimReplay`, and assigning `SimId`s on the authority;
- the input-replication plugin: lightyear's native inputs with `ActionState<CharacterIntent>` (not
  its BEI integration, which would put `krit_input` on the server). The command stream lives on the
  **Player** so it survives respawns, and is routed to the controlled Character each tick (DOTS
  `FirstPersonPlayerCommands`). A missing tick repeats the last command, which is safe because the
  intent is absolute (P1);
- a simulation filter so remote characters are not stepped from stale intents (DOTS `Simulate`);
- prespawn keys for predicted projectiles derived from `ShotId`;
- the lag-compensation history ring buffer of `Hurtbox` poses, implementing `RewindProvider`, with a
  `LagCompPolicy`:
  - rewind to `ShotRequest.view_time` (tick + interpolation fraction);
  - clamp to `max_rewind` (default about 220 ms, as Overwatch; Source allows up to 1 s);
  - never rewind across a death or a teleport;
  - a swept-bounds broadphase before rewinding, so only plausible targets are rewound;
  - return historical hurtbox shapes rather than moving entities and restoring them;
  - optional validation of client-reported hits (direction re-derived from `SimRng::for_shot`, a
    bounded leeway) for co-op games that choose `HitAuthority::ClientWithValidation`;
- per-client priority and relevance hooks, and a bandwidth/rollback profiler.

**Prediction scope:** movement, aim, weapons, ammo and projectiles are predicted for the owner.
Damage, death, spawning and scoring are never predicted (P12).

**Latency placement:** for each action the plugin documents where latency goes (shooter, target or
nobody), following Halo: Reach's "lag can only be placed": hits are favour-the-shooter within
`max_rewind`; damage and death are seen one round trip late by the shooter; movement is predicted.

**Boundary:** domain crates are unaware of networking. They only follow the rules in §8 and tag their
state with `SimState`.
