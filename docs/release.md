# Release process

This document is the release control surface for jankurai-tools-analyzers. It
covers the version source, the changelog, the release automation, integrity and
SBOM evidence, rollback, and the launch gate. Launch gates require every section
below to be backed by a real artifact or command.

## Version source

The single source of truth for the version is the [`VERSION`](../VERSION) file at
the repository root. The crate version in
`crates/jankurai/Cargo.toml` and any release tag MUST match
`VERSION`. Tags follow the family pattern
`jankurai-tools-analyzers-v<MAJOR.MINOR.PATCH>-split.<N>` as described in
[`SPLIT.md`](../SPLIT.md).

## Changelog

Every release records its user-visible changes in
[`CHANGELOG.md`](../CHANGELOG.md) under a heading that matches the new `VERSION`.
The `Unreleased` section is promoted to a dated version heading at tag time.

## Release automation

Releases are cut by CI, not by hand:

1. Bump [`VERSION`](../VERSION) and promote the `Unreleased` section of
   [`CHANGELOG.md`](../CHANGELOG.md).
2. Run the full local gate: `just check` (format, lint, fast lane, security,
   self-audit).
3. Push the version commit. Forge CI on our own hosts runs the build,
   security, and jankurai audit lanes and keeps the `repo-score` artifacts.
   GitHub is a publishing mirror only; releases are built and signed on our
   own servers, and a separate change introduces key-based release signing.
4. Tag the release commit with `jankurai-tools-analyzers-v<version>-split.<N>`.
   The tag mirror in [`.jeryu/repo.toml`](../.jeryu/repo.toml) publishes the
   immutable tag to the public GitHub mirror with `cargo publish`-equivalent
   release packaging.

Release builds depend on immutable tags, never branches.

## Integrity, provenance, and SBOM

- **Dependency integrity**: builds are reproducible because `Cargo.lock` is
  committed and every CI lane uses `--locked`. Release artifacts carry a
  `sha256` checksum manifest.
- **SBOM**: generate a CycloneDX software bill of materials from the locked
  dependency graph with `cargo cyclonedx --format json` (run in CI alongside the
  security job) and attach it to the release as `sbom.json`.
- **Provenance**: the security job runs `gitleaks detect` for secret scanning and
  `cargo audit` for advisory checks; the audit job publishes the signed
  `repo-score` artifacts with `cosign` attestation that prove the release passed
  the jankurai gate.
- **Action pinning**: every third-party GitHub Action is pinned to a 40-character
  commit SHA so the supply chain of the release pipeline itself is fixed.

## Launch gate

The release gate (launch gate) blocks publishing until every operational control
below is proven:

- **Backups**: immutable, offsite backups exist and a tested restore procedure is
  on file before any destructive release step runs.
- **Monitoring**: error-rate and saturation monitoring dashboards and alerts are
  green for the candidate build.
- **Rollback**: a one-command rollback to the previous immutable tag is verified
  (see the Rollback section below).
- **Security**: the security lane (gitleaks + cargo audit) passes with no high or
  critical findings.
- **Abuse and rate limit**: rate limit and abuse controls are exercised so a bad
  release cannot be weaponized against downstream consumers.

A release that cannot show backup, monitoring, rollback, security, and rate
limit / abuse evidence does not pass the launch gate and is not published.

## Rollback

If a release regresses:

1. Identify the last known-good tag
   (`jankurai-tools-analyzers-v<version>-split.<N>`).
2. Re-point consumers at that immutable tag; tags are never moved or deleted.
3. Restore from backup if data or state changed, using the tested restore
   procedure.
4. Open a revert commit that restores the previous `VERSION` and `CHANGELOG.md`
   state, and add a `### Fixed` entry describing the rollback.
5. Re-run `just check` to confirm the rolled-back tree is green before
   re-publishing.

Because tags are immutable and `Cargo.lock` is committed, any prior release can
be rebuilt bit-for-bit from its tag.
