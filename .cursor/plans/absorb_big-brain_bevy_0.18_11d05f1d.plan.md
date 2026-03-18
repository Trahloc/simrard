---
name: Absorb big-brain Bevy 0.18
overview: Absorb big-brain's latest code (GitHub main, Bevy 0.17) into a new lib/utility_ai crate, upgrade the whole workspace to Bevy 0.18, and fix compilation/runtime errors until the app runs.
todos: []
isProject: false
---

# Absorb big-brain and move to Bevy 0.18

## Scope

- **Source**: big-brain from GitHub main (v0.23, Bevy 0.17) — one version jump to 0.18.
- **Destination**: New workspace crate `lib/utility_ai` (and `lib/utility_ai/derive` for proc-macros). Apache-2.0 NOTICE in absorbed code.
- **Consumers**: `lib/ai` and `bin` switch to `simrard-lib-utility-ai` and Bevy 0.18; remove `big-brain` dependency.

## Key files to touch

- **New**: `lib/utility_ai/` (crate root, Cargo.toml, NOTICE, re-export prelude).
- **New**: `lib/utility_ai/derive/` (proc-macro crate for `ScorerBuilder` / `ActionBuilder`).
- **Copy from big-brain**: `src/*.rs`, `derive/`* (minimal set: actions, choices, evaluators, measures, pickers, scorers, thinker; keep derive).
- **Edit**: Root `Cargo.toml` — add `lib/utility_ai` to `members`; remove `[patch.crates-io]` for bevy_render (no longer on 0.15).
- **Edit**: `lib/ai/Cargo.toml` — replace `big-brain` with `simrard-lib-utility-ai`, `bevy = "0.18"`.
- **Edit**: `lib/ai/ai.rs` — change `use big_brain::prelude::`* to `use simrard_lib_utility_ai::prelude::`* (and any type paths if re-exports differ).
- **Edit**: `bin/Cargo.toml` — `bevy = "0.18"`, depend on `simrard-lib-utility-ai` only via `simrard-lib-ai` (no direct big-brain); remove bevy_render patch.
- **Edit**: `bin/src/simrard.rs` — `BigBrainPlugin` from utility_ai; fix any Bevy 0.18 API breaks (schedules, systems, UI, Gizmos, etc.).
- **Edit**: Other libs that depend on Bevy (`charter`, `pawn`, `time`, `causal`, `transforms`) — bump to `bevy = "0.18"` where applicable and fix breakage.

## Execution order

1. Clone big-brain (main), copy `src/` and `derive/` into `lib/utility_ai` and `lib/utility_ai/derive`, add NOTICE and Cargo.toml(s) for Bevy 0.18.
2. Add `lib/utility_ai` to workspace members; remove bevy_render patch and bevy_patch_source/patches if unused.
3. Bump all crates to Bevy 0.18; point lib/ai and bin at simrard-lib-utility-ai.
4. Fix compile errors in utility_ai (0.17 → 0.18 migration: ScheduleLabel, Interned, system sets, etc.).
5. Fix compile errors in lib/ai and bin (imports, Bevy API changes).
6. Fix remaining libs (charter, pawn, time, causal, transforms) for Bevy 0.18 if they use Bevy.
7. Run `cargo build --workspace` and `cargo test --workspace`; run binary and fix runtime/visual issues (e.g. UI, Gizmos).

## Risks / fallbacks

- Bevy 0.18 may have large breaking changes (ECS, schedules, app builder). Fix incrementally by following compiler errors and Bevy 0.17→0.18 migration guide.
- If derive proc-macro is fragile under 0.18, we can replace with manual impls for our three scorers and three actions only.

