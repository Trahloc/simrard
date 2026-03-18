---
name: rust-r2026t Workspace Conventions
description: Core architecture rules for the rust-2026-Trahloc (r2026t) microcrate convention. Use this when creating crates, defining module structures, managing features, or configuring Cargo. Supersedes rust-r2025t.
---

# rust-r2026t Workspace Conventions

The **rust-2026-Trahloc Microcrate Convention (r2026t)** is a strict layout, naming, and configuration standard for Rust workspaces. It supersedes r2025t — layout invariants are unchanged; r2026t adds centralized dependency/lint configuration and toolchain pinning.

For full reference, see the linked docs in this directory.

## Core Invariant

> For every crate directory `X/`, the root module is `X.rs` (flattened) or `src/X.rs` (binary only).

**Banned filenames**: `mod.rs` (forbidden everywhere) · `lib.rs`/`main.rs` (forbidden as logic containers — max 5 lines as tooling redirects only)

## Quick Reference

| Topic | Reference |
|---|---|
| Workspace structure, crate naming, layout | [workspace-layout.md](workspace-layout.md) |
| `Cargo.toml` templates, `workspace.dependencies`, `workspace.lints`, `rust-toolchain.toml` | [cargo-config.md](cargo-config.md) |
| Unit tests (in-file) and integration test crate | [testing-strategy.md](testing-strategy.md) |
| Feature flags (downward flow) and splitting thresholds | [splitting-and-features.md](splitting-and-features.md) |
| `cargo-watch`, `sccache`, `mold`, `rust-analyzer` | [tooling.md](tooling.md) |

## Key Concepts at a Glance

- **Flattened microcrates**: no `src/` except in `bin/`
- **Features flow binary → libs**, never upward
- **Split by threshold** (5 criteria), not line count
- **Unit tests in-file**, integration tests in `tests/` workspace crate
- **All internal crates**: `publish = false`, `[lints] workspace = true`, `edition = "2024"`
- **External dep versions** declared once in `[workspace.dependencies]`, inherited with `{ workspace = true }`
- **Lint policy** declared once in `[workspace.lints]`, inherited by all members
