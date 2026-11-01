# Contracts

This directory is the contract index for jankurai-tools-analyzers. The analyzer
suite emits several machine-readable evidence artifacts (coverage audit, security
evidence, boundary evidence, migration report). Their wire contracts are the
JSON Schemas under [`schemas/`](../schemas/):

| Artifact | Contract schema |
| --- | --- |
| Coverage audit | [`schemas/coverage-audit.schema.json`](../schemas/coverage-audit.schema.json) |
| Security evidence | [`schemas/security-evidence.schema.json`](../schemas/security-evidence.schema.json) |
| Boundary evidence | [`schemas/boundary-evidence.schema.json`](../schemas/boundary-evidence.schema.json) |
| Migration report | [`schemas/migration-report.schema.json`](../schemas/migration-report.schema.json) |
| Repo-score report (`.jankurai/repo-score.json`) | [`schemas/repo-score.schema.json`](../schemas/repo-score.schema.json) |
| Repair queue (`target/jankurai/repair-queue.jsonl`) | [`schemas/repair-queue.schema.json`](../schemas/repair-queue.schema.json) |

## Contract discipline

- Every analyzer that writes an evidence artifact MUST validate its output
  against the matching schema before the artifact is published.
- The audit-evidence contracts are proved against real reports by
  `cargo nextest run --workspace -E 'binary(evidence_schema_contract)'` in the
  required lane; see
  [`docs/testing.md`](../docs/testing.md#structured-evidence-artifacts-and-their-contracts).
- The schemas are versioned (`schema_version`); a breaking change bumps the
  version and is gated by the public-API / semver drift lane (`just api-drift`).
- These schemas are the single source of truth for consumers of the analyzer
  output (for example `jankurai-core`, which re-exports the analyzers). They play
  the same role an OpenAPI or protobuf contract plays for a service boundary:
  the consumer is generated/validated from the contract, never hand-mirrored.
