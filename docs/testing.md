# Testing

This document is the testing and verification surface for
jankurai-tools-analyzers. It describes the deterministic proof lanes, the Rust
test strategy for the analyzer crate, and the agent-friendly exception pattern
that governs every audit override.

## Proof lanes

Every change routes to the smallest deterministic lane in
[`agent/proof-lanes.toml`](../agent/proof-lanes.toml). The lanes are exposed both
locally (the root [`Justfile`](../Justfile)) and in forge CI on our own hosts
through the shared
`ops/ci/<lane>.sh` scripts, so a green local run means a green CI run.

| Lane | Command | Purpose |
| --- | --- | --- |
| `required` | `bash scripts/ci-local.sh required` | locked metadata + `cargo fmt --all --check` + `cargo clippy --workspace --all-targets` + `cargo nextest run --workspace`, all offline |
| `fast` | `just fast` | `cargo check --workspace --locked` + `cargo nextest run --workspace` |
| `security` | `just security` | `gitleaks detect` + `cargo audit` |
| `audit` | `just audit` | `jankurai audit` writes `.jankurai/repo-score.{json,md}` |

## Rust test strategy

The analyzer crate carries three layers of Rust proof:

- **Unit tests** live next to the analyzers under
  `crates/jankurai/src/audit/**` behind `#[cfg(test)]`, exercising
  each detector against tempfile fixtures.
- **Integration tests** live under
  `crates/jankurai/tests/` and drive the public analyzer API
  (`repo_rot`, `zyal`) end-to-end through a constructed `AuditContext`.
- **Property tests** live under
  `crates/jankurai/tests/` and use `proptest` to assert detector
  invariants over randomized inputs (clean source never flags; the detector
  vocabulary is matched deterministically).

Run the whole suite with `cargo test --workspace --locked` (the route declared in
[`agent/test-map.json`](../agent/test-map.json) for `crates/`).

## Coverage and false-green defense

Coverage and mutation evidence is parsed deterministically from the artifacts
declared in [`agent/coverage-sources.toml`](../agent/coverage-sources.toml). The
HLT-008 false-green check requires both property and integration Rust tests on
the analyzer surface; both are present under the crate's `tests/` directory.

LCOV input must contain complete `SF` / `end_of_record` sections and valid
line/branch counts. Declared line and branch totals must match their records;
truncation, duplicate records within a section, unknown records, and invalid
counts produce parser findings. Required sources retain a blocking finding.
Repeated complete sections for different tests merge their hits and preserve
all instrumented lines. Artifact reads are bounded and require nonempty regular
UTF-8 files. Input-discovery errors cannot disable an automatic source.

The parser follows the [LCOV tracefile format](https://github.com/linux-test-project/lcov/blob/master/docs/man/geninfo.rst).
Function/version metadata is diagnostic; it does not establish line or branch
coverage. Unsupported extensions require an explicit parser change. These
checks validate imported data structure; they do not authenticate execution,
producer identity or freshness. `coverage_input_integrity.rs` retains the
malformed, truncated, duplicate-section and required-source refusal controls.

Generic JSON summaries require a string `status`, an object `metrics`, and an
array `findings`. Supported outcomes are `pass`, `warn`, `fail`, `error`,
`missing`, `cancelled`, `timeout` and `incomplete`. A declared unsuccessful
outcome remains a finding even if its finding list is empty. Each finding
requires a nonempty `repair` (or `fix`); malformed findings and duplicate
status or finding fields fail parsing. Required and strict sources retain
blocking outcomes; advisory sources retain warnings. De-duplication and display
limits cannot discard the strongest failure. `coverage_summary_integrity.rs`
exercises these rules through the complete coverage audit entry point.

The cargo-mutants importer reads the explicit `outcomes` list, recognizes
native `CaughtMutant`/`MissedMutant` results, and excludes a successful baseline
from mutation counts. It also accepts the existing explicit legacy
`caught`/`missed` and `killed`/`survived` records. Missing or unknown outcomes,
failed baselines, timeouts, check-only `Success` results, conflicting source
locations, and declared totals inconsistent with the records are incomplete
inputs. They cannot become clean mutation coverage. The wire format follows
[cargo-mutants 25.3.1](https://github.com/sourcefrog/cargo-mutants/blob/v25.3.1/src/outcome.rs);
`coverage_mutation_integrity.rs` covers native and legacy parsing and refusal.

Stryker must provide a supported schema version and an explicit file/mutant
inventory. Duplicate files or mutant IDs, unknown states, pending mutations,
runtime errors and timeouts are incomplete inputs. Trivy requires scanned
targets and structurally valid vulnerabilities with assessed severities;
missing results and unknown severities cannot become zero vulnerabilities.
Hadolint accepts an empty diagnostic list, but every present diagnostic must
have a valid location, level, code and message. `coverage_scanner_integrity.rs`
retains positive and malformed-input controls for these formats.

Coverage source globs are validated before selecting evidence. Display limits
apply after all input findings have been classified and sorted, so a blocking
diagnostic late in a report cannot disappear behind earlier informational ones.

## Agent-friendly exception pattern

Exceptions are the only sanctioned way to deviate from the audit baseline. They
are data, not prose, and every exception entry is reviewed on each audit. An
exception record carries these fields so an agent can act on it without guessing:

- **purpose** — what the exception buys and why the default rule does not fit.
- **reason** — the concrete justification, e.g. "this crate is the dead-language
  detector, so its term lists are load-bearing data, not dead code."
- **owner** — the team or person accountable for removing it.
- **expires** — an ISO date after which the exception is invalid and the audit
  fails again.
- **common fixes** — the repair_hint and docs_url an agent should follow first:
  see [`docs/exceptions.md`](exceptions.md) (`docs_url`) and the `repair_hint`
  recorded next to each override.

The full exception lifecycle is documented in
[`docs/exceptions.md`](exceptions.md).
