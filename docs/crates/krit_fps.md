# 5.18 `krit_fps`: facade

> Part of the [crate architecture plan](../architecture.md) (§5). Section references (§) point to
> that document unless stated otherwise.

```toml
[features]
default = ["input", "movement", "kamera", "inventory", "equipment", "weapons",
           "ballistics", "damage", "interaction", "match", "modes-deathmatch", "feedback", "avian"]
viewmodel = ["dep:krit_viewmodel"]
net       = ["dep:krit_net"]
persist   = ["dep:krit_persist"]
ahoy      = ["dep:krit_ahoy"]   # M1 movement adapter
assets    = ["krit_core/assets", …]   # RON loaders for definition assets
serialize = ["krit_core/serialize", …]
modes-ctf = ["krit_modes/ctf"]   # etc.
```

- `KritFpsPlugins` is a `PluginGroup` that adds each enabled crate's group, in order, after
  `KritCorePlugin`.
- Startup work runs in public `KritStartupSystems` sets rather than plugin-priority hooks.
- `krit_fps::prelude::*` re-exports each crate's prelude.
- Crates are also re-exported as modules (`krit_fps::weapons::…`) so that users of the facade never
  add the subcrates manually.
