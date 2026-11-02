#!/usr/bin/env bash
# Exercise the real dispatcher and lanes with controlled auditor executables.
set -euo pipefail
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
mkdir -p "$root/target"
fixture="$(mktemp -d "$root/target/auditor-test.XXXXXX")"
trap 'rm -rf -- "$fixture"' EXIT
mkdir -p "$fixture/ops/ci" "$fixture/scripts" "$fixture/agent" "$fixture/bin"
cp "$root/ops/ci/"{lib,audit}.sh "$fixture/ops/ci/"
cp "$root/scripts/ci-local.sh" "$fixture/scripts/"
cp "$root/agent/standard-version.toml" "$fixture/agent/"
cat > "$fixture/bin/jankurai" <<'SH'
#!/usr/bin/env bash
set -euo pipefail
if [[ "$1" == --version ]]; then
  printf '%s\n' "$TEST_VERSION"
  exit "${TEST_VERSION_EXIT:-0}"
fi
printf '%s\n' "$*" >> calls
mkdir -p .jankurai
rec='{"path":"README.md","priority":"medium","task":"t","why":"w"}'
# TEST_QUEUE picks what the stub plays: populated (default), clean (the real
# auditor's output for zero findings: one newline), zero (zero bytes), garbage,
# badrecord, stale (queue disagrees with the report) or skip (never written).
mode="${TEST_QUEUE:-populated}"
if [[ "$mode" == clean ]]; then
  printf '{"agent_fix_queue":[]}\n' > .jankurai/repo-score.json
else
  printf '{"agent_fix_queue":[%s]}\n' "$rec" > .jankurai/repo-score.json
fi
printf 'score\n' > .jankurai/repo-score.md
queue=''
while [[ $# -gt 0 ]]; do
  if [[ "$1" == --repair-queue-jsonl ]]; then
    queue="${2:-}"
  fi
  shift
done
if [[ -n "$queue" && "$mode" != skip ]]; then
  mkdir -p "$(dirname "$queue")"
  case "$mode" in
    populated) printf '%s\n' "$rec" > "$queue" ;;
    clean) printf '\n' > "$queue" ;;
    zero) : > "$queue" ;;
    garbage) printf '{"path":\n' > "$queue" ;;
    badrecord) printf '{"path":"README.md"}\n' > "$queue" ;;
    stale) printf '%s\n%s\n' "$rec" "$rec" > "$queue" ;;
  esac
fi
exit "${TEST_AUDIT_EXIT:-0}"
SH
chmod +x "$fixture/bin/jankurai"
export PATH="$fixture/bin:$PATH"
# UX permits REPO_ROOT overrides for its bootstrap tests.
export REPO_ROOT="$fixture"
cd "$fixture"
reject() {
  if bash scripts/ci-local.sh "${1:-audit}" > output 2>&1; then
    echo 'unexpected audit success' >&2
    exit 1
  fi
  test ! -e calls
  test ! -e target/jankurai/auditor.txt
}
want="$(sed -n 's/^auditor_version = "\([^"]*\)"$/\1/p' agent/standard-version.toml)"
export TEST_VERSION='jankurai 0.0.0'
reject
grep -F "auditor version mismatch: want $want got 0.0.0" output
# Every fast lane that resolves the auditor must refuse an ambient one too.
if grep -qx require_auditor "$root/ops/ci/fast.sh"; then
  cp "$root/ops/ci/fast.sh" ops/ci/
  reject fast
  grep -F "auditor version mismatch: want $want got 0.0.0" output
fi
export TEST_VERSION="jankurai $want"
bash scripts/ci-local.sh audit
grep -F 'audit .' calls
grep -F -- '--repair-queue-jsonl target/jankurai/repair-queue.jsonl' calls
test -s target/jankurai/repair-queue.jsonl
# A clean audit has zero findings: the real auditor writes one newline and the
# report lists no fixes. That is a valid, empty queue.
rm calls target/jankurai/repair-queue.jsonl
TEST_QUEUE=clean bash scripts/ci-local.sh audit
test "$(wc -c < target/jankurai/repair-queue.jsonl)" -eq 1
# Everything else that is not a faithful queue is a failed promise, and a
# previous run's queue must not satisfy the assertion.
queue_rejected() {
  if TEST_QUEUE="$1" bash scripts/ci-local.sh audit > output 2>&1; then
    echo "repair queue mode $1 was accepted" >&2
    exit 1
  fi
  grep -F -- "$2" output
}
queue_rejected skip 'expected artifact missing: target/jankurai/repair-queue.jsonl'
queue_rejected zero 'repair queue is empty'
queue_rejected garbage 'repair queue is malformed'
queue_rejected badrecord 'repair queue is malformed'
queue_rejected stale 'repair queue is stale or partial'
# Rejection also holds when the audit itself failed: the failure is not masked
# into success, and a bad queue is not excused by it.
if TEST_AUDIT_EXIT=1 TEST_QUEUE=zero bash scripts/ci-local.sh audit > output 2>&1; then
  echo 'bad queue accepted after audit failure' >&2
  exit 1
fi
rm -f calls
bash scripts/ci-local.sh audit
grep -Fx "version: jankurai $want" target/jankurai/auditor.txt
digest="$(sha256sum bin/jankurai)"
grep -Fx "sha256: ${digest%% *}" target/jankurai/auditor.txt
# A failed rerun cannot leave the prior identity receipt looking current.
rm calls
export TEST_VERSION_EXIT=7
reject
unset TEST_VERSION_EXIT
export TEST_VERSION="other $want"
reject
mv agent/standard-version.toml agent/pin.saved
printf 'standard_version = "0.9.0"\n' > agent/standard-version.toml
reject
grep -F 'missing or invalid auditor_version pin' output
mv agent/pin.saved agent/standard-version.toml
export TEST_VERSION="jankurai $want"
export TEST_AUDIT_EXIT=9
if bash scripts/ci-local.sh audit > output 2>&1; then
  echo 'audit failure was swallowed' >&2
  exit 1
fi
test -e calls
printf 'auditor identity tests passed\n'
