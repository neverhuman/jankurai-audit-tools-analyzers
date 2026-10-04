#!/usr/bin/env bash
# Required lane: the proof that must pass on every push. Resolves the locked
# dependency graph and then builds, lints and tests the workspace, so a compile
# or test failure cannot reach a green gate. Mirrors jankurai-core's required
# lane; every command is offline so the lane needs no network after
# `cargo fetch --locked`.
set -euo pipefail
source "$(dirname "${BASH_SOURCE[0]}")/lib.sh"
cd "$REPO_ROOT"

log "required lane: locked metadata, format, lint, and workspace tests"
cargo metadata --locked --offline --no-deps --format-version 1 >/dev/null
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked --offline -- -D warnings
cargo nextest run --workspace --locked --offline
