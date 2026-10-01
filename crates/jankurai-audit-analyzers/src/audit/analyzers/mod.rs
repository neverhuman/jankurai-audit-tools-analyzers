pub mod ast;
pub mod context;
pub mod contracts;
pub mod data;
pub mod observability;
pub mod ownership;
pub mod proof;
pub mod python;
pub mod security;
pub mod shape;
pub mod speed;
pub mod tool_adoption;
pub mod tuiwright;

use jankurai_audit_kernel::audit::helpers::AuditContext;
use jankurai_audit_kernel::model::ProfileStructureReadiness;
use jankurai_audit_kernel::model::*;
use rayon::prelude::*;

pub fn all_dimensions(
    ctx: &AuditContext,
    profile_structure: &ProfileStructureReadiness,
) -> Vec<DimensionResult> {
    let analyzers: [fn(&AuditContext) -> DimensionResult; 10] = [
        contracts::analyze,
        proof::analyze,
        security::analyze,
        shape::analyze,
        data::analyze,
        observability::analyze,
        context::analyze,
        tool_adoption::analyze,
        python::analyze,
        speed::analyze,
    ];
    let mut dimensions = vec![ownership::analyze(ctx, profile_structure)];
    dimensions.extend(
        analyzers
            .par_iter()
            .map(|analyze| analyze(ctx))
            .collect::<Vec<_>>(),
    );
    dimensions
}

pub fn ux_qa_status(ctx: &AuditContext) -> UxQaReadiness {
    use jankurai_audit_kernel::audit::helpers::*;

    let mut evidence = serde_json::json!({
        "storybook": paths_with(ctx, &[".storybook/", ".stories.", ".story."], &["@storybook", "storybook", "component story format", "csf"]),
        "playwright_visual": paths_with(ctx, &[], &["tohavescreenshot", "page.screenshot", "locator.screenshot", "visual comparisons", "screenshotpath"]),
        "visual_review": paths_with(ctx, &["backstop", "loki", "argos", "chromatic", "percy", "applitools"], &["@argos-ci", "argos", "chromatic", "percy", "applitools", "backstopjs", "loki", "visual regression", "visual review"]),
        "accessibility": paths_with(ctx, &[], &["@axe-core", "axe-core", "pa11y", "storybook-addon-a11y", "eslint-plugin-jsx-a11y", "accessibility testing", "wcag"]),
        "layout_stability": paths_with(ctx, &[], &["lighthouse", "lhci", "web-vitals", "cumulative layout shift", "layout shift", "cls"]),
        "api_mocks": paths_with(ctx, &[], &["msw", "mock service worker", "msw-storybook-addon", "mockserviceworker", "orval"]),
        "design_tokens": paths_with(ctx, &["tokens/", "design-tokens", "style-dictionary"], &["design tokens", "design-token", "style dictionary", "style-dictionary", "figma variables", "semantic tokens"]),
        "geometry_runtime": paths_with(ctx, &["packages/ux-qa", "ux-qa"], &["@jankurai/ux-qa", "jankurai-ux-qa", "analyzepage", "expectnouxviolations", "edge clearance", "target size", "getboundingclientrect"]),
        "artifact_backed_proof": paths_with(ctx, &["ux-qa-artifacts", "test-results", "playwright-report"], &["--artifacts-dir", "--screenshot", "--aria-snapshot", "artifactpath", "artifactsdir", "ariasnapshot", "tohavescreenshot", "tomatchariasnapshot", "page.screenshot", "trace"]),
    });
    if let Some(tuiwright) = tuiwright::analyze(ctx) {
        evidence
            .as_object_mut()
            .expect("ux evidence object")
            .insert(
                "tuiwright".into(),
                serde_json::to_value(tuiwright).expect("serialize tuiwright evidence"),
            );
    }
    let web_surface = has_web_surface(ctx);
    let missing = if !web_surface {
        vec![]
    } else {
        let mut v = vec![];
        let storybook = evidence
            .get("storybook")
            .and_then(|v| v.as_array())
            .map(|a| a.is_empty())
            .unwrap_or(true);
        let playwright = evidence
            .get("playwright_visual")
            .and_then(|v| v.as_array())
            .map(|a| a.is_empty())
            .unwrap_or(true);
        let visual = evidence
            .get("visual_review")
            .and_then(|v| v.as_array())
            .map(|a| a.is_empty())
            .unwrap_or(true);
        let accessibility = evidence
            .get("accessibility")
            .and_then(|v| v.as_array())
            .map(|a| a.is_empty())
            .unwrap_or(true);
        let layout = evidence
            .get("layout_stability")
            .and_then(|v| v.as_array())
            .map(|a| a.is_empty())
            .unwrap_or(true);
        let mocks = evidence
            .get("api_mocks")
            .and_then(|v| v.as_array())
            .map(|a| a.is_empty())
            .unwrap_or(true);
        let design = evidence
            .get("design_tokens")
            .and_then(|v| v.as_array())
            .map(|a| a.is_empty())
            .unwrap_or(true);
        let proof = evidence
            .get("artifact_backed_proof")
            .and_then(|v| v.as_array())
            .map(|a| a.is_empty())
            .unwrap_or(true);
        if storybook {
            v.push("Storybook state coverage".into());
        }
        if playwright {
            v.push("Playwright screenshot capture".into());
        }
        if visual {
            v.push("visual review or geometry runtime".into());
        }
        if accessibility {
            v.push("accessibility automation".into());
        }
        if layout {
            v.push("layout stability checks".into());
        }
        if mocks {
            v.push("generated API mocks".into());
        }
        if design {
            v.push("design token discipline".into());
        }
        if proof {
            v.push("artifact-backed UX proof receipts".into());
        }
        v
    };
    UxQaReadiness {
        web_surface,
        has_rendered_ux_lane: !web_surface || missing.is_empty(),
        missing_categories: missing,
        evidence,
        artifact: None,
    }
}

#[cfg(test)]
mod calibration_tests {
    use jankurai_audit_kernel::audit::helpers::AuditContext;
    use jankurai_audit_kernel::model::FileInfo;

    fn product_file(rel_path: &str, text: &str) -> FileInfo {
        let path = std::path::PathBuf::from(rel_path);
        FileInfo {
            rel_path: rel_path.into(),
            name: path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned(),
            suffix: path
                .extension()
                .map(|ext| format!(".{}", ext.to_string_lossy()))
                .unwrap_or_default(),
            size: text.len() as u64,
            line_count: text.lines().count(),
            text: text.into(),
            is_generated: false,
            is_code: true,
        }
    }

    fn make_ctx(files: Vec<FileInfo>) -> AuditContext {
        AuditContext {
            root: std::path::PathBuf::from("."),
            scope_files: files.clone(),
            all_files: files,
            scope_paths: vec![],
            self_audit: false,
            boundary_reclassifications: vec![],
            copy_code: None,
        }
    }

    #[test]
    fn data_truth_does_not_apply_without_a_database() {
        let no_db = make_ctx(vec![product_file(
            "src/main.rs",
            "fn main() { println!(\"hi\"); }\n",
        )]);
        let dim = super::data::analyze(&no_db);
        assert_eq!(dim.score, 90, "{:?}", dim.evidence);
        // A driver in a dependency manifest is a database even without db/ or SQL.
        let driver_only = make_ctx(vec![
            product_file("src/main.rs", "fn main() {}\n"),
            product_file("Cargo.toml", "[dependencies]\nsqlx = \"0.8\"\n"),
        ]);
        let dim = super::data::analyze(&driver_only);
        assert!(
            !dim.evidence.iter().any(|e| e.contains("do not apply")),
            "a driver dependency must be judged: {:?}",
            dim.evidence
        );
        let with_db = make_ctx(vec![
            product_file("src/main.rs", "use sqlx::PgPool;\n"),
            product_file("migrations/0001_init.sql", "CREATE TABLE t (id int);\n"),
        ]);
        let dim = super::data::analyze(&with_db);
        assert!(
            !dim.evidence.iter().any(|e| e.contains("do not apply")),
            "a repository with a database must be judged: {:?}",
            dim.evidence
        );
    }

    #[test]
    fn build_speed_reaches_the_floor_with_generic_signals() {
        let justfile = "\ncheck:\n    cargo check -p app\nfast:\n    cargo nextest run -p app\n";
        let ci = "jobs:\n  ci:\n    steps:\n      - uses: actions/cache@v4\n      - run: cargo check -p app\n";
        let ctx = make_ctx(vec![
            product_file("Justfile", justfile),
            product_file("Cargo.lock", "# lock\n"),
            product_file(".github/workflows/ci.yml", ci),
        ]);
        let dim = super::speed::analyze(&ctx);
        assert!(
            dim.score >= 85,
            "score {} evidence {:?} notes {:?}",
            dim.score,
            dim.evidence,
            dim.notes
        );
    }

    #[test]
    fn dependency_audits_are_judged_per_ecosystem() {
        let rust_only = make_ctx(vec![
            product_file("Cargo.lock", "# lock\n"),
            product_file("ops/ci/security.sh", "cargo deny check\ngitleaks detect\n"),
        ]);
        let dim = super::security::analyze(&rust_only);
        assert!(
            dim.evidence
                .iter()
                .any(|e| e.contains("every dependency ecosystem")),
            "a Rust-only repo with cargo deny should earn the audit credit: {:?}",
            dim.evidence
        );
        assert!(
            dim.evidence
                .iter()
                .any(|e| e.contains("scripted security lane")),
            "an ops/ci security script should count as the lane wrapper: {:?}",
            dim.evidence
        );
        let mixed_uncovered = make_ctx(vec![
            product_file("Cargo.lock", "# lock\n"),
            product_file("package-lock.json", "{}\n"),
            product_file("ops/ci/security.sh", "cargo deny check\n"),
        ]);
        let dim = super::security::analyze(&mixed_uncovered);
        assert!(
            !dim.evidence
                .iter()
                .any(|e| e.contains("every dependency ecosystem")),
            "npm deps without an npm audit must not earn the credit: {:?}",
            dim.evidence
        );
    }
}
