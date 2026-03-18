# .cursor/rules Index

This directory contains 3 consolidated rules:

| Rule file | Scope | `alwaysApply` | Purpose |
|---|---|---|---|
| `rust-r2026t.mdc` | Any r2026t workspace | false | Complete r2026t convention — layout, naming, Cargo config, features, testing, tooling |
| `simrard-conventions.mdc` | simrard only | false | Wolfram tiebreaker, Observer mode invariants, dead code policy |
| `baseline-agent-practices.mdc` | All repos | true | Fail-fast, warnings-as-errors, git safety, TODO conventions |

These rules mirror the `.agents/skills/` files. Both formats carry identical information; cursor rules are denser for context efficiency, agent skills use progressive disclosure for context budget management.
