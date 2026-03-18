---
name: Simrard Development Environment
description: Repo-specific constraints, tiebreakers, and coding standards for the Simrard simulator. Use this for architectural decisions and code style specific to Simrard.
---

# Simrard Development Skill

Simrard is a "true simulation" in the lineage of Will Wright's Maxis titles. The player is a gardener of systems, not a commander of units. Every mechanic must be expressible as a composable transform; emergent behavior in Observer mode (zero player input) is the primary design success criterion.

---

## 1. Wolfram-Alignment Tiebreaker

When multiple design or implementation options are viable, apply this tiebreaker in order:

1. **Prefer the option that best aligns with Wolfram Physics Project philosophy:**
   - Causality is the fundamental substrate.
   - Space and time are emergent from causal structure.
   - Propagation (e.g., the Causal Speed Constant `C`) is a property of causal rules, not of space.
2. **If multiple options still tie**, prefer the one that is computationally simpler or faster — so long as it still respects rule 1.

**Do not** choose a faster/simpler option that inverts the causality-first ontology (e.g., treating space as primary and causal propagation as merely a spatial property) unless there is a compelling practical constraint.

---

## 2. Crate Layout & Edition

Simrard uses the **r2026t** microcrate convention (see the `rust-r2026t` skill for the full spec).

```
simrard/
├── Cargo.toml          # workspace root
├── bin/                # binary front door (src/)
├── lib/                # library microcrates (flattened, no src/)
│   ├── ai/
│   ├── causal/
│   ├── charter/
│   ├── time/
│   ├── transforms/
│   └── utility_ai/
├── tests/              # integration test crate
└── docs/
```

- **Edition**: All crates use **Rust 2024**. Keep `edition = "2024"` in every `Cargo.toml`.
- Binary entry point: `bin/src/simrard.rs` (the orchestrator).

---

## 3. Strict Dead Code & Warning Intolerance

Warnings are build-failing errors. The project enforces `-D warnings` via `.cargo/config.toml`.

- **No `#[allow(unused)]` or `#![allow(dead_code)]`** without explicit user permission.
- Unimplemented or future behavior belongs in `TODO` comments with the code **removed or stubbed out**, not left as dead code. Do not leave unused symbols "for later".
- If suppression is absolutely necessary and user-approved, document why with a dated TODO:

```rust
// User approved (2026-03-14): Kept for future GPU NN integration.
// TODO(2026-04-14): Remove once wgpu inference path is implemented.
#[allow(dead_code)]
pub fn gpu_nn_stub() {}
```

---

## 4. Observer Mode as the North Star

The simulation must generate interesting emergent narrative drama with **zero player input**. This is the hardest design constraint and always the first thing to prove.

- Never hard-code job logic into systems.
- Never use a black-box AI that cannot be expressed as a transform.
- Never ship a sim that dies without player input in Observer mode.
- Never bake a design that blocks future causal propagation or GPU NN upgrade.

---

## 5. Repo Conventions & Exceptions

- `.cursor/rules/` **and** `.agents/skills/` are intentionally **committed** to this repo.
  - Do not add them to `.gitignore` or `.git/info/exclude` in this project.
- All other standard r2026t and baseline-agent-practices rules apply (see their respective skills).
