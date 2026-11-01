#!/usr/bin/env bash
# Jankurai self-audit lane: writes the repo-score artifacts that CI uploads.
# The same lane runs locally via `just audit`.
set -euo pipefail
source "$(dirname "${BASH_SOURCE[0]}")/lib.sh"
cd "$REPO_ROOT"
require_auditor

mkdir -p .jankurai target/jankurai
log "audit lane: jankurai audit -> .jankurai/repo-score.{json,md}, target/jankurai/repair-queue.jsonl"
# The repair queue is the routing artifact the next agent reruns from: one JSON
# object per finding, contracted by schemas/repair-queue.schema.json. The audit
# can exit nonzero while still writing its evidence, so keep the artifacts and
# let the assertions below decide whether the lane produced what it promised.
audit_status=0
"$JANKURAI_BIN" audit . --no-score-history --json .jankurai/repo-score.json --md .jankurai/repo-score.md --full \
  --repair-queue-jsonl target/jankurai/repair-queue.jsonl || audit_status=$?

assert_artifact .jankurai/repo-score.json
assert_artifact .jankurai/repo-score.md
assert_artifact target/jankurai/repair-queue.jsonl
log "audit lane: evidence schemas are proved by the required lane (cargo nextest run -E 'binary(evidence_schema_contract)')"
if [ "$audit_status" -ne 0 ]; then
  printf '[ci] audit reported findings (exit %s); report and repair queue retained\n' "$audit_status" >&2
  exit "$audit_status"
fi

if [[ -f agent/badge.toml && -f agent/jankurai-badge.svg ]]; then
  log "audit lane: jankurai badge --check"
  grep -q 'jankurai-badge:start' README.md
  test -s agent/jankurai-badge.svg
  test -s agent/jankurai-badge.json
  "$JANKURAI_BIN" badge --check \
    --score agent/baselines/main.repo-score.json \
    --out agent/jankurai-badge.svg \
    --json-out agent/jankurai-badge.json \
    --readme README.md \
    --link agent/jankurai-badge.json \
    --update-readme \
    --label jankurai
fi
