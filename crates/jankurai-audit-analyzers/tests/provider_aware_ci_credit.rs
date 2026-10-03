//! The proof, speed and security bonuses that used to read only
//! `.github/workflows/` are provider-aware: a `.jeryu/ci.toml` lane earns them
//! through the text it really runs, never through the declaration itself.
use jankurai_audit_analyzers::audit::analyzers::{proof, security, speed};
use jankurai_audit_kernel::audit::helpers::AuditContext;
use jankurai_audit_kernel::model::{DimensionResult, FileInfo};
use tempfile::{tempdir, TempDir};

const DECLARATION: &str = "\
schema_version = \"2\"
provider = \"jeryu\"

[[lane]]
name = \"required\"
command = \"just required\"
";

const PRODUCT: &[(&str, &str)] = &[
    (
        "Cargo.toml",
        "[package]\nname = \"widgetworks-ledger\"\nversion = \"0.1.0\"\n",
    ),
    ("src/lib.rs", "pub fn balance() -> i64 {\n    0\n}\n"),
];

fn file(path: &str, text: &str) -> FileInfo {
    FileInfo {
        rel_path: path.into(),
        name: path.rsplit('/').next().unwrap().into(),
        suffix: path
            .rsplit_once('.')
            .map_or(String::new(), |(_, ext)| format!(".{ext}")),
        size: text.len() as u64,
        line_count: text.lines().count(),
        text: text.into(),
        is_generated: false,
        is_code: false,
    }
}

fn repo(extra: &[(&str, &str)]) -> (TempDir, AuditContext) {
    let root = tempdir().unwrap();
    let files: Vec<FileInfo> = PRODUCT
        .iter()
        .chain(extra.iter())
        .map(|(path, text)| file(path, text))
        .collect();
    let ctx = AuditContext {
        root: root.path().into(),
        scope_paths: vec![],
        scope_files: files.clone(),
        all_files: files,
        self_audit: false,
        boundary_reclassifications: vec![],
        copy_code: None,
    };
    (root, ctx)
}

fn evidence(dim: &DimensionResult) -> String {
    dim.evidence.join("\n")
}

#[test]
fn proof_ci_presence_is_on_par_for_github_and_jeryu() {
    let justfile = "required:\n    cargo test --workspace\n";
    let (_a, github) = repo(&[
        (
            ".github/workflows/ci.yml",
            "jobs:\n  t:\n    steps:\n      - run: just required\n",
        ),
        ("Justfile", justfile),
    ]);
    let (_b, jeryu) = repo(&[(".jeryu/ci.toml", DECLARATION), ("Justfile", justfile)]);
    let (_c, none) = repo(&[("Justfile", justfile)]);
    let unresolved_justfile = "fast:\n    cargo test --workspace\n";
    let (_d, unresolved) = repo(&[
        (".jeryu/ci.toml", DECLARATION),
        ("Justfile", unresolved_justfile),
    ]);
    let (_e, unresolved_none) = repo(&[("Justfile", unresolved_justfile)]);

    let github_dim = proof::analyze(&github);
    let jeryu_dim = proof::analyze(&jeryu);
    assert!(evidence(&github_dim).contains("GitHub workflow files present"));
    assert!(evidence(&jeryu_dim).contains("jeryu CI lane resolved in .jeryu/ci.toml: required"));
    assert_eq!(github_dim.score, jeryu_dim.score, "same CI-presence credit");
    assert_eq!(jeryu_dim.score - proof::analyze(&none).score, 8);
    // A declaration whose lane does not resolve is not CI presence.
    let unresolved_dim = proof::analyze(&unresolved);
    assert!(!evidence(&unresolved_dim).contains("jeryu CI lane"));
    assert_eq!(unresolved_dim.score, proof::analyze(&unresolved_none).score);
}

#[test]
fn speed_ci_cache_hint_reads_resolved_lane_text() {
    let hint = |dim: &DimensionResult| {
        dim.evidence
            .iter()
            .any(|line| line.starts_with("CI cache hint found"))
    };
    let (_a, github) = repo(&[(
        ".github/workflows/ci.yml",
        "jobs:\n  t:\n    steps:\n      - uses: actions/cache@v4\n",
    )]);
    assert!(hint(&speed::analyze(&github)));

    let (_b, cached) = repo(&[
        (".jeryu/ci.toml", DECLARATION),
        (
            "Justfile",
            "required:\n    RUSTC_WRAPPER=sccache cargo test\n",
        ),
    ]);
    let cached_dim = speed::analyze(&cached);
    assert!(evidence(&cached_dim).contains("CI cache hint found in jeryu lane: sccache"));

    let (_c, comment) = repo(&[
        (".jeryu/ci.toml", DECLARATION),
        (
            "Justfile",
            "required:\n    # the cache is warmed elsewhere\n    cargo test\n",
        ),
    ]);
    let comment_dim = speed::analyze(&comment);
    assert!(
        !hint(&comment_dim),
        "a bare `cache` in a comment does not count"
    );

    // Cache use outside any declared lane earns nothing.
    let (_d, undeclared) = repo(&[(
        "Justfile",
        "required:\n    RUSTC_WRAPPER=sccache cargo test\n",
    )]);
    assert!(!hint(&speed::analyze(&undeclared)));
    assert_eq!(cached_dim.score - speed::analyze(&undeclared).score, 10);
}

const POSTURE_GATE: &str = "\
#!/usr/bin/env bash
set -euo pipefail
cargo audit
gitleaks detect --no-banner
syft . -o spdx-json
jankurai audit . --json target/jankurai/repo-score.json --md target/jankurai/repo-score.md
";

#[test]
fn security_workflow_lint_is_not_applicable_without_workflows() {
    const POSTURE: &str = "complete operational security command posture";
    const LINT: &str = "workflow linting tooling found";
    let base = [
        ("Cargo.lock", "# lock\n"),
        ("tools/security-lane.sh", POSTURE_GATE),
        ("Justfile", "security:\n    bash tools/security-lane.sh\n"),
    ];
    let with = |extra: &[(&'static str, &'static str)]| {
        let mut pairs = base.to_vec();
        pairs.extend_from_slice(extra);
        repo(&pairs)
    };

    let (_a, jeryu) = with(&[(
        ".jeryu/ci.toml",
        "schema_version = \"2\"\nprovider = \"jeryu\"\n\n[[lane]]\nname = \"security\"\ncommand = \"just security\"\n",
    )]);
    let text = evidence(&security::analyze(&jeryu));
    assert!(!text.contains(LINT), "no free lint bonus");
    assert!(text.contains("workflow-linting check does not apply"));
    assert!(text.contains(POSTURE), "{text}");

    let (_b, github) = with(&[(
        ".github/workflows/ci.yml",
        "jobs:\n  t:\n    steps:\n      - run: bash tools/security-lane.sh\n",
    )]);
    let text = evidence(&security::analyze(&github));
    assert!(!text.contains(LINT));
    assert!(!text.contains(POSTURE), "workflows present but unlinted");
    assert!(!text.contains("does not apply"));

    // A lint tool in the lane text satisfies it.
    let (_c, linted) = with(&[
        (
            ".jeryu/ci.toml",
            "schema_version = \"2\"\nprovider = \"jeryu\"\n\n[[lane]]\nname = \"lint\"\ncommand = \"bash scripts/lint.sh\"\n",
        ),
        ("scripts/lint.sh", "actionlint\n"),
    ]);
    assert!(evidence(&security::analyze(&linted)).contains(LINT));
}
