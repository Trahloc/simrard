# r2026t Splitting & Features Reference

## When to Split: Microcrate Thresholds

**Do NOT split by line count.** Split a module into its own microcrate only when **any** threshold is met:

| # | Criterion | Example |
|---|---|---|
| 1 | **Type Proliferation** | 3+ non-trivial public types/traits with impl blocks (`Config`, `ConfigBuilder`, `ConfigValidator`) |
| 2 | **Dependency Isolation** | Module needs `serde_json` but the rest of the crate doesn't |
| 3 | **Test Divergence** | Tests need a live database / external service; siblings use mocks |
| 4 | **Independent Versioning** | Code needs separate semver or feature-gating |
| 5 | **Collaboration Boundary** | Two developers/agents would reasonably work on it simultaneously |

**Anti-patterns** — do NOT split for these reasons:
- ❌ "This file has 500 lines"
- ❌ "We have 5 modules, let's make 5 microcrates"
- ❌ "This feels like a separate thing"

## Decision Tree

```
Is this code used by multiple crates?
├─ Yes → Move to lib/ (shared library)
└─ No → Keep in current crate as module or nested microcrate
         └─ Does it meet a split criterion?
            ├─ Yes → Split to nested microcrate (or lib/ if shared)
            └─ No → Keep as module
```

## Promotion Path

1. **Module** in parent crate (default — start here)
2. **Nested Microcrate** if it meets a threshold but is still an impl detail
3. **Promoted Library** (`lib/<name>/`) when needed by siblings or has independent identity

**Goal**: Change one file → rebuild one crate → tests in **< 2 seconds**.

---

## Feature Flag Strategy

Features flow **downward from binary to libraries. Never upward.**

```
Binary (Root)
  ├─→ Optional crates (commands, plugins) — gated by features
  └─→ Library crates (required or optional)
        └─→ (no upward coordination between libs)
```

### Naming Conventions

- Verb/command features: `install`, `fix`, `status`
- Capability features: `verbose-logging`, `network-retry`, `async-backend`
- Propagated features: `crate-name/feature-name` syntax

### Example — Full Feature Chain

```toml
# bin/Cargo.toml
[features]
default = ["install", "fix"]
install = ["myapp-command-install"]
fix = ["myapp-command-fix"]
full = ["install", "fix"]
verbose-logging = ["myapp-lib-common/logging"]   # propagated

[dependencies]
myapp-command-install = { path = "../command/install", optional = true }
myapp-lib-common = { path = "../lib/common" }
```

```toml
# lib/common/Cargo.toml
[features]
default = []
logging = ["tracing"]

[dependencies]
tracing = { workspace = true, optional = true }
```

```bash
# User runs:
cargo build --features verbose-logging
# Flow: verbose-logging → myapp-lib-common/logging → tracing enabled
```

### Rules

- Binary decides what is included and what features are exposed
- Binary propagates features to libraries
- Libraries do NOT coordinate features horizontally or upward
- Optional crates must be optional dependencies gated by features
