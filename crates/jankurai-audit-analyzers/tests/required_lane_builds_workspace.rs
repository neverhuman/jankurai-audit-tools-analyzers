//! The required lane is this repo's push gate, so it must compile, lint and
//! test the workspace rather than only resolving the manifest. A metadata-only
//! lane is a false green: a panicking test or a broken build would pass it.
use std::fs;
use std::path::PathBuf;

fn required_lane() -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../ops/ci/required.sh")
        .canonicalize()
        .expect("ops/ci/required.sh exists");
    fs::read_to_string(path).expect("ops/ci/required.sh is readable")
}

#[test]
fn required_lane_runs_the_build_lint_and_test_commands() {
    let lane = required_lane();
    for command in [
        "cargo metadata --locked --offline --no-deps --format-version 1",
        "cargo fmt --all --check",
        "cargo clippy --workspace --all-targets --locked --offline -- -D warnings",
        "cargo nextest run --workspace --locked --offline",
    ] {
        assert!(
            lane.contains(command),
            "required lane is missing `{command}`:\n{lane}"
        );
    }
}

/// `cargo fmt` reads no dependency graph, so it takes neither flag; every other
/// cargo invocation must resolve from the lockfile without touching the network.
#[test]
fn required_lane_stays_offline_and_locked() {
    let lane = required_lane();
    for line in lane
        .lines()
        .map(str::trim)
        .filter(|line| line.starts_with("cargo ") && !line.starts_with("cargo fmt"))
    {
        assert!(line.contains("--locked"), "cargo line is unlocked: {line}");
        assert!(
            line.contains("--offline"),
            "cargo line needs network: {line}"
        );
    }
}

#[test]
fn required_lane_fails_fast_on_the_first_failing_command() {
    let lane = required_lane();
    assert!(
        lane.contains("set -euo pipefail"),
        "required lane must abort on the first failure:\n{lane}"
    );
}
