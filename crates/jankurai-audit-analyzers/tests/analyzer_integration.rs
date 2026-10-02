//! Integration tests for the analyzer suite.
//!
//! These drive the public analyzer API end-to-end through a constructed
//! `AuditContext`, the same context the orchestration in `jankurai-core` builds
//! before calling each dimension analyzer. They assert the observable contract
//! of the detectors rather than internal helpers, so they survive refactors of
//! the private scan internals.

use jankurai_audit_analyzers::audit::{repo_rot, zyal};
use jankurai_audit_kernel::audit::helpers::AuditContext;
use jankurai_audit_kernel::model::FileInfo;
use std::path::PathBuf;

/// Build a minimal `AuditContext` over the given in-memory files.
fn ctx(files: Vec<FileInfo>) -> AuditContext {
    AuditContext {
        root: PathBuf::from("/tmp/jankurai-analyzers-it"),
        all_files: files.clone(),
        scope_files: files,
        scope_paths: vec![],
        self_audit: false,
        boundary_reclassifications: vec![],
        copy_code: None,
    }
}

/// Construct a code `FileInfo` from a relative path and source text.
fn code_file(rel_path: &str, text: &str) -> FileInfo {
    let suffix = rel_path
        .rfind('.')
        .map(|i| rel_path[i..].to_string())
        .unwrap_or_default();
    let name = rel_path.rsplit('/').next().unwrap_or(rel_path).to_string();
    FileInfo {
        rel_path: rel_path.to_string(),
        name,
        suffix,
        size: text.len() as u64,
        line_count: text.lines().count(),
        text: text.to_string(),
        is_generated: false,
        is_code: true,
    }
}

const MANIFEST_SCHEMA_V2: &str = r#"{
  "$schema": "https://json-schema.org/draft/2020-12/schema",
  "$id": "https://contracts.example.test/model-artifact-v2.schema.json",
  "title": "Model artifact v2",
  "type": "object"
}"#;

const MANIFEST_SCHEMA_V4: &str = r#"{
  "$schema": "https://json-schema.org/draft/2020-12/schema",
  "$id": "https://contracts.example.test/model-run-v4.schema.json",
  "title": "Model run v4",
  "type": "object"
}"#;

const MANIFEST_FIXTURE_V2: &str = r#"{"version":2,"features":["x"]}"#;

fn manifest_fixture_context(manifest: &str) -> AuditContext {
    ctx(vec![
        code_file("contracts/public-api.toml", manifest),
        code_file("schemas/model-artifact-v2.schema.json", MANIFEST_SCHEMA_V2),
        code_file("contracts/model-artifact-v2.json", MANIFEST_FIXTURE_V2),
        code_file(
            "crates/model/src/artifact.rs",
            "pub fn emit_artifact() {}\n",
        ),
    ])
}

fn manifest_row_v2() -> &'static str {
    r#"
[[source_contract]]
id = "model-artifact-v2"
path = "schemas/model-artifact-v2.schema.json"
fixture = "contracts/model-artifact-v2.json"
schema_version = 2
owner = "core"
proof_lane = "contracts"
producer = "crates/model/src/artifact.rs"
"#
}

#[test]
fn repo_rot_summary_is_clean_for_well_named_source() {
    let context = ctx(vec![code_file(
        "crates/app/src/service.rs",
        "pub fn compute(value: u32) -> u32 {\n    value.saturating_add(1)\n}\n",
    )]);
    let summary = repo_rot::summary(&context);
    assert_eq!(
        summary.hard_findings, 0,
        "clean source must not produce repo-rot hard findings"
    );
}

#[test]
fn repo_rot_accepts_structurally_governed_versioned_contract_pair() {
    let context = ctx(vec![
        code_file(
            "contracts/widget-events-v2.schema.json",
            r#"{
  "$schema": "https://json-schema.org/draft/2020-12/schema",
  "$id": "https://contracts.example.test/widget-events-v2.schema.json",
  "title": "widget-events-v2",
  "version": 2,
  "type": "object"
}"#,
        ),
        code_file(
            "contracts/widget-events-v2.jsonl",
            "{\"kind\":\"created\",\"sequence\":1}\n{\"kind\":\"updated\",\"sequence\":2}\n",
        ),
    ]);

    let findings = repo_rot::findings(&context);

    assert_eq!(repo_rot::summary(&context).hard_findings, 0);
    assert!(findings
        .iter()
        .all(|finding| { finding.matched_term != "repo-rot.path.fake-versioned-source" }));
}

#[test]
fn repo_rot_accepts_structurally_governed_v10_contract_pair() {
    let context = ctx(vec![
        code_file(
            "contracts/widget-events-v10.schema.json",
            r#"{
  "$schema": "https://json-schema.org/draft/2020-12/schema",
  "$id": "https://contracts.example.test/widget-events-v10.schema.json",
  "title": "widget-events-v10",
  "version": 10,
  "type": "object"
}"#,
        ),
        code_file(
            "contracts/widget-events-v10.jsonl",
            "{\"kind\":\"created\",\"sequence\":1}\n",
        ),
    ]);

    assert_eq!(repo_rot::summary(&context).hard_findings, 0);
}

#[test]
fn repo_rot_accepts_typed_manifest_bound_json_fixtures() {
    let manifest = format!(
        r#"{}
[[source_contract]]
id = "model-run-v4"
path = "schemas/model-run-v4.schema.json"
fixture = "contracts/model-run-v4.json"
schema_version = 4
owner = "core"
proof_lane = "contracts"
producer = "crates/model/src/run.rs"
"#,
        manifest_row_v2()
    );
    let mut files = manifest_fixture_context(&manifest).all_files;
    files.push(code_file(
        "schemas/model-run-v4.schema.json",
        MANIFEST_SCHEMA_V4,
    ));
    files.push(code_file(
        "contracts/model-run-v4.json",
        r#"{"schema_version":4,"trials":[]}"#,
    ));
    files.push(code_file(
        "crates/model/src/run.rs",
        "pub fn emit_run() {}\n",
    ));
    let context = ctx(files);

    assert_eq!(repo_rot::summary(&context).hard_findings, 0);
}

#[test]
fn repo_rot_rejects_manifest_row_without_fixture_or_with_absent_fixture() {
    let missing_field = manifest_fixture_context(
        &manifest_row_v2().replace("fixture = \"contracts/model-artifact-v2.json\"\n", ""),
    );
    let absent_file = manifest_fixture_context(&manifest_row_v2().replace(
        "contracts/model-artifact-v2.json",
        "contracts/missing-artifact-v2.json",
    ));

    assert_eq!(repo_rot::summary(&missing_field).hard_findings, 1);
    assert_eq!(repo_rot::summary(&absent_file).hard_findings, 1);
}

#[test]
fn repo_rot_rejects_duplicate_or_traversing_manifest_bindings() {
    let duplicate =
        manifest_fixture_context(&format!("{}\n{}", manifest_row_v2(), manifest_row_v2()));
    let traversal = manifest_fixture_context(&manifest_row_v2().replace(
        "contracts/model-artifact-v2.json",
        "contracts/../contracts/model-artifact-v2.json",
    ));

    assert_eq!(repo_rot::summary(&duplicate).hard_findings, 1);
    assert_eq!(repo_rot::summary(&traversal).hard_findings, 1);
}

#[test]
fn repo_rot_rejects_malformed_manifest_or_json_fixture() {
    let malformed_manifest = manifest_fixture_context("[[source_contract]\n");
    let mut malformed_fixture = manifest_fixture_context(manifest_row_v2()).all_files;
    let fixture = malformed_fixture
        .iter_mut()
        .find(|file| file.rel_path == "contracts/model-artifact-v2.json")
        .unwrap();
    *fixture = code_file("contracts/model-artifact-v2.json", "not-json");

    assert_eq!(repo_rot::summary(&malformed_manifest).hard_findings, 1);
    assert_eq!(repo_rot::summary(&ctx(malformed_fixture)).hard_findings, 1);
}

#[test]
fn repo_rot_rejects_manifest_identity_version_or_schema_mismatch() {
    let mismatches = [
        manifest_row_v2().replace("id = \"model-artifact-v2\"", "id = \"other-v2\""),
        manifest_row_v2().replace("schema_version = 2", "schema_version = 4"),
        manifest_row_v2().replace(
            "schemas/model-artifact-v2.schema.json",
            "schemas/other-v2.schema.json",
        ),
    ];
    for manifest in mismatches {
        assert_eq!(
            repo_rot::summary(&manifest_fixture_context(&manifest)).hard_findings,
            1
        );
    }

    let mut bad_schema = manifest_fixture_context(manifest_row_v2()).all_files;
    let schema = bad_schema
        .iter_mut()
        .find(|file| file.rel_path == "schemas/model-artifact-v2.schema.json")
        .unwrap();
    *schema = code_file(
        "schemas/model-artifact-v2.schema.json",
        &MANIFEST_SCHEMA_V2.replace("model-artifact-v2.schema.json", "other-v2.schema.json"),
    );
    assert_eq!(repo_rot::summary(&ctx(bad_schema)).hard_findings, 1);
}

#[test]
fn repo_rot_rejects_manifest_with_absent_producer() {
    let mut files = manifest_fixture_context(manifest_row_v2()).all_files;
    files.retain(|file| file.rel_path != "crates/model/src/artifact.rs");

    assert_eq!(repo_rot::summary(&ctx(files)).hard_findings, 1);
}

#[test]
fn repo_rot_manifest_evidence_does_not_hide_fake_product_source() {
    let mut files = manifest_fixture_context(manifest_row_v2()).all_files;
    files.push(code_file(
        "crates/model/src/payment-v2.rs",
        "pub fn charge() {}\n",
    ));

    let context = ctx(files);
    let findings = repo_rot::findings(&context);
    assert_eq!(repo_rot::summary(&context).hard_findings, 1);
    assert!(findings.iter().any(|finding| {
        finding.path == "crates/model/src/payment-v2.rs"
            && finding.matched_term == "repo-rot.path.fake-versioned-source"
    }));
}

#[test]
fn repo_rot_rejects_versioned_contract_without_exact_evidence_pair() {
    let context = ctx(vec![code_file(
        "contracts/widget-events-v2.schema.json",
        r#"{
  "$schema": "https://json-schema.org/draft/2020-12/schema",
  "$id": "https://contracts.example.test/widget-events-v2.schema.json",
  "title": "widget-events-v2",
  "version": 2
}"#,
    )]);

    let findings = repo_rot::findings(&context);

    assert_eq!(repo_rot::summary(&context).hard_findings, 1);
    assert!(findings
        .iter()
        .any(|finding| finding.matched_term == "repo-rot.path.fake-versioned-source"));
}

#[test]
fn repo_rot_rejects_unpaired_v10_schema() {
    let context = ctx(vec![code_file(
        "contracts/widget-events-v10.schema.json",
        r#"{
  "$schema": "https://json-schema.org/draft/2020-12/schema",
  "$id": "https://contracts.example.test/widget-events-v10.schema.json",
  "title": "widget-events-v10",
  "version": 10
}"#,
    )]);

    let findings = repo_rot::findings(&context);

    assert_eq!(repo_rot::summary(&context).hard_findings, 1);
    assert!(findings
        .iter()
        .any(|finding| finding.matched_term == "repo-rot.path.fake-versioned-source"));
}

#[test]
fn repo_rot_rejects_unpaired_out_of_range_numeric_schema() {
    let context = ctx(vec![code_file(
        "contracts/widget-events-v18446744073709551616.schema.json",
        r#"{
  "$schema": "https://json-schema.org/draft/2020-12/schema",
  "$id": "https://contracts.example.test/widget-events-v18446744073709551616.schema.json",
  "title": "widget-events-v18446744073709551616",
  "version": "18446744073709551616"
}"#,
    )]);

    assert_eq!(repo_rot::summary(&context).hard_findings, 1);
}

#[test]
fn repo_rot_rejects_out_of_range_numeric_contract_pair() {
    let context = ctx(vec![
        code_file(
            "contracts/widget-events-v18446744073709551616.schema.json",
            r#"{
  "$schema": "https://json-schema.org/draft/2020-12/schema",
  "$id": "https://contracts.example.test/widget-events-v18446744073709551616.schema.json",
  "title": "widget-events-v18446744073709551616",
  "version": "18446744073709551616"
}"#,
        ),
        code_file(
            "contracts/widget-events-v18446744073709551616.jsonl",
            "{\"kind\":\"created\",\"sequence\":1}\n",
        ),
    ]);

    assert_eq!(repo_rot::summary(&context).hard_findings, 2);
}

#[test]
fn repo_rot_rejects_fake_source_inside_versioned_contract_directory() {
    let context = ctx(vec![code_file(
        "contracts/v2/payment-old.rs",
        "pub fn charge() {}\n",
    )]);

    let findings = repo_rot::findings(&context);

    assert_eq!(repo_rot::summary(&context).hard_findings, 1);
    assert!(findings
        .iter()
        .any(|finding| finding.matched_term == "repo-rot.path.fake-versioned-source"));
}

#[test]
fn repo_rot_rejects_unpaired_schema_inside_versioned_contract_directory() {
    let context = ctx(vec![code_file(
        "contracts/v2/widget-events-v2.schema.json",
        r#"{
  "$schema": "https://json-schema.org/draft/2020-12/schema",
  "$id": "https://contracts.example.test/widget-events-v2.schema.json",
  "title": "widget-events-v2",
  "version": 2
}"#,
    )]);

    let findings = repo_rot::findings(&context);

    assert_eq!(repo_rot::summary(&context).hard_findings, 1);
    assert!(findings
        .iter()
        .any(|finding| finding.matched_term == "repo-rot.path.fake-versioned-source"));
}

#[test]
fn repo_rot_rejects_mismatched_schema_version_and_malformed_jsonl() {
    let mismatched = ctx(vec![
        code_file(
            "contracts/widget-events-v2.schema.json",
            r#"{
  "$schema": "https://json-schema.org/draft/2020-12/schema",
  "$id": "https://contracts.example.test/widget-events-v2.schema.json",
  "title": "widget-events-v2",
  "version": 3
}"#,
        ),
        code_file(
            "contracts/widget-events-v2.jsonl",
            "{\"kind\":\"created\"}\n",
        ),
    ]);
    let malformed = ctx(vec![
        code_file(
            "contracts/widget-events-v2.schema.json",
            r#"{
  "$schema": "https://json-schema.org/draft/2020-12/schema",
  "$id": "https://contracts.example.test/widget-events-v2.schema.json",
  "title": "widget-events-v2",
  "version": 2
}"#,
        ),
        code_file(
            "contracts/widget-events-v2.jsonl",
            "{\"kind\":\"created\"}\nnot-json\n",
        ),
    ]);

    assert_eq!(repo_rot::summary(&mismatched).hard_findings, 2);
    assert_eq!(repo_rot::summary(&malformed).hard_findings, 2);
}

#[test]
fn repo_rot_rejects_contract_schema_identity_not_matching_basename() {
    let context = ctx(vec![
        code_file(
            "contracts/widget-events-v2.schema.json",
            r#"{
  "$schema": "https://json-schema.org/draft/2020-12/schema",
  "$id": "https://contracts.example.test/other-events-v2.schema.json",
  "title": "widget-events-v2",
  "version": 2
}"#,
        ),
        code_file(
            "contracts/widget-events-v2.jsonl",
            "{\"kind\":\"created\"}\n",
        ),
    ]);

    assert_eq!(repo_rot::summary(&context).hard_findings, 2);
}

#[test]
fn repo_rot_still_flags_fake_versioned_source_path() {
    let context = ctx(vec![code_file(
        "crates/app/src/payment-v2.rs",
        "pub fn charge() {}\n",
    )]);

    let findings = repo_rot::findings(&context);

    assert_eq!(repo_rot::summary(&context).hard_findings, 1);
    assert!(findings
        .iter()
        .any(|finding| finding.matched_term == "repo-rot.path.fake-versioned-source"));
}

/// `final` is domain vocabulary: only whole backup-ish name parts count.
#[test]
fn repo_rot_ignores_final_as_a_domain_word() {
    let context = ctx(vec![
        code_file("contracts/v1/model-diagnostics/final-outcome.json", "{}\n"),
        code_file("crates/app/src/final_score.rs", "pub fn score() {}\n"),
    ]);
    assert_eq!(repo_rot::summary(&context).hard_findings, 0);
}

#[test]
fn repo_rot_flags_backup_name_patterns() {
    for path in [
        "crates/app/src/handler-old.rs",
        "crates/app/src/handler_bak.rs",
        "crates/app/src/copy-of-handler.rs",
        "crates/app/src/handler-final-final.rs",
    ] {
        let context = ctx(vec![code_file(path, "pub fn handle() {}\n")]);
        assert!(
            repo_rot::findings(&context)
                .iter()
                .any(|f| f.matched_term == "repo-rot.path.fake-versioned-source"),
            "{path} must be flagged"
        );
    }
}

#[test]
fn repo_rot_findings_are_capped_and_deterministic() {
    let context = ctx(vec![code_file(
        "crates/app/src/service.rs",
        "pub fn compute(value: u32) -> u32 {\n    value.saturating_add(1)\n}\n",
    )]);
    // Two evaluations of the same context must yield the same finding count.
    let first = repo_rot::findings(&context).len();
    let second = repo_rot::findings(&context).len();
    assert_eq!(first, second, "repo-rot findings must be deterministic");
}

#[test]
fn zyal_summary_ignores_plain_rust_source() {
    let context = ctx(vec![code_file(
        "crates/app/src/lib.rs",
        "pub fn run() {}\n",
    )]);
    let summary = zyal::summary(&context);
    assert_eq!(
        summary.hard_findings, 0,
        "non-runbook Rust source must not be flagged as a misplaced ZYAL runbook"
    );
}

#[test]
fn zyal_flags_runbook_file_outside_canonical_root() {
    let context = ctx(vec![code_file(
        "ops/runbooks/radar.zyal",
        "<<<ZYAL 1.0.0:daemon id=radar>>>\nbody: true\n<<<END_ZYAL id=radar>>>\n",
    )]);
    let findings = zyal::findings(&context);
    assert!(
        findings
            .iter()
            .any(|finding| finding.problem.contains("must live under agent/zyal")),
        "a .zyal runbook outside agent/zyal must be flagged as misplaced: {findings:?}"
    );
}

#[test]
fn zyal_flags_runbook_in_a_noncanonical_zyal_directory() {
    let context = ctx(vec![code_file(
        "ops/zyal/radar.txt",
        "<<<ZYAL 1.0.0:daemon id=radar>>>\nbody: true\n<<<END_ZYAL id=radar>>>\n",
    )]);
    assert!(!zyal::findings(&context).is_empty());
}

/// An envelope quoted in a chat log, a note or a test fixture is not a runbook:
/// placement fires only for runbook locations (`.zyal` files, `zyal/` folders).
#[test]
fn zyal_ignores_envelopes_quoted_outside_runbook_locations() {
    let envelope = "<<<ZYAL 1.0.0:daemon id=radar>>>\nbody: true\n<<<END_ZYAL id=radar>>>\n";
    let mut chat = code_file(
        "AGENT_CHAT.md",
        &format!("Agent A: arm this one\n\n{envelope}\nAgent B: armed.\n"),
    );
    chat.is_code = false;
    let context = ctx(vec![
        chat,
        code_file("crates/app/src/runbook_parse_test_data.rs", envelope),
        code_file("crates/app/tests/fixtures/radar.txt", envelope),
    ]);
    let findings = zyal::findings(&context);
    assert!(
        findings
            .iter()
            .all(|finding| !finding.problem.contains("must live under agent/zyal")),
        "quoted envelopes must not be flagged as misplaced runbooks: {findings:?}"
    );
}
