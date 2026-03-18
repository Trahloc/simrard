# r2026t Cargo Configuration Reference

## Workspace `Cargo.toml` — Canonical Template

```toml
[workspace]
members = ["bin", "lib/*", "tests"]
# optional: "command/*"
resolver = "2"

# Centralized dep versions — all members inherit with { workspace = true }
[workspace.dependencies]
bevy = { version = "0.18", default-features = false }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
tracing = "0.1"
# Internal path deps are NOT hoisted here — declare per-crate

# Workspace-wide lint policy — inherited with [lints] workspace = true
[workspace.lints.rust]
unused_imports = "deny"
dead_code = "deny"
unused_variables = "deny"
unused_must_use = "deny"

[workspace.lints.clippy]
all = "warn"
pedantic = "warn"

[profile.dev]
opt-level = 0
debug = true
incremental = true

[profile.dev.package."*"]
opt-level = 2      # optimize deps, fast incremental for your code

[profile.release]
opt-level = 3
lto = "thin"
codegen-units = 1
```

## Member Crate `Cargo.toml` — Lib Template

```toml
[package]
name = "myapp-lib-common"
version = "0.1.0"
edition = "2024"
publish = false     # all internal crates

[lib]
name = "myapp_lib_common"
path = "common.rs"  # explicit path — no src/

[lints]
workspace = true    # inherit from [workspace.lints]

[dependencies]
# External: inherit version from workspace
serde = { workspace = true }
tracing = { workspace = true }
# Internal: path deps declared explicitly (not hoisted)
myapp-lib-other = { path = "../other" }
```

## Binary Crate `Cargo.toml` — Template

```toml
[package]
name = "myapp-bin"
version = "0.1.0"
edition = "2024"
publish = false     # omit if this binary is shipped to crates.io

[[bin]]
name = "myapp"
path = "src/myapp.rs"

[lints]
workspace = true

[features]
default = ["install"]
install = ["myapp-command-install"]
verbose-logging = ["myapp-lib-common/logging"]   # propagated feature

[dependencies]
myapp-lib-common = { path = "../lib/common" }
bevy = { workspace = true }
myapp-command-install = { path = "../command/install", optional = true }
```

## `rust-toolchain.toml` — Workspace Root

```toml
[toolchain]
channel = "stable"
components = ["rustfmt", "clippy", "rust-src"]
```

Pin to a specific release for full reproducibility:
```toml
channel = "1.85.0"
```

## `.cargo/config.toml` — Workspace Root

```toml
[target.x86_64-unknown-linux-gnu]
linker = "clang"
rustflags = ["-C", "link-arg=-fuse-ld=mold"]

[build]
rustc-wrapper = "sccache"   # comment out if sccache not installed
```

## Key Rules

- `publish = false` on ALL internal crates (`lib/*`, `command/*`, `tests/`)
- `[lints] workspace = true` in EVERY member — no exceptions
- All external dep versions in `[workspace.dependencies]`, inherited with `{ workspace = true }`
- Internal path deps stay per-crate — do not hoist to workspace
- `edition = "2024"` everywhere — no `"2021"` allowed
