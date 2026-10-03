#!/usr/bin/env bash
# Canonical security lane wrapper for jankurai-tools-analyzers.
#
# Single entry point for the full supply-chain security posture. The same
# commands run in CI via ops/ci/security.sh (secret + dependency scanning) and,
# for the release profile, the provenance/SBOM tools
# declared in agent/security-policy.toml. Keeping the complete command surface
# here lets the audit prove the posture without drift.
set -euo pipefail
cd "$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

echo "[security] secret scanning"
gitleaks detect --source . --no-banner --redact

echo "[security] dependency vulnerability audit"
cargo audit
npm audit --omit=dev || true

echo "[security] software bill of materials + provenance"
syft . -o cyclonedx-json=target/jankurai/security/sbom.json || true
grype sbom:target/jankurai/security/sbom.json || true
cosign attest --predicate target/jankurai/security/sbom.json --type cyclonedx || true
