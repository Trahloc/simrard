# Copilot Instructions (VS Code)

This file exists because VS Code/Copilot does not natively enforce Cursor `.mdc` rules.
Treat this as the canonical in-repo instruction source for Copilot behavior.

## Internal Systems: No Fallback

For systems we fully control (our own crates, configs, assets, schemas, transforms, and runtime resources), do not implement silent fallback behavior.

Required behavior:

1. If X is required, fail immediately when X is missing or invalid.
2. Emit a loud, actionable failure message that includes component, cause, and required fix.
3. Do not auto-substitute Y for missing/broken X in owned systems.

Examples:

- Forbidden: `unwrap_or(...)`, `unwrap_or_default()`, or branch-to-default behavior that masks missing internal state.
- Required: explicit error return, `expect(...)`, `panic!(...)` for violated invariants, or early process exit with diagnostics.

## Exception: External Dependencies Only

Fallback is allowed only for external dependencies (network/API/service reachability), and must include:

1. Loud warning message.
2. Explicit reason for fallback.
3. Marker comment in code: `EXTERNAL_FALLBACK_OK: <reason>`.

## Enforcement

Use `scripts/check_internal_fallbacks_in_diff.sh` during local validation and CI/pre-merge checks.
Any newly introduced internal fallback patterns in `bin/` or `lib/` should fail validation.
