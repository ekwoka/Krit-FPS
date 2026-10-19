# 5.13 `krit_viewmodel`: arms and weapon rendering (presentation, render deps)

> Part of the [crate architecture plan](../architecture.md) (§5). Section references (§) point to
> that document unless stated otherwise.

**Owns:**
- the view-model camera (separate `RenderLayers`, its own FOV, clears depth only, following Bevy's
  `first_person_view_model` example), with scene lights mirrored onto the view-model layer as Foxtrot
  does;
- `ViewModel` for each item entity, with its mesh/scene handle;
- sway, bob and recoil animation driven from sim facts;
- `MuzzleSocket`, for tracers;
- third-person ↔ first-person mesh visibility.

**Consumes:** `ActiveItemChanged`, `WeaponFired` (the counter), `ReloadStarted`, `AdsState`,
`ViewAngles` and `CurrentSpread`.

**Boundary:** shots trace from the **eye**, never from the view-model muzzle. The muzzle is used for
tracers only. (The sim-side `MuzzleOffset` in §5.6 is separate data.)

**Known limitation:** the two-camera approach loses shadows cast on and by the view model. Unreal
5.5+ instead renders first-person primitives with their own FOV from a single camera. Whether Bevy can
do the same is an open question (§12).
