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
printf '{}\n' > .jankurai/repo-score.json
printf 'score\n' > .jankurai/repo-score.md
# Mirror the real auditor: write the repair queue to the path the lane asks
# for. TEST_SKIP_QUEUE=1 plays an auditor that promises the queue and does not
# write it, so the lane's artifact assertion stays load-bearing.
queue=''
while [[ $# -gt 0 ]]; do
  if [[ "$1" == --repair-queue-jsonl ]]; then
    queue="${2:-}"
  fi
  shift
done
if [[ -n "$queue" && "${TEST_SKIP_QUEUE:-0}" != 1 ]]; then
  mkdir -p "$(dirname "$queue")"
  printf '{"path":"README.md","priority":"medium","task":"t","why":"w"}\n' > "$queue"
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
# A missing repair queue is a missing promise, not a pass.
rm calls target/jankurai/repair-queue.jsonl
export TEST_SKIP_QUEUE=1
if bash scripts/ci-local.sh audit > output 2>&1; then
  echo 'missing repair queue was accepted' >&2
  exit 1
fi
grep -F 'expected artifact missing: target/jankurai/repair-queue.jsonl' output
unset TEST_SKIP_QUEUE
rm calls
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
