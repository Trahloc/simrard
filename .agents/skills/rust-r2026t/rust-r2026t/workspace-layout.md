# r2026t Workspace Layout Reference

## Workspace Structure

```
<workspace-root>/
├── Cargo.toml              # workspace root (see cargo-config.md)
├── rust-toolchain.toml     # toolchain pin
├── .cargo/
│   └── config.toml         # linker + sccache
├── bin/                    # binary front door (uses src/)
├── lib/                    # shared microcrates (flattened)
│   ├── common/
│   └── ai/
├── tests/                  # integration test crate (flattened)
└── command/                # optional: CLI verb crates (flattened)
```

## Crate Naming

`<project>-<kind>-<tld>` — where `<kind>` is `bin`, `lib`, `tests`, or `command`.

| Directory | Package Name | Lib/Bin Name |
|---|---|---|
| `bin/` | `myapp-bin` | binary `myapp` |
| `lib/common/` | `myapp-lib-common` | `myapp_lib_common` |
| `lib/ai/` | `myapp-lib-ai` | `myapp_lib_ai` |
| `tests/` | `myapp-tests` | `myapp_tests` |
| `command/install/` | `myapp-command-install` | `myapp_command_install` |

## Flattened Microcrate Layout (`lib/`, `command/`)

No `src/` directory. `Cargo.toml` lives alongside `X.rs`:

```
lib/common/
├── Cargo.toml      # see cargo-config.md for template
├── common.rs       # crate root — named after the directory
├── fs_utils.rs     # module
└── xdg.rs          # module
```

```rust
// common.rs — crate root
mod fs_utils;
mod xdg;
pub use fs_utils::*;
pub use xdg::*;
```

## Binary Layout (`bin/` — uses `src/`)

```
bin/
├── Cargo.toml
└── src/
    ├── myapp.rs    # orchestrator — all logic here
    └── main.rs     # optional redirect only (max 5 lines)
```

## Nested Microcrates

Nested crates are allowed as **implementation details** of their parent:

```
command/install/
├── Cargo.toml
├── install.rs
└── migration/          # impl detail — not used by siblings
    ├── Cargo.toml
    └── migration.rs
```

**Promote** to `lib/<name>/` when: needed by siblings · independent versioning needed · no longer an impl detail.

## Banned Filenames

- **`mod.rs`**: Strictly forbidden everywhere
- **`lib.rs`** / **`main.rs`**: Forbidden as logic containers. Allowed only as 5-line tooling redirects

```rust
// lib.rs — redirect only
mod common;
pub use common::*;
```

**Never** name a crate `core` — shadows `std::core`. Use `common`, `shared`, or `kernel`.
