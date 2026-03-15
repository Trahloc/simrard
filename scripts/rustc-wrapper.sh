#!/usr/bin/env bash
set -euo pipefail

# Cargo passes the real rustc path as argv[1] and all rustc args after it.
# EXTERNAL_FALLBACK_OK: sccache is an optional external compiler cache; plain
# rustc preserves correctness when sccache is unavailable, but we emit a loud
# warning so the loss of caching is explicit.
#
# If sccache is available, use it. Otherwise warn loudly and execute rustc
# directly so builds still succeed without changing compiler semantics.
if command -v sccache >/dev/null 2>&1; then
  exec sccache "$@"
fi

echo "WARNING: scripts/rustc-wrapper.sh could not find sccache; compiler caching is disabled." >&2
echo "WARNING: install sccache to avoid unnecessary recompilation of unchanged dependencies." >&2

exec "$@"
