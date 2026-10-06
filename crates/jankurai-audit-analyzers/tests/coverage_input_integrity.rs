use jankurai_audit_analyzers::audit::coverage::{self, CoverageAuditOptions};
use std::{fs, path::PathBuf};

fn parse(source: &str) -> anyhow::Result<coverage::LcovReport> {
    let root = tempfile::tempdir()?;
    let path = root.path().join("lcov.info");
    fs::write(&path, source)?;
    coverage::parse_lcov(&path, 4096)
}

#[test]
fn complete_native_lcov_and_crlf_preserve_paths_and_counts() {
    let source = "TN:unit\nSF:source with spaces/a.rs\nFN:1,example\nFNDA:2,example\nFNF:1\nFNH:1\nDA:1,2,checksum\nDA:2,0\nBRDA:1,0,then,2\nBRDA:1,0,else,-\nLF:2\nLH:1\nBRF:2\nBRH:1\nend_of_record\n";
    for source in [source.to_owned(), source.replace('\n', "\r\n")] {
        let report = parse(&source).unwrap();
        assert_eq!((report.total_lines, report.covered_lines), (2, 1));
        assert_eq!((report.total_branches, report.covered_branches), (2, 1));
        assert!(report.files.contains_key("source with spaces/a.rs"));
        assert!(report.parser_warnings.is_empty());
    }
}

#[test]
fn repeated_complete_test_records_merge_without_dropping_uncovered_lines() {
    let report = parse("TN:first\nSF:src/a.rs\nDA:1,0\nDA:2,1\nBRDA:2,0,0,1\nend_of_record\nTN:second\nSF:src/a.rs\nDA:2,3\nBRDA:2,0,0,0\nend_of_record\n").unwrap();
    assert_eq!((report.total_lines, report.covered_lines), (2, 1));
    assert_eq!((report.total_branches, report.covered_branches), (1, 1));
    assert_eq!(report.files["src/a.rs"].lines[&1], 0);
    assert_eq!(report.files["src/a.rs"].lines[&2], 4);
}

#[test]
fn truncated_malformed_and_contradictory_records_cannot_be_complete_coverage() {
    for source in [
        "SF:src/a.rs\nDA:1,1\n",
        "SF:src/a.rs\nDA:1,1\nSF:src/b.rs\nDA:2,1\nend_of_record\n",
        "SF:src/a.rs\nDA:1,1\nTN:other\nend_of_record\n",
        "SF:src/a.rs\nDA:1,1\nDA:1,0\nend_of_record\n",
        "SF:src/a.rs\nDA:0,1\nend_of_record\n",
        "SF:src/a.rs\nDA:1,1,checksum,extra\nend_of_record\n",
        "SF:src/a.rs\nDA:1,1\nBRDA:1,0,0,bad\nend_of_record\n",
        "BRDA:1,0,0,1\nSF:src/a.rs\nDA:1,1\nend_of_record\n",
        "SF:src/a.rs\nDA:1,1\nBRDA:1,0,0,1\nBRDA:1,0,0,0\nend_of_record\n",
        "SF:src/a.rs\nDA:1,1\nDA:2,1\nLF:1\nend_of_record\n",
        "SF:src/a.rs\nDA:1,1\nLH:0\nend_of_record\n",
        "SF:src/a.rs\nDA:1,1\nLF:2\nLH:3\nend_of_record\n",
        "SF:src/a.rs\nLF:3\nLH:1\nend_of_record\nSF:src/b.rs\nDA:1,1\nend_of_record\n",
        "SF:src/a.rs\nDA:1,1\nBRF:1\nend_of_record\n",
        "SF:src/a.rs\nDA:1,1\nLF:1\nLF:1\nend_of_record\n",
        "SF:src/a.rs\nDA:1,1\nDAX:2,0\nend_of_record\n",
        "SF:src/a.rs\nend_of_record\n",
        "SF:src/a.rs\nDA:1,18446744073709551615\nend_of_record\nSF:src/a.rs\nDA:1,1\nend_of_record\n",
    ] {
        assert!(parse(source).is_err(), "{source}");
    }
}

#[test]
fn artifact_reads_require_regular_bounded_nonempty_utf8_inputs() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("input.info");
    assert!(coverage::parse_lcov(&path, 4096).is_err());
    assert!(coverage::parse_lcov(root.path(), 4096).is_err());
    for bytes in [vec![], vec![0xff], vec![b'x'; 4097]] {
        fs::write(&path, bytes).unwrap();
        assert!(coverage::parse_lcov(&path, 4096).is_err());
    }
    fs::write(&path, "SF:a.rs\nDA:1,1\nend_of_record\n").unwrap();
    assert!(coverage::parse_lcov(&path, 4096).is_ok());
    #[cfg(unix)]
    {
        let alias = root.path().join("alias.info");
        std::os::unix::fs::symlink(&path, &alias).unwrap();
        assert!(coverage::parse_lcov(&alias, 4096).is_err());
    }
}

#[test]
fn malformed_source_globs_fail_before_evidence_selection() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("coverage.toml");
    for mode in ["required", "advisory", "disabled", "auto"] {
        fs::write(&path, format!("version = 1\n[[source]]\nid = 'rust'\nkind = 'line_coverage'\nformat = 'lcov'\nmode = '{mode}'\nowner = 'tools'\nlane = 'coverage-audit'\nartifacts = ['lcov.info']\napplies_to = ['[']\nrules = ['HLT-008-FALSE-GREEN-RISK']\n")).unwrap();
        assert!(
            coverage::load_coverage_config(root.path(), &path).is_err(),
            "{mode}"
        );
    }
}

#[test]
fn required_audit_retains_a_hard_parser_finding_for_truncated_coverage() {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("coverage.toml"), "version = 1\n[[source]]\nid = 'rust'\nkind = 'line_coverage'\nformat = 'lcov'\nmode = 'required'\nowner = 'tools'\nlane = 'coverage-audit'\nartifacts = ['lcov.info']\napplies_to = ['src/**/*.rs']\nrules = ['HLT-008-FALSE-GREEN-RISK']\n").unwrap();
    fs::write(root.path().join("lcov.info"), "SF:src/a.rs\nDA:1,1\n").unwrap();
    let report = coverage::run_coverage_audit(CoverageAuditOptions {
        repo_root: root.path().into(),
        config_path: PathBuf::from("coverage.toml"),
        changed_from: None,
        strict: false,
        max_artifact_bytes: 4096,
        max_findings: 200,
    })
    .unwrap();
    assert_eq!(report.summary.status, "fail");
    assert!(report.summary.hard_findings > 0);
    assert!(report.findings.iter().any(|finding| finding
        .evidence
        .iter()
        .any(|text| text.contains("missing final end_of_record"))));
}

#[cfg(unix)]
#[test]
fn automatic_source_discovery_cannot_silently_disable_symlinked_inputs() {
    let root = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    fs::write(outside.path().join("source.rs"), "pub fn value() {}\n").unwrap();
    fs::create_dir(root.path().join("src")).unwrap();
    std::os::unix::fs::symlink(
        outside.path().join("source.rs"),
        root.path().join("src/source.rs"),
    )
    .unwrap();
    fs::write(root.path().join("coverage.toml"), "version = 1\n[[source]]\nid = 'rust'\nkind = 'line_coverage'\nformat = 'lcov'\nmode = 'auto'\nowner = 'tools'\nlane = 'coverage-audit'\nartifacts = ['lcov.info']\napplies_to = ['src/**/*.rs']\nrules = ['HLT-008-FALSE-GREEN-RISK']\n").unwrap();
    let error = coverage::run_coverage_audit(CoverageAuditOptions {
        repo_root: root.path().into(),
        config_path: PathBuf::from("coverage.toml"),
        changed_from: None,
        strict: false,
        max_artifact_bytes: 4096,
        max_findings: 200,
    })
    .unwrap_err();
    assert!(error.to_string().contains("applicable source is a symlink"));
}

// cargo-llvm-cov 0.8.7 (LLVM line summary) declares LF/LH above the DA records
// it emits: this sample has 31 DA lines (30 hit) under LF:32 and LH:31.
#[test]
fn cargo_llvm_cov_lcov_with_lf_above_da_records_is_complete() {
    let source = include_str!("fixtures/lcov/cargo-llvm-cov-0.8.7.lcov");
    let report = parse(source).expect("cargo-llvm-cov 0.8.7 LCOV parses");
    assert_eq!(report.total_lines, 31);
    assert_eq!(report.covered_lines, 30);
}

#[test]
fn declared_totals_above_captured_records_are_accepted() {
    for source in [
        "SF:src/a.rs\nDA:1,1\nLF:2\nend_of_record\n",
        "SF:src/a.rs\nDA:1,0\nLH:1\nLF:2\nend_of_record\n",
    ] {
        assert!(parse(source).is_ok(), "{source}");
    }
}
