use jankurai_audit_kernel::audit::ci_provider;
use jankurai_audit_kernel::audit::helpers::*;
use jankurai_audit_kernel::model::DimensionResult;

pub fn analyze(ctx: &AuditContext) -> DimensionResult {
    let mut score = 20;
    let mut evidence = vec![];
    let mut notes = vec![];
    if ctx.all_files.iter().any(|f| {
        [
            "Cargo.lock",
            "package-lock.json",
            "pnpm-lock.yaml",
            "yarn.lock",
            "poetry.lock",
            "uv.lock",
            "Gemfile.lock",
        ]
        .contains(&f.name.as_str())
    }) {
        score += 12;
        evidence.push("lockfile present".into());
    }
    let surface_text = command_surface_text(ctx);
    let security_text = security_lane_text(ctx);
    if [
        "gitleaks",
        "detect-secrets",
        "secret",
        "audit",
        "deny",
        "dependency-review",
    ]
    .iter()
    .any(|n| surface_text.contains(n))
    {
        score += 12;
        evidence.push("secret or dependency scan tooling found".into());
    }
    if ["syft", "grype", "slsa", "sbom", "cosign"]
        .iter()
        .any(|n| security_text.contains(n))
    {
        score += 8;
        evidence.push("provenance/SBOM tooling found".into());
    }
    // Workflow linting (actionlint, zizmor) lints GitHub Actions workflow files.
    // It applies only when the repository has workflow files to lint. Without
    // any (a jeryu-gated repository, or no committed CI) the check is not
    // applicable: it earns no points and it does not block the complete-posture
    // bonus below. A lint tool found anywhere in the command or lane text still
    // earns the points either way.
    let workflow_lint_found = ["actionlint", "zizmor"]
        .iter()
        .any(|n| security_text.contains(n));
    let workflow_lint_applies = ci_provider::has_github_workflows(&ctx.all_files);
    if workflow_lint_found {
        score += 8;
        evidence.push("workflow linting tooling found".into());
    } else if !workflow_lint_applies {
        evidence
            .push("no GitHub workflow files to lint; workflow-linting check does not apply".into());
    }
    if has_security_lane(ctx) {
        score += 8;
        evidence.push("security lane present".into());
    } else {
        notes.push("no explicit security lane found".into());
    }
    if has_security_lane_script(ctx) {
        score += 6;
        evidence.push("scripted security lane wrapper present".into());
    }
    if has_jankurai_audit_ci_lane(ctx) {
        score += 6;
        evidence.push("agent-readiness audit gate found in CI".into());
    } else {
        score -= 6;
        notes.push("CI does not run the jankurai audit".into());
    }
    let rust_summary = jankurai_audit_kernel::audit::language_rules::rust::summary(ctx);
    let mut hard_language_findings = rust_summary.hard_findings;
    if rust_summary.hard_findings > 0 {
        evidence.push(format!(
            "rust bad-behavior hard findings: {}",
            rust_summary.hard_findings
        ));
        notes.push("rust hard findings are scored through the language-rule catalog".into());
    } else if rust_summary.advisory_signals > 0 {
        evidence.push(format!(
            "rust bad-behavior advisory signals: {}",
            rust_summary.advisory_signals
        ));
    }
    for (label, hard, advisory) in [
        (
            "sql",
            jankurai_audit_kernel::audit::language_rules::sql::summary(ctx).hard_findings,
            jankurai_audit_kernel::audit::language_rules::sql::summary(ctx).advisory_signals,
        ),
        (
            "typescript",
            jankurai_audit_kernel::audit::language_rules::typescript::summary(ctx).hard_findings,
            jankurai_audit_kernel::audit::language_rules::typescript::summary(ctx).advisory_signals,
        ),
        (
            "docker",
            jankurai_audit_kernel::audit::language_rules::docker::summary(ctx).hard_findings,
            jankurai_audit_kernel::audit::language_rules::docker::summary(ctx).advisory_signals,
        ),
        (
            "python",
            jankurai_audit_kernel::audit::language_rules::python::summary(ctx).hard_findings,
            jankurai_audit_kernel::audit::language_rules::python::summary(ctx).advisory_signals,
        ),
        (
            "ci",
            jankurai_audit_kernel::audit::language_rules::ci::summary(ctx).hard_findings,
            jankurai_audit_kernel::audit::language_rules::ci::summary(ctx).advisory_signals,
        ),
        (
            "git",
            jankurai_audit_kernel::audit::language_rules::git::summary(ctx).hard_findings,
            jankurai_audit_kernel::audit::language_rules::git::summary(ctx).advisory_signals,
        ),
        (
            "gittools",
            jankurai_audit_kernel::audit::language_rules::gittools::summary(ctx).hard_findings,
            jankurai_audit_kernel::audit::language_rules::gittools::summary(ctx).advisory_signals,
        ),
        (
            "release",
            jankurai_audit_kernel::audit::language_rules::release::summary(ctx).hard_findings,
            jankurai_audit_kernel::audit::language_rules::release::summary(ctx).advisory_signals,
        ),
        (
            "web security",
            crate::audit::web_security::summary(ctx).hard_findings,
            crate::audit::web_security::summary(ctx).advisory_signals,
        ),
    ] {
        hard_language_findings += hard;
        if hard > 0 {
            evidence.push(format!("{label} bad-behavior hard findings: {hard}"));
        } else if advisory > 0 {
            evidence.push(format!("{label} bad-behavior advisory signals: {advisory}"));
        }
    }
    let audits_cover_ecosystems = dependency_audits_cover_ecosystems(ctx, &security_text);
    if audits_cover_ecosystems {
        score += 8;
        evidence.push("every dependency ecosystem present has an operational audit command".into());
    }
    if security_text.contains("gitleaks detect") {
        score += 6;
        evidence.push("secret scanning command is operational".into());
    }
    if hard_language_findings == 0
        && has_security_lane(ctx)
        && has_jankurai_audit_ci_lane(ctx)
        && has_security_lane_script(ctx)
        && audits_cover_ecosystems
        && security_text.contains("gitleaks detect")
        && ["syft", "grype", "slsa", "sbom", "cosign"]
            .iter()
            .any(|n| security_text.contains(n))
        && (workflow_lint_found || !workflow_lint_applies)
    {
        score += 8;
        evidence.push(
            "complete operational security command posture with zero hard language findings".into(),
        );
    }
    make_dim("Security and supply-chain posture", score, evidence, notes)
}

/// Dependency audits are judged per ecosystem the repository actually has: a Rust-only
/// repository needs a Rust audit, not `npm audit` as well.
fn dependency_audits_cover_ecosystems(ctx: &AuditContext, security_text: &str) -> bool {
    let has = |names: &[&str]| {
        ctx.all_files
            .iter()
            .any(|f| names.contains(&f.name.as_str()))
    };
    let covered = |cmds: &[&str]| cmds.iter().any(|c| security_text.contains(c));
    let ecosystems = [
        (
            has(&["Cargo.lock"]),
            covered(&["cargo audit", "cargo deny", "cargo-audit", "cargo-deny"]),
        ),
        (
            has(&["package-lock.json", "pnpm-lock.yaml", "yarn.lock"]),
            covered(&[
                "npm audit",
                "pnpm audit",
                "yarn audit",
                "yarn npm audit",
                "osv-scanner",
            ]),
        ),
        (
            has(&["poetry.lock", "uv.lock", "requirements.txt"]),
            covered(&["pip-audit", "safety check", "osv-scanner"]),
        ),
        (has(&["go.sum"]), covered(&["govulncheck", "osv-scanner"])),
    ];
    let present: Vec<bool> = ecosystems
        .iter()
        .filter(|(p, _)| *p)
        .map(|(_, c)| *c)
        .collect();
    !present.is_empty() && present.iter().all(|c| *c)
}
