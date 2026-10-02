//! Tool adoption credits a tool once its adopted command is present in CI text,
//! and credits artifact verification once the upload names every artifact path.
//! This is the governed split.5 crediting, restored for 1.7.2.
use jankurai_audit_analyzers::audit::analyzers::tool_adoption;
use jankurai_audit_kernel::audit::helpers::{AuditContext, TOOL_ADOPTION_CATALOG};
use jankurai_audit_kernel::model::FileInfo;
use std::fs;
use tempfile::{tempdir, TempDir};

const COMMAND: &str =
    "jankurai ux audit --config agent/ux-qa.toml --out target/jankurai/ux-qa.json";

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

fn fixture(mut files: Vec<FileInfo>) -> (TempDir, AuditContext) {
    let root = tempdir().unwrap();
    fs::create_dir(root.path().join("agent")).unwrap();
    let mut policy = "schema_version = \"1.0.0\"\n".to_string();
    for entry in TOOL_ADOPTION_CATALOG {
        let mode = if entry.id == "ux-qa" {
            "required"
        } else {
            "disabled"
        };
        policy += &format!("\n[[tools]]\nid = {:?}\nmode = {mode:?}\n", entry.id);
    }
    fs::write(root.path().join("agent/tool-adoption.toml"), policy).unwrap();
    files.push(file("AGENTS.md", "Read the repository contract."));
    let ctx = AuditContext {
        root: root.path().into(),
        scope_paths: files.iter().map(|f| f.rel_path.clone()).collect(),
        scope_files: files.clone(),
        all_files: files,
        self_audit: false,
        boundary_reclassifications: vec![],
        copy_code: None,
    };
    (root, ctx)
}

fn workflow(steps: &str) -> FileInfo {
    file(
        ".github/workflows/ci.yml",
        &format!(
            "name: ci\non: [push]\njobs:\n  ux:\n    runs-on: ubuntu-latest\n    steps:\n{steps}"
        ),
    )
}

#[test]
fn missing_ci_command_earns_only_configuration_credit() {
    let (_root, ctx) = fixture(vec![workflow("      - run: echo hello\n")]);
    let readiness = tool_adoption::status(&ctx);
    let ux = readiness.items.iter().find(|i| i.id == "ux-qa").unwrap();
    assert_eq!(ux.status, "configured");
    assert_eq!(readiness.replaced_count, 0);
    assert_eq!(tool_adoption::analyze(&ctx).score, 30);
    assert_eq!(
        tool_adoption::missing_required_ci_tools(&ctx),
        vec!["ux-qa"]
    );
}

#[test]
fn ci_command_counts_as_replacement() {
    let (_root, ctx) = fixture(vec![workflow(&format!("      - run: {COMMAND}\n"))]);
    let readiness = tool_adoption::status(&ctx);
    let ux = readiness.items.iter().find(|i| i.id == "ux-qa").unwrap();
    assert_eq!(ux.status, "ci_evidence");
    assert_eq!(readiness.replaced_count, 1);
    assert_eq!(readiness.artifact_verified_count, 0);
    assert_eq!(tool_adoption::analyze(&ctx).score, 90);
    // Required tools still need the artifact upload.
    assert_eq!(
        tool_adoption::missing_required_ci_tools(&ctx),
        vec!["ux-qa"]
    );
}

#[test]
fn ci_command_with_artifact_upload_is_artifact_verified() {
    let (_root, ctx) = fixture(vec![workflow(&format!(
        "      - run: {COMMAND}\n      - uses: actions/upload-artifact@v4\n        with:\n          path: target/jankurai/ux-qa.json\n"
    ))]);
    let readiness = tool_adoption::status(&ctx);
    let ux = readiness.items.iter().find(|i| i.id == "ux-qa").unwrap();
    assert_eq!(ux.status, "artifact_verified");
    assert_eq!(readiness.replaced_count, 1);
    assert_eq!(readiness.artifact_verified_count, 1);
    assert_eq!(tool_adoption::analyze(&ctx).score, 100);
    assert!(tool_adoption::missing_required_ci_tools(&ctx).is_empty());
}

#[test]
fn ci_lane_script_counts_as_ci_text() {
    let (_root, ctx) = fixture(vec![
        workflow("      - run: bash ops/ci/ux.sh\n"),
        file("ops/ci/ux.sh", &format!("#!/usr/bin/env bash\n{COMMAND}\n")),
    ]);
    let readiness = tool_adoption::status(&ctx);
    assert_eq!(readiness.replaced_count, 1);
}
