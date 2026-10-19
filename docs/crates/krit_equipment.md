# 5.6 `krit_equipment`: what you are holding

> Part of the [crate architecture plan](../architecture.md) (§5). Section references (§) point to
> that document unless stated otherwise.

**Owns:**
- `EquipmentSlots` on the Character, holding the active slot and the last slot for quick-switch;
- the `EquippedBy(character)` ↔ `Equipment` relationship;
- the `ActiveItem` marker;
- the switch state machine (`Holstering` → `Equipping`, using `equip_time` / `holster_time` from
  stats);
- `HolsterRequest { restore_after }`, for carrying physics props, climbing and scripted sequences;
- attachment entities as `AttachedTo(item)`, which are also `krit_stats` modifier sources;
- the `ViewSocket` entity under the character's eye, which items are parented to, and a sim-side
  `MuzzleOffset` per item used as the projectile launch point (§5.8);
- `ActionRules`, a data table of `(action, blocks, cancels)` rows evaluated each tick, for example
  sprint cancels reload, firing cancels sprint, damage cancels a hold-to-use. This is the cancel half
  that `ActionGates` lacks (Lyra `AbilityTagRelationshipMapping`, FPS Sample `canInterrupt`).

**Consumes:**
- `CharacterIntent` (switch, next, prev, last);
- `ActionGates` (`SWITCH`);
- inventory relationships.

**Produces:**
- **Intent routing.** Each tick, the active item gets an `ItemIntent { primary, secondary, reload,
  melee, … }` with held buttons and edges copied from the holder, as in the DOTS sample. Items never
  look up their holder's intent themselves.
- `Equipped`, `Holstered` and `ActiveItemChanged` facts.
- `ActionGates` contributions: no fire while switching.
- `MovementStats` modifiers from the active item's weight.

**Boundary:**
- Weapons do not depend on equipment. A weapon is just an entity holding `ItemIntent`, `ViewSocket`
  parenting and an `Owner`.
- Equipment works for non-weapon items too (flashlight, medkit, scanner, keycard, pickaxe). This
  matters for horror and immersive-sim games; COGITO equips them all through the same path.
