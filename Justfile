# jankurai-tools-analyzers root command surface.
# One-command setup and validation lanes for agents and CI.
# Every lane below is deterministic, hermetic, and runnable from the repo root.

# Default: list available lanes.
default:
    @just --list

# One-command bootstrap: install the toolchain components this repo needs.
setup:
    rustup component add rustfmt clippy
    cargo fetch --locked

# Alias for setup so `just install` and `just bootstrap` also resolve.
install: setup

bootstrap: setup

# Deterministic fast lane: the narrowest proof loop for agent iteration.
fast:
    cargo check --workspace --locked
    cargo nextest run --workspace

# Run the full local check: format, lint, fast lane, security, and audit.
check: fmt lint fast security audit

# Verify is an alias of check for agents that look for a `verify` lane.
verify: check

fmt:
    cargo fmt --all --check

lint:
    cargo clippy --workspace --all-targets --locked -- -D warnings

# Run the workspace test suite.
test:
    cargo test --workspace --locked

# Narrow per-package proof loop: the fastest agent-iteration target. Pairs the
# nextest cache with a single-package check/test so reruns stay incremental.
fast-pkg:
    cargo check -p jankurai-audit-analyzers --locked
    cargo nextest run -p jankurai-audit-analyzers

# Security lane: secret scanning plus dependency vulnerability scanning.
# gitleaks scans for committed secrets; cargo audit checks the Rust dependency
# tree; the canonical wrapper adds SBOM/provenance and workflow hardening.
security:
    gitleaks detect --source . --no-banner --redact
    cargo audit
    npm audit --omit=dev
    bash tools/security-lane.sh

# Public-API / semver drift check: this crate is a published library, so the
# public surface must not break consumers (jankurai-core) silently.
api-drift:
    cargo public-api --deny changed
    cargo semver-checks check-release

# Jankurai self-audit lane: writes the repo-score artifacts that CI uploads.
audit:
    jankurai audit . --no-score-history --json .jankurai/repo-score.json --md .jankurai/repo-score.md

# Print the declared version.
versions:
    cat VERSION

# Full existing required proof, also used by the hosted gate runner.
required:
    bash scripts/ci-local.sh required
