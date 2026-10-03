#!/usr/bin/env bash
# Execute scanners once and report their actual exit status to the auditor.
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/../.."
mkdir -p target/jankurai/security
scan() {
  local tool="$1"; shift
  local status=ran result=0
  "$@" || { result=$?; status=failed; }
  jq -cn --arg tool "$tool" --arg command "$*" --arg status "$status" --argjson result "$result" \
    '{label:$tool,tool:$tool,shell_command:$command,status:$status,exit_code:$result,advisory:false}' \
    | sed 's/^/jankurai-security-step=/'
  return "$result"
}
scan gitleaks gitleaks detect --source . --no-banner --redact
if [[ -f Cargo.toml ]]; then
  scan cargo-audit cargo audit
  scan cargo-deny cargo deny check advisories bans sources
fi
if [[ -f package-lock.json ]]; then scan npm npm audit --audit-level=high; fi
scan syft syft scan dir:. --exclude './target/**' --exclude './.git/**' --exclude './node_modules/**' \
  -o cyclonedx-json=target/jankurai/security/sbom.json
scan grype grype sbom:target/jankurai/security/sbom.json --fail-on high
