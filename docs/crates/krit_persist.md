# 5.17 `krit_persist` (later): save and load

> Part of the [crate architecture plan](../architecture.md) (§5). Section references (§) point to
> that document unless stated otherwise.

**Owns:**
- `PersistentId` assignment for authored level entities;
- per-level state (entities updated in place by `PersistentId`, dynamic entities re-created from
  their definitions, tombstones for authored entities destroyed or picked up) and campaign state
  (Player, `PersistentInventory`, `WorldFlags`) saved separately, as COGITO does;
- a `WorldFlags` resource: free-form designer flags;
- a versioned, data-only save format built from `#[reflect(SimState)]` components marked
  persistable. Saves never contain executable content (COGITO's `.res` saves can).

**Boundary:** like `krit_net`, it depends only on `krit_core` and reads the type registry.
