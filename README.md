# jankurai-tools-analyzers

<!-- jankurai-badge:start -->
[![Jankurai score: 94/100](agent/jankurai-badge.svg)](agent/jankurai-badge.json)
<!-- jankurai-badge:end -->

Historical score from the committed [baseline report](agent/baselines/main.repo-score.json)
and [auditor metadata](agent/jankurai-badge.json).

Dimension analyzers and scoring suite for the **jankurai** audit standard. This
repository is one member of the Jankurai split family; read [`SPLIT.md`](SPLIT.md)
for the family contract and [`AGENTS.md`](AGENTS.md) for agent routing rules.

## Contributor setup

This repository supplies native language, web-security and repository-health detectors to
[Jankurai](https://github.com/neverhuman/jankurai). For binary installation,
your first audit, and the complete family build, start at the hub.

Install Rust **1.97.1**, a native compiler/linker and Node.js **24** for CI
control tests. Then run the crate's contributor checks:

```sh
cargo fetch --locked
cargo test --workspace --locked
bash scripts/ci-local.sh required
```

The native library does not need Node.js at runtime. The complete quality lane
also needs the pinned security tools and auditor installed by the owning CI
setup; see [testing](docs/testing.md). Local recipes are in the
[Justfile](Justfile). CI runs on the forge and our own hosts; GitHub is a
publishing mirror only and runs no workflows.

## Layout

| Path | Role |
| --- | --- |
| `crates/jankurai-audit-analyzers` | Rust analyzer + scoring crate |
| `agent/` | machine-readable owner, test, boundary, and proof maps |
| `docs/` | architecture, testing, boundaries, release, and exception docs |
| `ops/` | pinned CI script entrypoints |
| `scripts/` | local CI and fusion helpers |
| `schemas/` | JSON Schemas for the auditor's evidence artifacts |

## Documentation

- [Architecture](docs/architecture.md)
- [Testing](docs/testing.md)
- [Native semantic analysis and input integrity](docs/semantic-analysis.md)
- [Boundaries](docs/boundaries.md)
- [Release process](docs/release.md)
- [Agent exceptions and overrides](docs/exceptions.md)

## Versioning

The current version is recorded in [`VERSION`](VERSION) and the change history in
[`CHANGELOG.md`](CHANGELOG.md). Release mechanics are documented in
[`docs/release.md`](docs/release.md).

## License

See [`LICENSE`](LICENSE).
