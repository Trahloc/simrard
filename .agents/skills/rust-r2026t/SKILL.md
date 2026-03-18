---
name: rust-r2026t
description: Apply the rust-2026-Trahloc microcrate convention when making Rust layout, crate-boundary, Cargo, feature, or testing decisions in Trahloc-owned workspaces.
---

# rust-r2026t Workspace Conventions

Use this skill for Rust work in Trahloc-owned repositories when deciding:

- where new code belongs
- whether code should stay a module or become its own crate
- how to structure `Cargo.toml`, features, tests, and toolchain files
- whether an existing layout violation should be corrected instead of repeated

This skill is the operational guide. The detailed spec lives in [docs/rust-2026-trahloc.md](../../../docs/rust-2026-trahloc.md), with supporting references in this directory.

## Outcome

When applied correctly, this skill should produce:

- a workspace where file location matches responsibility
- no new `mod.rs`, logic-bearing `lib.rs`, or logic-bearing `main.rs`
- crate boundaries chosen by dependency/test/collaboration pressure, not by line count
- centralized external dependency and lint configuration
- tests placed where they compile fast and stay close to the code they verify

## Enforcement Posture

Do not perpetuate an existing violation just because it is already present.

- If new logic does not belong in the binary, place it in the correct library crate now.
- If you notice an existing violation during another task, do not let it hijack the current task.
- Raise the violation to the user explicitly and add a searchable `TODO:` comment at the relevant code site when that can be done safely and without changing the current task's scope.
- If the current task is already architectural or directly touches the violating code, correct the violation instead of only documenting it.
- If a broader migration is needed, call out the violation explicitly and avoid making it worse.

The standard is not relaxed for prototypes, diagnostics, temporary helpers, or deadline pressure.

Violation handling rule:

- prioritize the current task
- document noticed violations with `TODO:` comments where future cleanup needs to happen
- surface the issue to the user so they can decide whether to expand scope now or later

## Core Invariant

For every crate directory `X/`, there is exactly one crate whose root source file is:

- `X.rs` for flattened microcrates
- `src/X.rs` for the primary binary crate

Filename rules:

- `mod.rs` is forbidden everywhere
- `lib.rs` and `main.rs` may exist only as tooling redirects with no real logic
- `src/` is for the binary crate only

## Workflow

Apply this sequence whenever adding or moving Rust code.

### 1. Classify the code

Ask:

1. Does this code orchestrate app lifecycle, input, rendering, UI, process startup, or system wiring?
2. Or is it domain logic, state modeling, computation, transformation, policy, parsing, storage, or reusable helpers?

Placement rule:

- orchestration belongs in the binary crate
- domain logic belongs in a library crate

If the item has no real orchestration role, it does not belong in the binary even if nearby code currently does.

### 2. Decide whether to reuse, split, or create a crate

Default to an existing crate if it already owns the domain. Create or split only when one of these thresholds is met:

1. Type proliferation: 3 or more non-trivial public types or traits with meaningful impls
2. Dependency isolation: the code needs a dependency siblings should not inherit
3. Test divergence: the code needs materially different fixtures or test environment
4. Independent versioning or feature-gating: the code has a separate rollout surface
5. Collaboration boundary: separate people or agents can work on it independently

Do not split by file length alone.

### 3. Apply the correct workspace layout

Use these shapes:

- `bin/` for the front door and orchestration, using `src/`
- `lib/<name>/` for shared library microcrates, flattened with `<name>.rs`
- `tests/` as a workspace member crate for integration tests
- `command/<name>/` only for distinct CLI verb crates when the project actually has them

Reference: [workspace-layout.md](workspace-layout.md)

### 4. Configure Cargo the r2026t way

When touching manifests:

- keep external dependency versions in workspace root `[workspace.dependencies]`
- inherit them in member crates with `{ workspace = true }`
- keep internal path dependencies local to the member crate
- inherit lint policy with `[lints] workspace = true`
- keep `edition = "2024"`
- use `publish = false` for internal crates

Reference: [cargo-config.md](cargo-config.md)

### 5. Keep feature flow one-directional

Features flow downward:

- binary decides exposed features
- optional crates are gated by binary features
- libraries expose capabilities but do not coordinate upward or sideways

Reference: [splitting-and-features.md](splitting-and-features.md)

### 6. Place tests for fast iteration

- unit tests live in the same source file under `#[cfg(test)]`
- integration tests live in the dedicated `tests/` workspace crate
- do not use Cargo’s default loose `tests/*.rs` pattern as the main integration strategy

Reference: [testing-strategy.md](testing-strategy.md)

### 7. Validate the result

Before considering the work complete, verify:

1. no banned filenames were introduced as logic containers
2. new logic lives in the crate that matches its responsibility
3. manifests follow workspace dependency and lint inheritance rules
4. features still flow downward only
5. tests are in the correct tier
6. the changed crate or workspace still builds and tests cleanly

Reference: [tooling.md](tooling.md)

## Decision Shortcuts

Use these quick rules during implementation.

- If it is pure computation or reusable state, it is library code.
- If it only wires systems together, it can stay in the binary.
- If the code needs a dependency siblings should not pay for, isolate it.
- If you are tempted to add a `mod.rs`, the structure is wrong.
- If a new crate would not satisfy a real threshold, keep a module instead.

## Simrard-Specific Reminder

In Simrard, `bin/src/simrard.rs` is the orchestrator, not a dumping ground. HUD code, app setup, visual debug wiring, and input handling legitimately belong there. Pure simulation logic, domain state, and reusable transforms do not.

Because this repository carries a committed local copy of the skill for synchronization across workstations and collaborators, treat this repo-local version as the authoritative refinement when it differs from any personal global copy.

## References

- [docs/rust-2026-trahloc.md](../../../docs/rust-2026-trahloc.md)
- [workspace-layout.md](workspace-layout.md)
- [cargo-config.md](cargo-config.md)
- [splitting-and-features.md](splitting-and-features.md)
- [testing-strategy.md](testing-strategy.md)
- [tooling.md](tooling.md)

