# Krit FPS

A group of plugins for Bevy to implement the common basics of any FPS style game in an extensible manner.

Plugins can be used independently, or collectively, and focus on API contracts where the plugin units consume events and data and then output events and data, allowing systems to be essentially replaced by the game where needed.

## `pub` by Default

As such, virtually everything is `pub` to facilitate this, so even within a plugin, you can pick and choose systems and replace them without needing to recreate other behavior.

## Requirements

This is designed specifically for use with Avian3d and Bevy Enhanced Input, with no plans for supporting others internally. Due to the goals, you can make your own replacements for those relevant parts.

## Contributing

One requirement of the code is that it never relies on Bevy directly, instead using the most specific `bevy_*` crate directly. This can improve compilation times by users by not requiring them to wait for `bevy` to compile. Similarly, this does not do `use _::*`. Imports are specifically targeted.

### AI Usage

AI is not used for any code, outside of limited cases, mainly:

- Research and broad documentation (like in the `docs` directory)
- Handling more complex Math
- Generating Tests and Examples

This is mainly due to the fact that I, as the maintainer want to understand the code on a deep level.
