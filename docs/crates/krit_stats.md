# 5.2 `krit_stats`: modifiers

> Part of the [crate architecture plan](../architecture.md) (§5). Section references (§) point to
> that document unless stated otherwise.

**Owns:**
- `StatId`: an interned id or label, extensible;
- `Modifier { stat, op: Add | Mul | Override, value, priority, condition: Option<ConditionId> }`,
  stored as `Modifiers` on the source entity (an attachment, perk, equipped item or status effect);
- `ModifierSourceOf(target)` relationship;
- `StackPolicy` per `StatId` (`Stack`, `HighestOnly`, `LowestOnly`), so two identical slows do not
  stack unless the stat says so;
- a generic `Stat<T>` recompute pattern. A domain declares `BaseX` + `X` (computed) and registers
  which `StatId`s feed which fields.

**Aggregation formula (normative):**
1. If any `Override` applies, the one with the highest `priority` wins (ties broken by source
   `SimId`), and the rest are ignored.
2. Otherwise `value = (base + Σ Add) × Π Mul`. Multipliers **multiply**: two ×0.5 slows give ×0.25,
   not ×0 as in GAS's additive multipliers.
3. Sources are folded in a stable order: `(priority, source SimId)`. The result is clamped to the
   stat's declared range.

**Resources vs stats:** `Health`, `Shield`, `Stamina` and `Magazine` are state, not stats. Their caps
(`MaxHealth`, `MaxShield`) are stats. Each resource declares a `CapChangePolicy` (`KeepAbsolute`,
`KeepRatio`, `Clamp`) for what happens when its cap changes.

**Player-level modifiers:** a source attached to a Player (a perk, a campaign upgrade) forwards to
whichever Character the Player currently `Controls`, and re-attaches on respawn.

**Consumes:** `Modifiers` changes and relationship add/remove events.

**Produces:** recomputed stat components (`WeaponStats`, `MovementStats`, `HealthStats`) whenever a
source is added, removed or changed. Stats are `SimStateClass::Derived`: never replicated, always
recomputed.

**Why a crate of its own:** movement (weight, ADS slowdown), weapons (attachments) and damage (armor
perks) all need it. Putting it in core would make core opinionated, and putting it in weapons would
make movement depend on weapons.
