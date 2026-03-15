#!/usr/bin/env bash
set -euo pipefail

# Enforce "no fallback for internal systems" on newly added Rust lines.
# This checks only added lines in the current git diff to avoid blocking on legacy code.

ALLOWLIST_FILE="scripts/check_internal_fallbacks.allowlist"

DIFF_CONTENT="$(git diff --unified=0 --no-color -- bin lib)"

if [[ -z "$DIFF_CONTENT" ]]; then
  exit 0
fi

# Added lines only, excluding diff headers.
ADDED_LINES="$(printf '%s\n' "$DIFF_CONTENT" | rg '^\+[^+]' || true)"
if [[ -z "$ADDED_LINES" ]]; then
  exit 0
fi

# Patterns that usually indicate fallback masking in owned systems.
FALLBACK_HITS="$(
  printf '%s\n' "$ADDED_LINES" \
    | rg 'unwrap_or\(|unwrap_or_else\(|unwrap_or_default\(|\|\|\s*true|DefaultErrorHandler\(bevy::ecs::error::warn\)' \
    | rg -v 'EXTERNAL_FALLBACK_OK' \
    || true
)"

if [[ -n "$FALLBACK_HITS" && -f "$ALLOWLIST_FILE" ]]; then
  while IFS= read -r allowed_pattern; do
    [[ -z "$allowed_pattern" ]] && continue
    FALLBACK_HITS="$(printf '%s\n' "$FALLBACK_HITS" | rg -F -v "$allowed_pattern" || true)"
  done < "$ALLOWLIST_FILE"
fi

if [[ -n "$FALLBACK_HITS" ]]; then
  cat >&2 <<'MSG'
ERROR: Internal fallback pattern detected in added Rust code
CAUSE: New code introduces fallback behavior for systems under our control
REQUIRED ACTION: Replace fallback with fail-fast invariant/error handling
DIAGNOSTIC: Added lines containing fallback patterns are listed below

If this is an external dependency fallback, keep it loud and annotate the line with:
EXTERNAL_FALLBACK_OK: <reason>
MSG

  printf '%s\n' "$FALLBACK_HITS" >&2
  exit 1
fi
