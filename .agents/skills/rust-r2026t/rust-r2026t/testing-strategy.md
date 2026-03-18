# r2026t Testing Strategy Reference

## Two-Tier Testing

### Unit Tests — In-File

Tests for internal logic live **inside the source file they test**, under `#[cfg(test)] mod tests`. Never in a separate file.

```rust
// common.rs
pub fn validate_path(path: &Path) -> bool {
    path.exists() && path.is_dir()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_path_returns_false_for_missing() {
        let path = Path::new("/nonexistent/path");
        assert!(!validate_path(path));
    }
}
```

**Why**: AI and humans see code and tests together in one context. Tests compile only when that crate is tested. Private functions are testable without `pub(crate)` gymnastics. Incremental builds: changing one file rebuilds only that crate's tests.

### Integration Tests — Workspace Crate

Integration tests live in a dedicated workspace member crate called `tests/`. This is **NOT** Cargo's default bare `tests/` folder (which compiles each `.rs` file as a separate binary — slow). A single crate compiles once.

```
tests/
├── Cargo.toml
├── integration.rs      # crate root
├── install_flow.rs     # test module
└── e2e_flow.rs         # test module
```

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

[dev-dependencies]
myapp-bin = { path = "../bin" }
myapp-lib-common = { path = "../lib/common" }
```

```rust
// integration.rs — crate root
mod install_flow;
mod e2e_flow;
```

## Running Tests

```bash
# Unit tests for one crate only
cargo test -p myapp-lib-common

# All integration tests
cargo test -p myapp-tests

# Everything
cargo test --workspace

# Fast iteration: test one crate on every save
cargo watch -x "test -p myapp-lib-common"
```

## Rules

1. Unit tests: always in-file, never in a separate `tests/` subdirectory
2. Integration tests: in the `tests/` workspace crate, not bare Cargo test folders
3. Shared fixtures belong in the test crate root or a `fixtures.rs` module
4. Test crate must be a workspace member with its own `Cargo.toml`
