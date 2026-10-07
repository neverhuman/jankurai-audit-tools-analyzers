//! Operation evidence is narrower than a proof of credential origin or CSRF purpose.
use jankurai_audit_analyzers::audit::web_security;
use jankurai_audit_kernel::audit::helpers::AuditContext;
use jankurai_audit_kernel::model::FileInfo;
use std::path::PathBuf;

fn lines(source: &str) -> Vec<usize> {
    lines_at("src/storage.ts", source)
}

fn lines_at(path: &str, source: &str) -> Vec<usize> {
    let file = FileInfo {
        rel_path: path.into(),
        name: path.rsplit('/').next().unwrap().into(),
        suffix: format!(".{}", path.rsplit('.').next().unwrap()),
        size: source.len() as u64,
        line_count: source.lines().count(),
        text: source.into(),
        is_generated: false,
        is_code: true,
    };
    let ctx = AuditContext {
        root: PathBuf::from("/nonexistent/jankurai-browser-storage-pair"),
        all_files: vec![file.clone()],
        scope_files: vec![file],
        scope_paths: vec![],
        self_audit: false,
        boundary_reclassifications: vec![],
        copy_code: None,
    };
    web_security::findings(&ctx)
        .into_iter()
        .filter(|finding| finding.matched_term == "websec.storage.token")
        .map(|finding| {
            assert_eq!(finding.rule_id, "HLT-039-WEB-SECURITY-BAD-BEHAVIOR");
            finding.line.unwrap()
        })
        .collect()
}

fn assert_pair(source: &str, expected: bool) {
    assert_eq!(!lines(source).is_empty(), expected, "{source}");
}

#[test]
fn accessors_and_preferences_are_not_credential_operations() {
    // This retained historical fixture proves the accessor false positive only.
    // The opaque persistCsrf helper does not establish a CSRF value-origin theorem.
    assert_pair(
        r#"
function browserStorage(): Storage | null {
 return typeof window === "undefined" ? null : window.sessionStorage;
}
export function persistCsrf(csrf: string): void {
 browserStorage()?.setItem("bullet-farm.csrf.v1", csrf);
}
"#,
        false,
    );
    for source in [
        "const storage = window.sessionStorage;",
        "return globalThis.localStorage;",
        "sessionStorage.setItem('theme', 'dark');",
        "const storage = sessionStorage; storage.getItem('theme');",
        "const csrf = 'nonce'; sessionStorage.setItem('csrf', csrf);",
        "sessionStorage.theme = 'dark';",
        "sessionStorage.theme += 'light';",
        "sessionStorage.setItem?.('theme', 'dark');",
        "const store: Storage = sessionStorage; store.setItem('theme', 'dark');",
        "const store = sessionStorage\nstore.setItem('theme', 'dark')",
        "sessionStorage!.setItem('theme', 'dark');",
        "const store = sessionStorage!; store.setItem('theme', 'dark');",
        "sessionStorage.removeItem('access_token'); sessionStorage.clear();",
        "window.sessionStorage['removeItem']('token');",
    ] {
        assert_pair(source, false);
    }
}

#[test]
fn credential_set_get_and_property_operations_remain_findings() {
    for source in [
        "localStorage.setItem('access_token', accessToken);",
        "sessionStorage.setItem('csrf', accessToken);",
        "window.sessionStorage.setItem('theme', password);",
        "self.localStorage.getItem('refresh_token');",
        "globalThis.sessionStorage.authorization;",
        "sessionStorage['jwt'];",
        "window['sessionStorage']['setItem']('theme', secret);",
        "localStorage['csrf'] = authorization;",
        "localStorage.csrf = token;",
        "localStorage.csrf += accessToken;",
        "localStorage['csrf'] ||= accessToken;",
        "localStorage.csrf = (() => accessToken)();",
        "sessionStorage.getItem('csrf_token');",
    ] {
        assert_pair(source, true);
    }
}

#[test]
fn storage_and_credential_aliases_do_not_hide_misleading_keys() {
    for source in [
        "const store = sessionStorage; store.setItem('csrf', accessToken);",
        "const store: Storage = sessionStorage; store.setItem('csrf', accessToken);",
        "const store: Storage | null = window.sessionStorage; store?.setItem('csrf', accessToken);",
        "const store = sessionStorage\nstore.setItem('csrf', accessToken)",
        "const store = window.sessionStorage\nstore.setItem('csrf', accessToken)",
        "const store = sessionStorage!; store.setItem('csrf', accessToken);",
        "const second = first; const first = window.localStorage; second.setItem('csrf', jwt);",
        "let store = globalThis['sessionStorage']; store?.setItem('theme', secret);",
        "const csrf = accessToken; sessionStorage.setItem('csrf', csrf);",
        "let csrf = ''; csrf += accessToken; sessionStorage.setItem('csrf', csrf);",
        "const csrf = (\n accessToken\n); sessionStorage.setItem('csrf', csrf);",
        "const csrf = flag\n ? accessToken : 'nonce'; sessionStorage.setItem('csrf', csrf);",
        "const renamed = csrf; const csrf = accessToken; localStorage.setItem('theme', renamed);",
        "const key = 'access_token'; localStorage.getItem(key);",
        "const store = localStorage; store['csrf'] = authorization;",
    ] {
        assert_pair(source, true);
    }
    assert_pair(
        "const store = localStorage; const color = 'dark'; store.setItem('theme', color);",
        false,
    );
}

#[test]
fn unrelated_next_statements_do_not_taint_a_known_literal() {
    for source in [
        "const csrf = 'nonce'\nconst auth = accessToken;\nsessionStorage.setItem('csrf', csrf);",
        "const csrf = 'nonce'\nrecord(accessToken);\nsessionStorage.setItem('csrf', csrf);",
        "const csrf = 'as'\nconst auth = accessToken;\nsessionStorage.setItem('csrf', csrf);",
        "localStorage.theme = 'dark'\nconst auth = accessToken;",
    ] {
        assert_pair(source, false);
    }
    assert_pair(
        "const csrf = accessToken\nrecord('nonce');\nsessionStorage.setItem('csrf', csrf);",
        true,
    );
}

#[test]
fn multiline_and_optional_operations_keep_their_source_location() {
    let source =
        "const store = window.sessionStorage;\nstore\n ?.setItem(\n 'csrf',\n accessToken\n);";
    assert_eq!(lines(source), vec![2]);
    assert_eq!(
        lines("window\n.sessionStorage\n.setItem('csrf',\n password);"),
        vec![1]
    );
    assert_pair(
        "sessionStorage /* object */ . setItem ( 'csrf', /* value */ accessToken );",
        true,
    );
    assert_pair("sessionStorage?.['setItem']('theme', token);", true);
    assert_pair("sessionStorage.setItem?.('csrf', token);", true);
    assert_pair("sessionStorage['getItem']?.('access_token');", true);
    assert_pair("sessionStorage!.setItem('csrf', accessToken);", true);
    assert_pair("sessionStorage.setItem!('csrf', accessToken);", true);
}

#[test]
fn removal_only_never_suppresses_another_operation() {
    for source in [
        "sessionStorage.removeItem('token'); sessionStorage.setItem('token', token);",
        "sessionStorage.clear(); localStorage.csrf = password;",
        "localStorage.setItem('csrf', accessToken); localStorage.removeItem('token');",
        "sessionStorage.removeItem('token'); sessionStorage.getItem('token');",
    ] {
        assert_eq!(lines(source), vec![1], "{source}");
    }
    assert_pair(
        "sessionStorage.removeItem('token'); localStorage.clear();",
        false,
    );
}

#[test]
fn literal_catalogs_and_comments_do_not_execute_storage_calls() {
    let data = r#"
// sessionStorage.setItem('access_token', accessToken);
/* window.localStorage.password = password; */
const example = "localStorage.setItem('token', token)";
const escaped = 'a \' quote sessionStorage.setItem("token", token)';
const template = `sessionStorage.setItem("token", token)`;
"#;
    assert_pair(data, false);
    assert_eq!(
        lines(&format!(
            "{data}\nsessionStorage.setItem('csrf', accessToken);"
        )),
        vec![8]
    );
    assert_pair(
        "sessionStorage.setItem('token', 'https://host.invalid');",
        true,
    );
}

#[test]
fn incomplete_or_unsupported_lexical_forms_do_not_erase_known_positives() {
    for source in [
        "sessionStorage.setItem('token', token",
        "localStorage.setItem('token', token); /* unterminated",
        "const bad = `value ${sessionStorage.setItem('token', token)}`;",
        "const bad = 'unterminated; sessionStorage.setItem('token', token);",
    ] {
        assert_pair(source, true);
    }
    let deep = format!(
        "localStorage.setItem('csrf', {}accessToken{});",
        "(".repeat(1024),
        ")".repeat(1024)
    );
    assert_pair(&deep, true);
}

#[test]
fn existing_nearby_allow_semantics_are_preserved() {
    let allow = "// jankurai:allow websec.storage.token reason=reviewed fixture owner=web expires=2099-12-31";
    assert_pair(
        &format!("{allow}\nsessionStorage.setItem('token', token);"),
        false,
    );
    assert_pair(
        &format!("{allow}\n\n\n\nsessionStorage.setItem('token', token);"),
        true,
    );
    assert_pair(
        "// csrf-only preference\nlocalStorage.setItem('csrf', accessToken);",
        true,
    );
}

#[test]
fn html_event_handlers_retain_conservative_credential_detection() {
    assert_eq!(
        lines_at(
            "src/page.html",
            r#"<button onclick="localStorage.setItem('token', token)">"#
        ),
        vec![1]
    );
    assert!(lines_at("src/page.html", r#"<button title="ordinary preference">"#).is_empty());
}
