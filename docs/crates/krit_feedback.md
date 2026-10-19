# 5.14 `krit_feedback`: hitmarkers, killfeed, HUD data (presentation)

> Part of the [crate architecture plan](../architecture.md) (§5). Section references (§) point to
> that document unless stated otherwise.

**Owns** read-model resources and components:
- `HudHealth`, `HudAmmo`, `HudCrosshair` (spread), `HudInteractionPrompt`, `HudObjective`,
  `HudMatchTimer`, `HudHints` (a hint/message channel);
- `Killfeed` (a ring buffer);
- `DamageIndicators` (direction to `DamageRequest.reported_origin`);
- `HitmarkerQueue` with two tiers: **predicted** markers from the client's own `HitResult`, which are
  confirmed or dropped when the matching `HitConfirmed { shot_id }` arrives (Lyra's flow), and
  **confirmed-only** markers. `HitmarkerPolicy` picks one;
- a `TickEdge` helper that detects changes in tick-stamped state (`LastDamageTaken`,
  `LifeState::Dead { at_tick }`) and ignores stamps older than a cutoff, so re-entering view does not
  replay old effects;
- audio/VFX cues: one-shot cues driven by counters, and stateful cues driven by component presence so
  late joiners see them, with hierarchical cue ids and a 1P/3P perspective flag.

**Consumes:** facts and state from every domain, read-only. HUD state can always be rebuilt from
components (P11); events only make it faster.

**Boundary:**
- It does **not** draw UI. Games bind these read models to `bevy_ui`, egui or a custom HUD.
- An optional `krit_feedback/ui_example` feature adds a minimal debug HUD.
