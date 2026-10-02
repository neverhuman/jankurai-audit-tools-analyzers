//! HLT-047-CANONICAL-README and HLT-048-CANONICAL-CI-GAP detectors
//! (Jankurai pillar, canonical-shape guard).
//!
//! Validates that a repository's README and CI match the canonical agent-native
//! shape described in `agent/JANKURAI_STANDARD.md`:
//!
//! - **README (HLT-047)** should state the target stack, carry a status/score
//!   badge, and offer a quick-start (install / getting-started) section. Agents do
//!   not need a README link to find `AGENTS.md`: Codex, Cursor, Copilot and Claude
//!   Code all load it by filename. What hides it is a tool-specific instruction
//!   file (`CLAUDE.md`, `.claude/CLAUDE.md`, `CLAUDE.local.md`, `GEMINI.md`):
//!   Claude Code reads `AGENTS.md` only when no `CLAUDE.md` exists, so HLT-047
//!   flags such a file beside an `AGENTS.md` that does not reference, import or
//!   symlink it.
//! - **CI (HLT-048)** should delegate to versioned `ops/ci/*.sh` scripts (CI Local
//!   Parity), pin every `uses:` action to a full 40-character commit SHA, and run
//!   a jankurai audit lane.
//!
//! Both rules are ADVISORY (registered, not in `fail_on`). Each check only fires
//! when the relevant artifact exists: a repo with no README never trips the README
//! checks, a repo with no `AGENTS.md` never trips the entrypoint check, and a repo with no `.github/workflows/*` never trips HLT-048. A repo that
//! already matches the canonical shape (such as jankurai) yields zero findings
//! and stays ratchet-ready.

use jankurai_audit_kernel::audit::helpers::AuditContext;
use jankurai_audit_kernel::audit::scan::FindingHit;
use jankurai_audit_kernel::model::FileInfo;
use once_cell::sync::Lazy;
use regex::Regex;

/// Canonical-shape policy thresholds read from `[canonical]` in
/// `agent/audit-policy.toml`. Defaults keep both checks advisory and disabled
/// only when explicitly turned off.
struct CanonicalPolicy {
    check_readme: bool,
    check_ci: bool,
    require_badge: bool,
    require_quick_start: bool,
    require_stack: bool,
    require_agents_link: bool,
}

impl Default for CanonicalPolicy {
    fn default() -> Self {
        Self {
            check_readme: true,
            check_ci: true,
            require_badge: true,
            require_quick_start: true,
            require_stack: true,
            require_agents_link: true,
        }
    }
}

fn load_policy(ctx: &AuditContext) -> CanonicalPolicy {
    let mut policy = CanonicalPolicy::default();
    let path = ctx.root.join("agent/audit-policy.toml");
    let Ok(text) = std::fs::read_to_string(&path) else {
        return policy;
    };
    let Ok(value) = toml::from_str::<toml::Value>(&text) else {
        return policy;
    };
    let Some(section) = value.get("canonical") else {
        return policy;
    };
    let flag = |key: &str, default: bool| {
        section
            .get(key)
            .and_then(|v| v.as_bool())
            .unwrap_or(default)
    };
    policy.check_readme = flag("check_readme", policy.check_readme);
    policy.check_ci = flag("check_ci", policy.check_ci);
    policy.require_badge = flag("require_badge", policy.require_badge);
    policy.require_quick_start = flag("require_quick_start", policy.require_quick_start);
    policy.require_stack = flag("require_stack", policy.require_stack);
    policy.require_agents_link = flag("require_agents_link", policy.require_agents_link);
    policy
}

/// Locates the root README file (case-insensitive `README.md` / `README`).
fn readme_file(ctx: &AuditContext) -> Option<&FileInfo> {
    ctx.all_files.iter().find(|file| {
        let lower = file.rel_path.to_ascii_lowercase();
        lower == "readme.md" || lower == "readme" || lower == "readme.markdown"
    })
}

/// Detects README gaps against the canonical agent-native shape. Returns one
/// [`FindingHit`] per missing canonical element so an agent can repair each gap
/// independently. No README => no findings.
pub fn detect_readme_gaps(ctx: &AuditContext) -> Vec<FindingHit> {
    let policy = load_policy(ctx);
    if !policy.check_readme {
        return vec![];
    }
    let mut hits = Vec::new();
    if policy.require_agents_link {
        hits.extend(detect_shadowed_agents_md(ctx));
    }
    let Some(readme) = readme_file(ctx) else {
        return hits;
    };
    let text = &readme.text;
    let lower = text.to_ascii_lowercase();
    let path = readme.rel_path.as_str();
    if policy.require_stack && !states_target_stack(&lower) {
        hits.push(readme_hit(
            path,
            "target stack",
            "README does not state the target stack, so contributors cannot tell what the repo is built on",
            "state the target stack (for example Rust core, TypeScript/React product surface, PostgreSQL) in the README intro",
        ));
    }
    if policy.require_badge && !has_badge(text) {
        hits.push(readme_hit(
            path,
            "status badge",
            "README has no status or score badge, so build/audit health is not visible at a glance",
            "add a CI/score badge (for example a shields.io or jankurai score badge) near the top of the README",
        ));
    }
    if policy.require_quick_start && !has_quick_start(&lower) {
        hits.push(readme_hit(
            path,
            "quick-start",
            "README has no quick-start (install / getting-started) section, so the first-run path is undocumented",
            "add a `## Quick start` (or install / getting-started) section with the minimal commands to run the project",
        ));
    }
    hits
}

/// Detects CI gaps against the canonical CI-Local-Parity shape. Returns one
/// [`FindingHit`] per missing canonical element. No workflows => no findings.
pub fn detect_ci_gaps(ctx: &AuditContext) -> Vec<FindingHit> {
    let policy = load_policy(ctx);
    if !policy.check_ci {
        return vec![];
    }
    let workflows: Vec<&FileInfo> = ctx
        .all_files
        .iter()
        .filter(|file| is_workflow(file))
        .collect();
    if workflows.is_empty() {
        return vec![];
    }
    let anchor = workflows[0].rel_path.as_str();
    let mut hits = Vec::new();

    let delegates = workflows.iter().any(|file| calls_ops_ci(file));
    if !delegates {
        hits.push(ci_hit(
            anchor,
            "ops/ci delegation",
            "no workflow delegates to a versioned `ops/ci/*.sh` script, so CI cannot be reproduced locally before push",
            "move CI commands into `ops/ci/*.sh` and call `bash ops/ci/<lane>.sh` from the workflow",
        ));
    }

    // Flag every workflow that pins an action to a floating tag instead of a full
    // 40-character commit SHA. Floating tags (`@v4`, `@main`) are not reproducible
    // and are a supply-chain hazard.
    for file in &workflows {
        if let Some(unpinned) = first_unpinned_use(file) {
            let mut hit = ci_hit(
                &file.rel_path,
                "pinned action SHA",
                "workflow pins a GitHub Action to a floating tag instead of a full commit SHA",
                "pin every `uses:` action to a full 40-character commit SHA so the workflow is reproducible",
            );
            hit.text = format!("unpinned action `uses: {unpinned}`");
            hits.push(hit);
        }
    }

    let has_audit_lane = workflows.iter().any(|file| runs_jankurai_audit(file));
    if !has_audit_lane {
        hits.push(ci_hit(
            anchor,
            "jankurai audit lane",
            "CI has no jankurai audit lane, so merges are not gated on a repository conformance score",
            "add a jankurai audit lane (for example `bash ops/ci/audit.sh` or a `jankurai audit` step) to CI",
        ));
    }
    hits
}

fn readme_hit(path: &str, element: &str, problem: &str, fix: &str) -> FindingHit {
    FindingHit {
        path: path.to_string(),
        line: Some(1),
        text: format!("README missing canonical element: {element}"),
        matched_term: Some("canonical-readme".into()),
        agent_fix: fix.to_string(),
        problem: problem.to_string(),
    }
}

fn ci_hit(path: &str, element: &str, problem: &str, fix: &str) -> FindingHit {
    FindingHit {
        path: path.to_string(),
        line: Some(1),
        text: format!("CI missing canonical element: {element}"),
        matched_term: Some("canonical-ci".into()),
        agent_fix: fix.to_string(),
        problem: problem.to_string(),
    }
}

/// Tool-specific instruction files that a coding agent loads in place of the
/// `AGENTS.md` in the same directory. `.claude/CLAUDE.md` comes before
/// `CLAUDE.md` so the longer suffix wins.
const AGENTS_MD_SHADOWS: &[&str] = &[
    ".claude/CLAUDE.md",
    "CLAUDE.md",
    "CLAUDE.local.md",
    "GEMINI.md",
];

/// Flags each tool-specific instruction file that sits beside an `AGENTS.md` but
/// neither references, imports nor symlinks it. Claude Code reads `AGENTS.md`
/// only when no `CLAUDE.md` / `.claude/CLAUDE.md` / `CLAUDE.local.md` exists, so
/// such a file silently hides the repository entrypoint from that tool. Repos
/// without an `AGENTS.md`, or without a shadowing file, yield no findings.
fn detect_shadowed_agents_md(ctx: &AuditContext) -> Vec<FindingHit> {
    let mut hits = Vec::new();
    for file in &ctx.all_files {
        let Some(dir) = shadowed_dir(&file.rel_path) else {
            continue;
        };
        let agents_rel = if dir.is_empty() {
            "AGENTS.md".to_string()
        } else {
            format!("{dir}/AGENTS.md")
        };
        let Some(agents) = ctx.all_files.iter().find(|f| f.rel_path == agents_rel) else {
            continue;
        };
        if references_agents_md(&file.text)
            || file.text == agents.text
            || is_symlink_to_agents_md(ctx, &file.rel_path)
        {
            continue;
        }
        let name = file.rel_path.as_str();
        hits.push(FindingHit {
            path: file.rel_path.clone(),
            line: Some(1),
            text: format!("`{name}` does not reference `{agents_rel}`"),
            matched_term: Some("canonical-agents-entrypoint".into()),
            agent_fix: format!(
                "add `@AGENTS.md` (an import) to `{name}`, or replace `{name}` with a symlink to `AGENTS.md`"
            ),
            problem: format!(
                "`{name}` is loaded instead of `{agents_rel}` by its coding agent and never points to it, so that agent misses the repository entrypoint"
            ),
        });
    }
    hits
}

/// Returns the directory (repo-relative, `""` for the root) whose `AGENTS.md`
/// the file at `rel` would shadow, if `rel` is a known shadowing file.
fn shadowed_dir(rel: &str) -> Option<String> {
    AGENTS_MD_SHADOWS.iter().find_map(|name| {
        if rel == *name {
            return Some(String::new());
        }
        let dir = rel.strip_suffix(name)?.strip_suffix('/')?;
        Some(dir.to_string())
    })
}

/// A plain mention counts: an `@AGENTS.md` import, a Markdown link, or the
/// generated adapter's "Read `AGENTS.md` first" all lead the agent there.
fn references_agents_md(text: &str) -> bool {
    text.contains("AGENTS.md")
}

fn is_symlink_to_agents_md(ctx: &AuditContext, rel: &str) -> bool {
    std::fs::read_link(ctx.root.join(rel))
        .map(|target| target.file_name().is_some_and(|name| name == "AGENTS.md"))
        .unwrap_or(false)
}

fn states_target_stack(lower: &str) -> bool {
    // The canonical stack is Rust-first; any explicit stack statement that names
    // a primary stack language counts. Keep this permissive so a repo that names
    // its real stack is never nagged.
    [
        "rust",
        "typescript",
        "react",
        "postgres",
        "python",
        "go",
        "node",
    ]
    .iter()
    .any(|stack| lower.contains(stack))
}

fn has_badge(text: &str) -> bool {
    static BADGE_RE: Lazy<Regex> = Lazy::new(|| {
        // A Markdown image badge `[![alt](img)](link)` or a bare image `![alt](img)`,
        // or an explicit jankurai badge marker.
        Regex::new(r"!\[[^\]]*\]\([^)]*\)").expect("badge regex is valid")
    });
    BADGE_RE.is_match(text)
        || text.contains("shields.io")
        || text.contains("jankurai-badge")
        || text.contains("badge.svg")
}

fn has_quick_start(lower: &str) -> bool {
    lower.contains("quick start")
        || lower.contains("quickstart")
        || lower.contains("quick-start")
        || lower.contains("getting started")
        || lower.contains("## install")
        || lower.contains("# install")
        || lower.contains("## usage")
}

fn is_workflow(file: &FileInfo) -> bool {
    let lower = file.rel_path.to_ascii_lowercase();
    lower.starts_with(".github/workflows/") && (lower.ends_with(".yml") || lower.ends_with(".yaml"))
}

fn calls_ops_ci(file: &FileInfo) -> bool {
    static OPS_CI_RE: Lazy<Regex> =
        Lazy::new(|| Regex::new(r"(?m)\bbash\s+ops/ci/[A-Za-z0-9_./-]+\.sh").expect("regex"));
    OPS_CI_RE.is_match(&file.text)
}

fn runs_jankurai_audit(file: &FileInfo) -> bool {
    let lower = file.text.to_ascii_lowercase();
    lower.contains("ops/ci/audit.sh")
        || lower.contains("jankurai audit")
        || (lower.contains("jankurai") && lower.contains("audit"))
}

/// Returns the first `uses:` reference in `file` that is NOT pinned to a full
/// 40-character commit SHA (i.e. uses a floating tag or branch), if any.
fn first_unpinned_use(file: &FileInfo) -> Option<String> {
    static USES_RE: Lazy<Regex> =
        Lazy::new(|| Regex::new(r"(?m)^\s*-?\s*uses:\s*(\S+)").expect("uses regex is valid"));
    static SHA_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"@[0-9a-f]{40}$").expect("sha regex"));
    for caps in USES_RE.captures_iter(&file.text) {
        let reference = caps.get(1).map(|m| m.as_str()).unwrap_or_default();
        // Local composite actions (`./.github/...`) and docker refs are not SHA-pinnable.
        if reference.starts_with("./") || reference.starts_with("docker://") {
            continue;
        }
        if !reference.contains('@') {
            return Some(reference.to_string());
        }
        if !SHA_RE.is_match(reference) {
            return Some(reference.to_string());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn file(rel: &str, text: &str) -> FileInfo {
        FileInfo {
            rel_path: rel.into(),
            name: rel.rsplit('/').next().unwrap_or(rel).into(),
            suffix: format!(".{}", rel.rsplit('.').next().unwrap_or("")),
            size: text.len() as u64,
            line_count: text.lines().count(),
            text: text.into(),
            is_generated: false,
            is_code: false,
        }
    }

    fn ctx_for(files: Vec<FileInfo>) -> AuditContext {
        AuditContext {
            root: PathBuf::from("/nonexistent-canonical-root"),
            all_files: files,
            scope_files: vec![],
            scope_paths: vec![],
            self_audit: false,
            boundary_reclassifications: vec![],
            copy_code: None,
        }
    }

    const GOOD_README: &str = "# My Project\n\
        [![CI](https://img.shields.io/badge/ci-green.svg)](ci)\n\n\
        Built on a Rust core with a PostgreSQL durable store.\n\n\
        See [AGENTS.md](AGENTS.md) for the agent entrypoint.\n\n\
        ## Quick start\n\n```\ncargo install --path .\n```\n";

    const GOOD_WORKFLOW: &str = concat!(
        "name: ci\n",
        "jobs:\n",
        "  build:\n",
        "    runs-on: ubuntu-latest\n",
        "    steps:\n",
        "      - uses: actions/checkout@de0fac2e4500dabe0009e67214ff5f5447ce83dd\n",
        "      - name: gates\n",
        "        run: bash ops/ci/quality-gates.sh\n",
        "  audit:\n",
        "    steps:\n",
        "      - name: audit\n",
        "        run: bash ops/ci/audit.sh\n",
    );

    #[test]
    fn canonical_readme_yields_no_findings() {
        let hits = detect_readme_gaps(&ctx_for(vec![file("README.md", GOOD_README)]));
        assert!(hits.is_empty(), "canonical README must be clean: {hits:?}");
    }

    #[test]
    fn missing_readme_yields_no_findings() {
        let hits = detect_readme_gaps(&ctx_for(vec![file("src/main.rs", "fn main() {}")]));
        assert!(hits.is_empty(), "no README => no HLT-047: {hits:?}");
    }

    #[test]
    fn readme_without_agents_link_only_misses_quick_start() {
        // Agents load AGENTS.md by filename, so the README needs no link to it.
        let bare = "# Project\n\
            [![CI](https://img.shields.io/badge/ci-green.svg)](ci)\n\n\
            Built on Rust.\n";
        let hits = detect_readme_gaps(&ctx_for(vec![
            file("README.md", bare),
            file("AGENTS.md", "# Agents\n"),
        ]));
        assert_eq!(hits.len(), 1, "only quick-start is missing: {hits:?}");
        assert!(hits[0].text.contains("quick-start"));
        assert_eq!(hits[0].matched_term.as_deref(), Some("canonical-readme"));
    }

    fn entrypoint_hits(files: Vec<FileInfo>) -> Vec<FindingHit> {
        detect_readme_gaps(&ctx_for(files))
            .into_iter()
            .filter(|h| h.matched_term.as_deref() == Some("canonical-agents-entrypoint"))
            .collect()
    }

    #[test]
    fn claude_md_hiding_agents_md_fires() {
        let hits = entrypoint_hits(vec![
            file("AGENTS.md", "# Agents\n"),
            file("CLAUDE.md", "Use cargo test.\n"),
            file("README.md", GOOD_README),
        ]);
        assert_eq!(hits.len(), 1, "{hits:?}");
        assert_eq!(hits[0].path, "CLAUDE.md");
        assert!(hits[0].agent_fix.contains("@AGENTS.md"));
    }

    #[test]
    fn instruction_files_that_point_at_agents_md_pass() {
        let hits = entrypoint_hits(vec![
            file("AGENTS.md", "# Agents\n"),
            file("CLAUDE.md", "Read `AGENTS.md` first.\n"),
            file("GEMINI.md", "@AGENTS.md\n"),
            file(".claude/CLAUDE.md", "@../AGENTS.md\n"),
            file("CLAUDE.local.md", "# Agents\n"),
        ]);
        assert!(
            hits.is_empty(),
            "references and identical copies pass: {hits:?}"
        );
    }

    #[test]
    fn every_shadowing_file_is_checked_per_directory() {
        let hits = entrypoint_hits(vec![
            file("AGENTS.md", "# Agents\n"),
            file(".claude/CLAUDE.md", "local rules\n"),
            file("GEMINI.md", "gemini rules\n"),
            file("crates/x/AGENTS.md", "# X\n"),
            file("crates/x/CLAUDE.md", "x rules\n"),
        ]);
        let mut paths: Vec<_> = hits.iter().map(|h| h.path.as_str()).collect();
        paths.sort_unstable();
        assert_eq!(
            paths,
            [".claude/CLAUDE.md", "GEMINI.md", "crates/x/CLAUDE.md"]
        );
    }

    #[test]
    fn no_agents_md_or_no_shadow_yields_no_entrypoint_findings() {
        assert!(entrypoint_hits(vec![file("CLAUDE.md", "rules\n")]).is_empty());
        assert!(entrypoint_hits(vec![file("AGENTS.md", "# Agents\n")]).is_empty());
        // A CLAUDE.md in another directory does not hide the root AGENTS.md.
        assert!(entrypoint_hits(vec![
            file("AGENTS.md", "# Agents\n"),
            file("docs/CLAUDE.md", "docs rules\n"),
        ])
        .is_empty());
    }

    #[test]
    fn entrypoint_check_runs_without_a_readme() {
        let hits = entrypoint_hits(vec![
            file("AGENTS.md", "# Agents\n"),
            file("CLAUDE.md", "rules\n"),
        ]);
        assert_eq!(hits.len(), 1, "{hits:?}");
    }

    #[cfg(unix)]
    #[test]
    fn symlinked_claude_md_passes() {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(dir.path().join("AGENTS.md"), "# Agents\n").expect("write");
        std::os::unix::fs::symlink("AGENTS.md", dir.path().join("CLAUDE.md")).expect("symlink");
        let mut ctx = ctx_for(vec![
            file("AGENTS.md", "# Agents\n"),
            // A real walker reads through the link; give it different text so only
            // the symlink check can pass it.
            file("CLAUDE.md", "stale text\n"),
        ]);
        ctx.root = dir.path().to_path_buf();
        let hits: Vec<_> = detect_readme_gaps(&ctx)
            .into_iter()
            .filter(|h| h.matched_term.as_deref() == Some("canonical-agents-entrypoint"))
            .collect();
        assert!(hits.is_empty(), "symlink to AGENTS.md passes: {hits:?}");
    }

    #[test]
    fn readme_missing_badge_and_stack_fires() {
        let bare = "# Project\n\nSee [AGENTS.md](AGENTS.md).\n\n## Quick start\nrun it\n";
        let hits = detect_readme_gaps(&ctx_for(vec![file("README.md", bare)]));
        assert!(hits.iter().any(|h| h.text.contains("status badge")));
        assert!(hits.iter().any(|h| h.text.contains("target stack")));
    }

    #[test]
    fn canonical_ci_yields_no_findings() {
        let hits = detect_ci_gaps(&ctx_for(vec![file(
            ".github/workflows/ci.yml",
            GOOD_WORKFLOW,
        )]));
        assert!(hits.is_empty(), "canonical CI must be clean: {hits:?}");
    }

    #[test]
    fn missing_workflows_yields_no_findings() {
        let hits = detect_ci_gaps(&ctx_for(vec![file("README.md", GOOD_README)]));
        assert!(hits.is_empty(), "no workflows => no HLT-048: {hits:?}");
    }

    #[test]
    fn ci_without_ops_delegation_and_audit_fires() {
        let inline = concat!(
            "name: ci\n",
            "jobs:\n",
            "  build:\n",
            "    steps:\n",
            "      - uses: actions/checkout@de0fac2e4500dabe0009e67214ff5f5447ce83dd\n",
            "      - run: cargo test\n",
        );
        let hits = detect_ci_gaps(&ctx_for(vec![file(".github/workflows/ci.yml", inline)]));
        assert!(hits.iter().any(|h| h.text.contains("ops/ci delegation")));
        assert!(hits.iter().any(|h| h.text.contains("jankurai audit lane")));
        assert!(hits
            .iter()
            .all(|h| h.matched_term.as_deref() == Some("canonical-ci")));
    }

    #[test]
    fn ci_with_floating_tag_fires() {
        let floating = concat!(
            "name: ci\n",
            "jobs:\n",
            "  build:\n",
            "    steps:\n",
            "      - uses: actions/checkout@v4\n",
            "      - run: bash ops/ci/audit.sh\n",
        );
        let hits = detect_ci_gaps(&ctx_for(vec![file(".github/workflows/ci.yml", floating)]));
        assert!(
            hits.iter().any(|h| h.text.contains("actions/checkout@v4")),
            "floating tag must be flagged: {hits:?}"
        );
    }

    #[test]
    fn policy_can_disable_checks() {
        // With no policy file on disk (root does not exist), defaults apply and a
        // bare README fires; this guards the default-on behavior.
        let bare = "# Project\n";
        let hits = detect_readme_gaps(&ctx_for(vec![file("README.md", bare)]));
        assert!(!hits.is_empty(), "defaults keep README checks enabled");
    }
}
