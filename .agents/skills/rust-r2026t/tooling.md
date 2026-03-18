# r2026t Development Tooling Reference

## Philosophy

**Optimize for fastest in human time, not fastest in CPU time.** Small crate boundaries are a compilation unit strategy — every microcrate is a parallel compilation unit and a fast incremental rebuild target.

## Required Tools

| Tool | Install | Purpose |
|---|---|---|
| `cargo-watch` | `cargo install cargo-watch` | Auto-rebuild on file save |
| `sccache` | `cargo install sccache` | Shared compilation cache across projects |
| `mold` | system package manager | Faster linker (Linux — significant for large projects) |

## `.cargo/config.toml`

```toml
[target.x86_64-unknown-linux-gnu]
linker = "clang"
rustflags = ["-C", "link-arg=-fuse-ld=mold"]

[build]
rustc-wrapper = "sccache"   # comment out if sccache not installed
```

## Iteration Loop

```bash
# 1. Background: continuous type checking
cargo watch -x check

# 2. Edit code — errors appear within seconds

# 3. Test one crate
cargo test -p myapp-lib-common

# 4. Test one crate on every save
cargo watch -x "test -p myapp-lib-common"

# 5. Run the binary
cargo run -p myapp-bin

# 6. Lint
cargo clippy --workspace
```

**Tips:**
- Use `cargo check` instead of `cargo build` during active editing — much faster
- Use `cargo test --lib` to skip integration tests on quick iterations
- Test in isolation first (`-p <crate>`), then full workspace

## rust-analyzer (VS Code / Cursor)

```json
{
    "rust-analyzer.checkOnSave.command": "clippy",
    "rust-analyzer.cargo.buildScripts.enable": true,
    "rust-analyzer.procMacro.enable": true
}
```

## Performance Targets

| Metric | Target |
|---|---|
| Incremental rebuild (one crate change) | < 2 seconds |
| Type check (`cargo watch`) | < 1 second |
| Unit tests (one crate) | near-instant |
| Full workspace build | leverages parallel compilation |
