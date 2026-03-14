# r2025t Rule Set Index

The **rust-2025-Trahloc Microcrate Convention (r2025t)** is a generic layout and naming standard for Rust workspaces. These rules live in `.cursor/rules/` and can be referenced as a set (e.g. "follow r2025t" or "see .cursor/rules for r2025t").

| Rule file | Purpose |
|-----------|---------|
| `rust-r2025t-core.mdc` | Core principles, workspace structure, uniqueness invariant |
| `rust-r2025t-crate-structure.mdc` | Package/library naming, flattened layout, binary vs lib, nested crates |
| `rust-r2025t-features.mdc` | Feature flow (binary → libs), optional crates, propagation |
| `rust-r2025t-file-naming.mdc` | Root file = X.rs, banned mod.rs/lib.rs/main.rs, redirects |
| `rust-r2025t-microcrate-splitting.mdc` | When to split into a microcrate, promotion path |
| `rust-r2025t-testing.mdc` | Unit tests in-file, integration tests in `tests/` crate |
| `rust-r2025t-tooling.mdc` | cargo-watch, sccache, mold, rust-analyzer, workflow |

Replace placeholder `<project>` / `myapp` with your project name when applying the convention.
