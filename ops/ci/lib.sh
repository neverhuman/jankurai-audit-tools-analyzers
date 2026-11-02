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

# assert_repair_queue <queue> <report> -- validate the repair queue the audit
# promised. The auditor writes one JSON object per line and, for an audit with
# no findings, a single newline: zero records is a valid queue, a zero-byte
# file is a truncated write. Every record must carry the fields the
# schemas/repair-queue.schema.json contract requires as strings, and the record
# count must equal the report's agent_fix_queue, so output left by an earlier
# run or cut short cannot pass.
assert_repair_queue() {
  local queue="$1" report="$2" records expected
  assert_artifact "$queue"
  if [ ! -s "$REPO_ROOT/$queue" ]; then
    printf '[ci] repair queue is empty (zero bytes, truncated write?): %s\n' "$queue" >&2
    return 1
  fi
  require_tool jq || return 1
  if ! records="$(jq -e -s '
      if all(.[]; type == "object"
          and (.path | type == "string") and (.priority | type == "string")
          and (.task | type == "string") and (.why | type == "string"))
      then length else error("record missing a required string field") end
    ' "$REPO_ROOT/$queue" 2>&1)"; then
    printf '[ci] repair queue is malformed: %s: %s\n' "$queue" "$records" >&2
    return 1
  fi
  expected="$(jq -e '(.agent_fix_queue // []) | length' "$REPO_ROOT/$report")" || {
    printf '[ci] cannot read agent_fix_queue from %s\n' "$report" >&2
    return 1
  }
  if [ "$records" != "$expected" ]; then
    printf '[ci] repair queue is stale or partial: %s has %s records, %s lists %s\n' \
      "$queue" "$records" "$report" "$expected" >&2
    return 1
  fi
  log "repair queue valid: $records record(s)"
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
