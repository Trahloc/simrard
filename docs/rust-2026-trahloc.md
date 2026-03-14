# Rust Project Layout: rust-2026-Trahloc Microcrate Convention (r2026t)

## 0. Scope

The **rust-2026-Trahloc Microcrate Convention (r2026t)** defines a strict directory, naming, and configuration standard for Rust workspaces designed to:

1. **Eliminate File Ambiguity**: Prevent AI context hallucinations from multiple `lib.rs`, `main.rs`, or `mod.rs` files.
2. **Optimize Compilation**: Enforce boundaries for sub-2-second incremental builds.
3. **Structure for AI Agents**: Deterministic mapping where file location equals logical responsibility.
4. **Minimize Path Noise**: Flatten microcrate structure by eliminating unnecessary `src/` directories.
5. **Centralize Configuration**: Shared deps, lints, and edition declared once at workspace root.

This is a **layout, naming, and configuration spec only**. It does not change Rust semantics or compiler behavior.

**Supersedes**: `r2025t`. The layout invariants are identical; r2026t adds centralized dependency inheritance, workspace-level lint configuration, toolchain pinning, and progressive build safety.

---

## 1. Core Principles (Unchanged from r2025t)

1. **Uniqueness by Top-Level Directory (TLD)**: For every logical component X, there is exactly one directory `X/` containing exactly one crate whose root source file is `X.rs`.

2. **Banned Filenames**:
   - `mod.rs` is **strictly forbidden**. Use the Rust 2018 `foo.rs` + `foo/` pattern.
   - `lib.rs` and `main.rs` are **forbidden as logic containers**. They exist only as optional tooling redirects (max 5 lines, zero logic).

3. **Flattened Microcrates**: Microcrates omit the `src/` directory. The `Cargo.toml` lives alongside the root `X.rs`. The `src/` directory is reserved for the binary crate only.

4. **One Binary Front Door**: Exactly one primary binary crate responsible for the main executable.

5. **Backwards Compatibility over Ideological Purity**: When tooling requires `lib.rs` or `main.rs`, those files exist as redirects. The convention adapts to the ecosystem, not the reverse.

---

## 2. Workspace Structure

```
<workspace-root>/
├── Cargo.toml          # workspace root (see §3 for full template)
├── rust-toolchain.toml # toolchain pin (new in r2026t)
├── .cargo/
│   └── config.toml     # linker + sccache config
├── bin/                # binary front door (uses src/)
├── lib/                # shared library microcrates (flattened)
│   ├── common/
│   └── ai/
├── tests/              # integration test crate (flattened)
└── command/            # optional: CLI verb crates (flattened)
    ├── install/
    └── status/
```

---

## 3. Workspace Cargo.toml (r2026t canonical template)

```toml
[workspace]
members = ["bin", "lib/*", "tests"]
# optional: add "command/*" for CLI verb crates
resolver = "2"

# ── Centralized dependency versions (r2026t) ──────────────────────────────
# All member crates inherit with: dep = { workspace = true }
# Feature overrides: dep = { workspace = true, features = ["extra"] }
[workspace.dependencies]
bevy = { version = "0.18", default-features = false }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
tracing = "0.1"
# Add all shared external deps here. Internal crates use path deps, not workspace.

# ── Workspace-wide lint configuration (r2026t) ───────────────────────────
[workspace.lints.rust]
unused_imports = "deny"
dead_code = "deny"
unused_variables = "deny"
unused_must_use = "deny"

[workspace.lints.clippy]
all = "warn"
pedantic = "warn"

# ── Build profiles ─────────────────────────────────────────────────────────
[profile.dev]
opt-level = 0
debug = true
incremental = true

[profile.dev.package."*"]
opt-level = 2      # optimize deps, keep your code fast to compile

[profile.release]
opt-level = 3
lto = "thin"
codegen-units = 1  # better optimization, acceptable release-only cost
```

---

## 4. Toolchain Pinning (r2026t addition)

Place `rust-toolchain.toml` at the workspace root to guarantee reproducible builds across machines and CI:

```toml
# rust-toolchain.toml
[toolchain]
channel = "stable"
components = ["rustfmt", "clippy", "rust-src"]
```

Pin to a specific release for maximum reproducibility:
```toml
channel = "1.85.0"
```

---

## 5. Crate Naming & Layout

### Package Name Format

`<project>-<kind>-<tld>` where `<kind>` is `bin`, `lib`, `tests`, or optionally `command`.

| Directory | Package Name | Lib/Bin Name |
|---|---|---|
| `bin/` | `myapp-bin` | binary `myapp` |
| `lib/common/` | `myapp-lib-common` | `myapp_lib_common` |
| `lib/ai/` | `myapp-lib-ai` | `myapp_lib_ai` |
| `tests/` | `myapp-tests` | `myapp_tests` |
| `command/install/` | `myapp-command-install` | `myapp_command_install` |

### Flattened Microcrate `Cargo.toml` (r2026t template)

```toml
[package]
name = "myapp-lib-common"
version = "0.1.0"
edition = "2024"
publish = false      # prevents accidental crates.io publish (r2026t)

[lib]
name = "myapp_lib_common"
path = "common.rs"   # explicit; no src/

# Inherit lint config from workspace
[lints]
workspace = true

[dependencies]
# Inherit versions from workspace:
serde = { workspace = true }
# Internal path deps declared explicitly (not hoisted):
myapp-lib-other = { path = "../other" }
```

### Binary Crate `Cargo.toml` (r2026t template)

```toml
[package]
name = "myapp-bin"
version = "0.1.0"
edition = "2024"
publish = false

[[bin]]
name = "myapp"
path = "src/myapp.rs"   # orchestrator — all logic here

[lints]
workspace = true

[features]
default = ["install"]
install = ["myapp-command-install"]
verbose-logging = ["myapp-lib-common/logging"]

[dependencies]
myapp-lib-common = { path = "../lib/common" }
bevy = { workspace = true }
myapp-command-install = { path = "../command/install", optional = true }
```

### Key r2026t config rules

- `publish = false` on every internal crate (all `lib/*`, `command/*`, `tests/`; keep default on `bin/` if you ship it)
- `[lints] workspace = true` in every member crate — inherits from `[workspace.lints]`
- All external dep versions declared in `[workspace.dependencies]`, inherited with `{ workspace = true }`
- Do NOT hoist internal path deps to workspace — they remain in member `Cargo.toml`

---

## 6. Banned Filenames & Tooling Redirects

**`mod.rs` — Strictly Forbidden everywhere.**

**`lib.rs` / `main.rs` — Forbidden as logic containers.** Only as tooling redirects (max 5 lines):

```rust
// lib.rs — tooling compatibility redirect only
// All logic lives in common.rs
mod common;
pub use common::*;
```

```rust
// main.rs — tooling compatibility redirect only
// All logic lives in myapp.rs
mod myapp;
fn main() { myapp::run(); }
```

**Naming restrictions**: Never name a microcrate `core` — shadows `std::core`. Use `common`, `shared`, or `kernel`.

---

## 7. Feature Flag Strategy

Features flow **downward from binary to libraries. Never upward.**

```
Binary (Root)
  ├─→ Optional crates (commands, plugins) gated by features
  └─→ Library crates (required or optional)
        └─→ (no upward coordination)
```

### Naming
- Verb/command features: `install`, `fix`, `status`
- Capability features: `verbose-logging`, `network-retry`
- Propagated features: `crate-name/feature-name` syntax

---

## 8. When to Split: Microcrate Thresholds

**Do NOT split by line count.** Split when **any** threshold is met:

| # | Criterion | Example trigger |
|---|---|---|
| 1 | **Type Proliferation** | 3+ non-trivial public types/traits with impl blocks |
| 2 | **Dependency Isolation** | Module needs a heavy dep nothing else uses |
| 3 | **Test Divergence** | Tests need materially different fixtures (DB vs mock) |
| 4 | **Independent Versioning** | Needs separate semver or feature-gating |
| 5 | **Collaboration Boundary** | Two developers/agents reasonably work on it simultaneously |

**Promotion path**: Module → Nested Microcrate (impl detail of parent) → `lib/<name>/` (shared library)

**Goal**: Change one file → rebuild one crate → tests in **< 2 seconds**.

---

## 9. Nested Microcrates

Allowed when the nested crate is an **implementation detail** of its parent. Each nested crate has its own `Cargo.toml` and follows `X/X.rs`.

**Promote** from nested → `lib/<name>/` when: needed by siblings, needs independent versioning, or is no longer an impl detail.

---

## 10. Testing Strategy

### Unit Tests (In-File)

```rust
pub fn validate(x: u32) -> bool { x > 0 }

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_validate() { assert!(validate(1)); }
}
```

### Integration Tests (Workspace Crate, not bare tests/ folder)

```toml
# tests/Cargo.toml
[package]
name = "myapp-tests"
version = "0.1.0"
edition = "2024"
publish = false

[lints]
workspace = true

[[test]]
name = "integration"
path = "integration.rs"

[dependencies]
myapp-bin = { path = "../bin" }
myapp-lib-common = { path = "../lib/common" }
```

```bash
cargo test -p myapp-lib-common   # unit tests for one crate
cargo test -p myapp-tests        # integration tests
cargo test --workspace           # everything
```

---

## 11. Development Tooling

**Philosophy**: Optimize for fastest in human time, not CPU time.

### `.cargo/config.toml`

```toml
[target.x86_64-unknown-linux-gnu]
linker = "clang"
rustflags = ["-C", "link-arg=-fuse-ld=mold"]

[build]
rustc-wrapper = "sccache"  # comment out if sccache not installed
```

### Recommended Tools

| Tool | Install | Purpose |
|---|---|---|
| `cargo-watch` | `cargo install cargo-watch` | Auto-rebuild on file save |
| `sccache` | `cargo install sccache` | Shared compilation cache |
| `mold` | system package manager | Fast linker (Linux) |

### Iteration Loop

```bash
cargo watch -x check                        # continuous type checking
cargo watch -x "test -p myapp-lib-common"   # test one crate on save
cargo test --workspace                      # full suite
cargo run -p myapp-bin                      # run the binary
```

### rust-analyzer settings (settings.json)

```json
{
    "rust-analyzer.checkOnSave.command": "clippy",
    "rust-analyzer.cargo.buildScripts.enable": true,
    "rust-analyzer.procMacro.enable": true
}
```

**Performance targets**: Incremental rebuild < 2s · Type check < 1s · Unit tests near-instant.

---

## 12. The Uniqueness Invariant

> For every crate directory `X/` under the workspace, there exists exactly one crate whose root module is `X.rs` (flattened) or `src/X.rs` (binary only). Files named `lib.rs`, `main.rs`, or `mod.rs` exist only as tooling-compatibility redirects containing no logic. `mod.rs` is strictly forbidden in all contexts.

---

## 13. r2026t vs r2025t Delta

| Area | r2025t | r2026t |
|---|---|---|
| Shared dep versions | Per-crate `[dependencies]` only | `[workspace.dependencies]` + `{ workspace = true }` |
| Lint config | `.cargo/config.toml` rustflags | `[workspace.lints]` + `[lints] workspace = true` |
| Toolchain | Unspecified | `rust-toolchain.toml` at workspace root |
| `publish = false` | Only on `tests/` | All internal crates |
| Edition | 2021 in some examples | 2024 everywhere, no exceptions |
| Layout invariants | ✓ | Unchanged |

---

## 14. External Repositories

Cloned third-party repositories (for reference, patching, or upstream PRs) MUST be placed in the `/external/` directory.

The standard `.gitignore` ensures that the massive histories and files of these third-party repos do not pollute the workspace's version control. However, we explicitly track ALL `README.md` files within `external/` as breadcrumbs. The root `external/README.md` acts as an index of what repos we have and why, and the individual `external/<repo>/README.md` files are the original upstream readmes kept to provide context on the clone.
