#!/usr/bin/env bash
# Local entry point for the CI lanes. Delegates to the exact same
# ops/ci/<lane>.sh scripts forge CI calls, so local runs
# never drift from CI.
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/.."

lane="${1:-all}"
case "$lane" in
  required) bash ops/ci/required.sh ;;
  fast)     bash ops/ci/fast.sh ;;
  security) bash ops/ci/security.sh ;;
  audit)    bash ops/ci/audit.sh ;;
  gates|all) bash ops/ci/quality-gates.sh ;;
  *) echo "usage: $0 {required|fast|security|audit|gates|all}" >&2; exit 2 ;;
esac
