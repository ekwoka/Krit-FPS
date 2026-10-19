# 5.5 `krit_inventory`: what you carry

> Part of the [crate architecture plan](../architecture.md) (§5). Section references (§) point to
> that document unless stated otherwise.

**Owns:**
- `Inventory` on any entity: by default on the **Character** (lost with the body, dropped by
  policy), on a container, a corpse or a vendor. `PersistentInventory` is an opt-in on the **Player**
  for campaigns and extraction stashes; weapons still live on the body, and persistent pools and
  stashes live on the Player;
- `InventorySlots` layout (`Primary`, `Secondary`, `Melee`, `Lethal`, `Tactical`, `Gadget`, quickslots,
  or a custom/grid layout);
- items as entities linked through the `InInventory(owner)` ↔ `InventoryItems` relationship;
- `ItemDefinition` assets, identified by a stable `ItemDefId` (never by display name), with optional
  "fragment" components (Lyra-style): `Equippable`, `Stackable`, `AmmoType`, `Pickup`, `Consumable`,
  `Key`, `GridSize`, `Charge` (batteries, flashlights), `Currency`;
- definitions are immutable; per-instance state (`Magazine`, `Charge`, durability) lives on the item
  entity;
- `AmmoPool` keyed by ammo type;
- pickup, drop and transfer systems, `DuplicatePickupPolicy` (`Reject`, `ConvertToAmmo`, `Stack`),
  `LootTable` and `LoadoutDefinition` assets.

**Consumes:**
- `PickupRequest`, from interaction or overlap;
- `TransferRequest` between inventories (containers, looting);
- `GrantLoadout { player, loadout }`, from match spawn;
- `DropRequest`, and the `Died` **message** for drop-on-death, gated by `DropOnDeath` (§4.1: not an
  observer).

**Produces:**
- `ItemAdded`, `ItemRemoved`, `AmmoChanged` and `PickupDenied { reason }` (so a refused pickup is not
  consumed);
- the item entity graph that equipment reads.

**Boundary:** it knows nothing about equipping or firing. An item is just an entity with a definition.
