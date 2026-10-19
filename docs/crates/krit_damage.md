# 5.9 `krit_damage`: taking hits

> Part of the [crate architecture plan](../architecture.md) (§5). Section references (§) point to
> that document unless stated otherwise.

**Owns:**
- `Health`, `Armor { absorb, durability }` and `Shield { regen_delay, regen_rate }`, on the
  **Character**;
- `LifeState` (`Alive`, `Downed { bleedout }`, `Dying { until }`, `Dead { at_tick, killer }`).
  `Dying` gives death animations and ragdoll handoff a sim-side window before despawn;
- `Invulnerable { until }` for spawn protection;
- `DamageLedger`, for attribution and assists (`AssistPolicy { window, min_damage }`), and
  `LastHitBy { instigator, at_tick }`, so environmental kills (falling, kill volumes) credit the last
  attacker within a window;
- `LastDamageTaken` and `LastDamageDealt`, tick-stamped read models for hit reactions and indicators
  (P11);
- `DamageResistances` per `DamageTypeId`;
- `SelfDamagePolicy { scale, knockback_scale }` for rocket jumping;
- `HealthStats`, through `krit_stats`.

**Consumes:**
- `DamageRequest` from *any* source (ballistics, fall, zone, kill volume, environment, game code);
- `TeamRelations` + `FriendlyFirePolicy` (a resource in core, set by match);
- `HealRequest`;
- `ReviveRequest`, from interaction.

**Produces:**
- `DamageApplied { request, to_shield, to_armor, to_health, lethal }`;
- `Downed`, `Revived` and `Died { victim, killer, assisters, cause }`;
- `HitConfirmed { shot_id, shooter, victim, zone, amount, flags, lethal }`, sent back to the instigator
  for hitmarkers and damage numbers;
- `ActionGates` blocks while downed, dying or dead.

**Pipeline:** an ordered `DamagePipelineSystems::{Validate, Resist, Knockback, Policy, Absorb, Apply,
Resolve}` chain:
- `Validate` checks invulnerability and life state;
- `Resist` applies type resistances and target-side multipliers (armor on a zone, for example);
- `Knockback` emits the `ExternalImpulse` from `DamageRequest.force`. It runs **before** the
  friendly-fire veto and applies `SelfDamagePolicy.knockback_scale`, which keeps rocket jumps working
  with friendly fire off (Q3 ordering);
- `Policy` applies friendly fire (`FORCE_FRIENDLY_FIRE` bypasses it), `SelfDamagePolicy.scale` and
  match gates;
- `Absorb` handles shield, then armor;
- `Apply` changes health;
- `Resolve` handles death, downed state and attribution.

The whole pipeline runs under `has_authority` (P12). A game inserts its own stage with
`.in_set(DamagePipelineSystems::Resist)`. Tarkov-style per-limb health is a replacement for `Apply`
that reads `HitZone`.

**Boundary:**
- Damage is the **single source of truth for kills**. Killfeed, score, XP and AI all read `Died` or
  `LifeState`.
- It does not despawn characters; that is the job of match/lifecycle.
- Props and destructibles without `krit_match` installed still die: `DeathPolicy` on the entity
  (`Despawn`, `Keep`, `ReplaceWith(scene)`) is handled here.
