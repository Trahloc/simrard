#!/usr/bin/env bash
set -euo pipefail

./scripts/check_internal_fallbacks_in_diff.sh
timeout "${SIMRARD_TEST_TIMEOUT_SECONDS:-120}" cargo run -p simrard-bin -- --headless-test
