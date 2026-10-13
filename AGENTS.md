# jankurai-tools-analyzers Agent Instructions

Read `SPLIT.md` first. This repository is one member of the Jankurai split family.

- Canonical GitHub repo: `neverhuman/jankurai-tools-analyzers`.
- Primary remote: `github.com/neverhuman/jankurai-tools-analyzers`.
- Do not add committed cross-repo `path = "../..."` dependencies. Use the hub fusion workspace for local path patches.
- Do not hand-edit generated artifacts listed in `agent/generated-zones.toml`.
- Do not edit CHANGELOG.md in a change: release notes are generated at release time from commit subjects and Todo: trailers.
- Run `bash scripts/ci-local.sh required` before handing off changes.
