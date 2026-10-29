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
if [[ -f "$root/ops/ci/check-bootstrap.sh" ]]; then
  cp "$root/ops/ci/fast.sh" ops/ci/
  reject fast
  grep -F "auditor version mismatch: want $want got 0.0.0" output
fi
export TEST_VERSION="jankurai $want"
bash scripts/ci-local.sh audit
grep -F 'audit .' calls
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
