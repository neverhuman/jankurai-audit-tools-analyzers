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
  `crates/jankurai-audit-analyzers/src/audit/**` behind `#[cfg(test)]`, exercising
  each detector against tempfile fixtures.
- **Integration tests** live under
  `crates/jankurai-audit-analyzers/tests/` and drive the public analyzer API
  (`repo_rot`, `zyal`) end-to-end through a constructed `AuditContext`.
- **Property tests** live under
  `crates/jankurai-audit-analyzers/tests/` and use `proptest` to assert detector
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

## Repair evidence and reruns

Keep the command, exit status, commit (`git rev-parse HEAD`), and tool versions
beside the failing output. The required lane prints `[ci]` phase labels; nextest
names each failed test. Rerun a reported test with
`cargo nextest run --workspace --locked --offline -E 'test(TEST_NAME)'`, replacing
`TEST_NAME` with the reported name. A narrow pass does not replace the required lane.

For structured audit diagnostics, run `bash scripts/ci-local.sh audit` with the
pinned auditor on PATH. It writes `.jankurai/repo-score.json` and
`.jankurai/repo-score.md`; `target/jankurai/auditor.txt` records its version and
SHA-256. Findings carry rule IDs, paths, evidence and repair routes. Preserve the
failed report before rerunning, and compare the same revision and auditor.
The audit lane can exit nonzero after writing reports; check both the exit code
and findings. These artifacts are repair evidence, not a claim that a gate passed.

### Structured evidence artifacts and their contracts

The audit lane publishes three machine-readable artifacts, each with a committed
wire contract, so the next agent routes off typed fields instead of log prose:

| Artifact | What it carries | Contract |
| --- | --- | --- |
| `.jankurai/repo-score.json` | score, caps, decision, per-dimension evidence, every finding with `rule_id`, `path`, `problem`, `agent_fix`, `rerun_command`, `docs_url` and a `fingerprint` | [`schemas/repo-score.schema.json`](../schemas/repo-score.schema.json) |
| `target/jankurai/repair-queue.jsonl` | one JSON object per repair task (`rule_id`, `path`, `priority`, `lane`, `owner`, `task`, `why`) | [`schemas/repair-queue.schema.json`](../schemas/repair-queue.schema.json) |
| `target/jankurai/auditor.txt` | the identity receipt (`version`, `sha256`) of the binary that produced the report | `ops/ci/lib.sh` (`require_auditor`) |

The report names its own contract in `schema_url`, and
`agent/baselines/main.repo-score.json` is the committed report the ratchet
compares against. Both contracts are enforced, not described: the required lane
runs

```sh
cargo nextest run --workspace --locked --offline -E 'binary(evidence_schema_contract)'
```

which validates the committed baseline and the committed 1.7.6 fixture against
the schemas, checks every inline `agent_fix_queue` record against the
repair-queue contract, proves the schemas reject corrupted reports, and fails
closed on any schema keyword its validator does not implement.

The audit lane clears the previous report and queue before it runs, then
`assert_repair_queue` (`ops/ci/lib.sh`) checks the queue. An audit with no
findings is valid: the auditor then writes a queue holding a single newline and
the report's `agent_fix_queue` is empty, so zero records passes. A zero-byte
file, a line that is not a JSON object with string `path`, `priority`, `task` and
`why`, a missing file, or a record count that differs from the report's
`agent_fix_queue` fails the lane. These checks run even when the audit exits
nonzero, and the audit's own exit status is still returned afterwards.
`bash ops/ci/auditor-test.sh` covers each case with a controlled auditor.

To triage a finding: read `rerun_command` and `docs_url` on the finding, rerun
that lane, then diff the new report's `findings[].fingerprint` set against the
retained one. A finding that disappears without its fingerprint being accounted
for means the scope changed, not that the defect was repaired.

## Local verification budget

These checks use local CPU, disk and Git fixtures; they require no paid API.
The spend cap for this procedure is zero and the paid-request quota is zero.
Use one required-lane process per checkout. A reproducible 15-minute wall-clock
budget and explicit stop condition can be applied with GNU `timeout`:

```sh
mkdir -p target/verification
bash -c 'set -o pipefail; timeout --signal=TERM --kill-after=10s 15m bash scripts/ci-local.sh required 2>&1 | tee target/verification/required.log'
python3 scripts/release-notes-test.py
```

The timeout is the kill switch: expiry terminates the lane, and status 124 is
incomplete proof, never a pass. On expiry or failure, retain the log, inspect the
last phase and fix the cause before a deliberate rerun; do not retry indefinitely.
The release-note fixture exercises the real CLI against a temporary Git repository
under `target/`, including release and rollback records, invalid input and a
repeated render. It neither contacts production nor writes release notes here.
Dependency fetching is a separate setup step (`cargo fetch --locked`); the required
Rust checks use the offline locked graph.

### Python classification and reproduction limits

Python in `scripts/`, `ops/`, `tools/`, `db/`, `migrations/` and `contracts/`
continues through the Python-containment checks, including on self-audits. A
runtime-stack classifier is not evidence that Python cannot own durable data.
Non-product exceptions must use the declared boundary mechanism and its review
process; this change grants no directory exemption and changes no score floor.
`python_containment_keeps_operational_and_db_paths_in_scope` covers these paths
with database imports, plus the existing DB-truth deduction.

The historical dependency head `49856cb` was not reproduced by this repair.
No score comparison or clean-proof claim for that head is made here. The
committed ratchet baseline is an older auditor artifact, not a measurement of
this revision under 1.7.6; validating its shape does not refresh that baseline.

### Verified 1.7.6 lane invocation

On 2026-11-01, `PATH=/usr/local/libexec/jain:$PATH bash ops/ci/audit.sh` ran
with the pinned real auditor (SHA-256
`63b445f21ca375ebd9515df49d0095aff6823b731121102651caf4e57cef1cf9`).
It accepted `--repair-queue-jsonl` and wrote the JSON report, Markdown report
and JSONL queue. The lane exited **1**, with score 72, raw 92, and two HLT-005
findings. This is evidence of artifact/CLI compatibility, not a passing audit
or a reproduction of historical PR14. The unchanged 1.7.6 binary does not run
the source changes in this PR.

The exact report, queue and binary receipt are retained in
`crates/jankurai-audit-analyzers/tests/fixtures/audit-1.7.6/`. The required lane
validates these committed artifacts and the older baseline deterministically;
it does not opportunistically read ignored working-tree reports. The schema
validator implements the forms used here and rejects other forms (including
schema-valued `additionalProperties` and `$ref` with constraint siblings).
