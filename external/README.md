# External Clones Index

This directory holds cloned, large upstream repositories used for reference, history, and patching.

**We do NOT track their contents in git.**
This single `README.md` serves as a tombstone/index describing what is in here and what commits they are pinned to.

### `bevy/`
A shallow clone of the main Bevy game engine repository at the `v0.18.1` tag. 
*Purpose*: Local reference for internal engine architecture, unrendered `rustdoc`, and rendering pipelines (`bevy_render`/`bevy_text`).
*Status*: Reference only.

### `big_brain/`
A clone of the `big-brain` declarative utility AI crate.
*Purpose*: Initial reference point for layering utility AI atop the Simrard Drive component system. 
*Status*: Legacy reference. The core logic has been fully absorbed, rewritten, and owned inside `simrard-lib-utility-ai`.

### `patches/`
Tracks the manual `git format-patch` files historically used to alter engine behaviors before Simrard moved to stable abstractions.
*Status*: Inactive. Preserved for historical reference.
