#!/usr/bin/env bash
# Shared CI helper module sourced by every ops/ci/<lane>.sh script.
# Single source of truth for tool version pins and artifact assertions so
# local runs and forge CI execute the exact same commands.
set -euo pipefail

# Resolve the repository root regardless of where a lane is invoked from.
REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
export REPO_ROOT

# Pinned tool versions. Lanes read these so CI and local environments match.
export RUST_TOOLCHAIN="${RUST_TOOLCHAIN:-1.97.1}"
export GITLEAKS_VERSION="${GITLEAKS_VERSION:-8.21.2}"
export CARGO_AUDIT_VERSION="${CARGO_AUDIT_VERSION:-0.22.1}"
export NEXTEST_VERSION="${NEXTEST_VERSION:-0.9.137}"

# log <message> -- emit a structured progress line.
log() {
  printf '[ci] %s\n' "$*"
}

# require_tool <binary> -- fail fast with an actionable message when a pinned
# tool is missing from PATH so local parity gaps surface before the lane runs.
require_tool() {
  local tool="$1"
  if ! command -v "$tool" >/dev/null 2>&1; then
    printf '[ci] missing required tool: %s\n' "$tool" >&2
    return 1
  fi
}

# assert_artifact <path> -- confirm a lane produced the artifact it promised.
assert_artifact() {
  local artifact="$1"
  if [ ! -e "$REPO_ROOT/$artifact" ]; then
    printf '[ci] expected artifact missing: %s\n' "$artifact" >&2
    return 1
  fi
  log "artifact present: $artifact"
}

# Resolve once so the audit and its receipt describe the same executable.
require_auditor() {
  local want got version digest receipt
  receipt="$REPO_ROOT/target/jankurai/auditor.txt"
  mkdir -p "${receipt%/*}"
  rm -f -- "$receipt"
  want="$(sed -n 's/^[[:space:]]*auditor_version[[:space:]]*=[[:space:]]*"\([^"]*\)"[[:space:]]*\(#.*\)\{0,1\}$/\1/p' "$REPO_ROOT/agent/standard-version.toml")"
  if [[ -z "$want" || "$want" == *$'\n'* ]]; then
    printf '[ci] missing or invalid auditor_version pin\n' >&2
    return 1
  fi
  if ! JANKURAI_BIN="$(type -P jankurai)"; then
    printf '[ci] missing required tool: jankurai\n' >&2
    return 1
  fi
  # Absolute paths keep relative PATH entries stable if a caller changes cwd.
  JANKURAI_BIN="$(cd "$(dirname "$JANKURAI_BIN")" && pwd)/$(basename "$JANKURAI_BIN")"
  version="$("$JANKURAI_BIN" --version)" || return 1
  got="${version#jankurai }"
  if [[ "$version" != "jankurai $want" ]]; then
    printf '[ci] auditor version mismatch: want %s got %s\n' "$want" "$got" >&2
    return 1
  fi
  digest="$(sha256sum -- "$JANKURAI_BIN")" || return 1
  printf 'version: %s\nsha256: %s\n' "$version" "${digest%% *}" > "$receipt"
}
