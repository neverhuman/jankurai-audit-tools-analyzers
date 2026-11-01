//! Contract proof for the audit-evidence artifacts this repository publishes.
//!
//! `ops/ci/audit.sh` writes `.jankurai/repo-score.json` and
//! `target/jankurai/repair-queue.jsonl`, and `agent/baselines/main.repo-score.json`
//! is the committed report the ratchet compares against. Contract discipline
//! (`contracts/README.md`) says every published evidence artifact validates
//! against its schema, so this test validates the real reports against the
//! committed schemas instead of describing them in prose.
//!
//! The validator below covers exactly the JSON Schema keywords those two
//! schemas use and fails closed on anything else, so a schema cannot silently
//! grow a constraint that is never checked.

use serde_json::{Map, Value};
use std::{collections::BTreeMap, fs, path::Path, path::PathBuf};

const ANNOTATIONS: [&str; 5] = ["$schema", "$id", "title", "description", "$defs"];

struct Schemas {
    root: Value,
    by_file: BTreeMap<String, Value>,
}

impl Schemas {
    fn resolve(&self, reference: &str) -> Result<&Value, String> {
        if let Some(pointer) = reference.strip_prefix("#") {
            return self
                .root
                .pointer(pointer)
                .ok_or_else(|| format!("unresolvable local $ref: {reference}"));
        }
        self.by_file
            .get(reference)
            .ok_or_else(|| format!("unresolvable $ref: {reference}"))
    }
}

fn type_matches(expected: &str, value: &Value) -> bool {
    match expected {
        "object" => value.is_object(),
        "array" => value.is_array(),
        "string" => value.is_string(),
        "boolean" => value.is_boolean(),
        "null" => value.is_null(),
        "number" => value.is_number(),
        "integer" => value.as_i64().is_some() || value.as_u64().is_some(),
        _ => false,
    }
}

fn validate(schema: &Value, value: &Value, at: &str, schemas: &Schemas, errors: &mut Vec<String>) {
    let Some(schema) = schema.as_object() else {
        // `true`/`false` schemas are not used by these contracts.
        errors.push(format!("{at}: non-object schema is not supported"));
        return;
    };
    for key in schema.keys() {
        let known = [
            "type",
            "required",
            "properties",
            "additionalProperties",
            "items",
            "enum",
            "const",
            "pattern",
            "minimum",
            "maximum",
            "$ref",
        ]
        .contains(&key.as_str());
        if !known && !ANNOTATIONS.contains(&key.as_str()) {
            errors.push(format!("{at}: unsupported schema keyword `{key}`"));
        }
    }
    if let Some(reference) = schema.get("$ref").and_then(Value::as_str) {
        match schemas.resolve(reference) {
            Ok(target) => validate(target, value, at, schemas, errors),
            Err(problem) => errors.push(format!("{at}: {problem}")),
        }
        return;
    }
    match schema.get("type") {
        None => {}
        Some(Value::String(expected)) => {
            if !type_matches(expected, value) {
                errors.push(format!("{at}: expected type {expected}"));
                return;
            }
        }
        Some(Value::Array(expected)) => {
            let any = expected
                .iter()
                .filter_map(Value::as_str)
                .any(|e| type_matches(e, value));
            if !any {
                errors.push(format!("{at}: expected one of types {expected:?}"));
                return;
            }
        }
        Some(other) => errors.push(format!("{at}: unsupported type form {other}")),
    }
    if let Some(expected) = schema.get("const") {
        if value != expected {
            errors.push(format!("{at}: expected const {expected}"));
        }
    }
    if let Some(Value::Array(allowed)) = schema.get("enum") {
        if !allowed.contains(value) {
            errors.push(format!("{at}: {value} is not one of {allowed:?}"));
        }
    }
    if let (Some(pattern), Some(text)) = (
        schema.get("pattern").and_then(Value::as_str),
        value.as_str(),
    ) {
        match regex::Regex::new(pattern) {
            Ok(regex) => {
                if !regex.is_match(text) {
                    errors.push(format!("{at}: {text} does not match {pattern}"));
                }
            }
            Err(problem) => errors.push(format!("{at}: invalid pattern {pattern}: {problem}")),
        }
    }
    if let Some(number) = value.as_f64() {
        if let Some(minimum) = schema.get("minimum").and_then(Value::as_f64) {
            if number < minimum {
                errors.push(format!("{at}: {number} is below minimum {minimum}"));
            }
        }
        if let Some(maximum) = schema.get("maximum").and_then(Value::as_f64) {
            if number > maximum {
                errors.push(format!("{at}: {number} is above maximum {maximum}"));
            }
        }
    }
    if let Some(object) = value.as_object() {
        validate_object(schema, object, at, schemas, errors);
    }
    if let (Some(items), Some(array)) = (schema.get("items"), value.as_array()) {
        for (index, item) in array.iter().enumerate() {
            validate(items, item, &format!("{at}[{index}]"), schemas, errors);
        }
    }
}

fn validate_object(
    schema: &Map<String, Value>,
    object: &Map<String, Value>,
    at: &str,
    schemas: &Schemas,
    errors: &mut Vec<String>,
) {
    if let Some(Value::Array(required)) = schema.get("required") {
        for field in required.iter().filter_map(Value::as_str) {
            if !object.contains_key(field) {
                errors.push(format!("{at}: missing required field `{field}`"));
            }
        }
    }
    let properties = schema.get("properties").and_then(Value::as_object);
    if let Some(properties) = properties {
        for (field, child) in properties {
            if let Some(found) = object.get(field) {
                validate(child, found, &format!("{at}.{field}"), schemas, errors);
            }
        }
    }
    if schema.get("additionalProperties") == Some(&Value::Bool(false)) {
        for field in object.keys() {
            if !properties.is_some_and(|p| p.contains_key(field)) {
                errors.push(format!("{at}: unexpected field `{field}`"));
            }
        }
    }
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("workspace root")
        .to_path_buf()
}

fn read_json(path: &Path) -> Value {
    let text = fs::read_to_string(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

fn schemas() -> Schemas {
    let root = repo_root();
    let queue = read_json(&root.join("schemas/repair-queue.schema.json"));
    Schemas {
        root: read_json(&root.join("schemas/repo-score.schema.json")),
        by_file: BTreeMap::from([("repair-queue.schema.json".to_owned(), queue)]),
    }
}

fn errors_for(report: &Value) -> Vec<String> {
    let schemas = schemas();
    let mut errors = vec![];
    validate(
        &schemas.root.clone(),
        report,
        "report",
        &schemas,
        &mut errors,
    );
    errors
}

/// Every repo-score report this repository publishes or compares against is a
/// real auditor artifact, so the committed schema must accept it as written.
#[test]
fn published_repo_score_reports_match_the_committed_schema() {
    let root = repo_root();
    let mut checked = vec![];
    for relative in [
        "agent/baselines/main.repo-score.json",
        ".jankurai/repo-score.json",
    ] {
        let path = root.join(relative);
        if !path.exists() {
            // The fresh lane artifact only exists after `ci-local.sh audit`.
            continue;
        }
        let report = read_json(&path);
        assert_eq!(errors_for(&report), Vec::<String>::new(), "{relative}");
        assert_eq!(
            report["schema_url"], "schemas/repo-score.schema.json",
            "{relative} must name the committed contract"
        );
        checked.push(relative);
    }
    assert!(
        checked.contains(&"agent/baselines/main.repo-score.json"),
        "the committed ratchet baseline must be validated"
    );
}

/// The repair queue is the artifact that routes a finding to the next agent,
/// inline in the report and one JSON object per line in the lane's JSONL.
#[test]
fn repair_queue_records_carry_the_fields_an_agent_reruns_from() {
    let schemas = schemas();
    let report = read_json(&repo_root().join("agent/baselines/main.repo-score.json"));
    let queue = report["agent_fix_queue"].as_array().expect("queue array");
    assert!(!queue.is_empty(), "the baseline must carry repair routing");
    for (index, record) in queue.iter().enumerate() {
        let mut errors = vec![];
        validate(
            schemas.resolve("repair-queue.schema.json").unwrap(),
            record,
            &format!("agent_fix_queue[{index}]"),
            &schemas,
            &mut errors,
        );
        assert_eq!(errors, Vec::<String>::new());
        for field in ["rule_id", "path", "task", "why"] {
            assert!(record[field].is_string(), "record {index} needs {field}");
        }
    }
}

/// A contract that accepts anything proves nothing: each constraint the schema
/// states has to reject the matching corruption of a real report.
#[test]
fn the_schema_rejects_corrupted_reports() {
    let base = read_json(&repo_root().join("agent/baselines/main.repo-score.json"));
    type Corruption = (&'static str, fn(&mut Value));
    let corruptions: [Corruption; 6] = [
        ("missing required field", |r| {
            r.as_object_mut().unwrap().remove("decision");
        }),
        ("wrong scalar type", |r| r["score"] = Value::from("94")),
        ("out-of-range score", |r| r["score"] = Value::from(140)),
        ("unknown severity", |r| {
            r["findings"][0]["severity"] = Value::from("nagging")
        }),
        ("non-sha256 fingerprint", |r| {
            r["report_fingerprint"] = Value::from("sha256:zz")
        }),
        ("foreign schema_url", |r| {
            r["schema_url"] = Value::from("schemas/other.schema.json")
        }),
    ];
    for (name, corrupt) in corruptions {
        let mut report = base.clone();
        corrupt(&mut report);
        assert!(
            !errors_for(&report).is_empty(),
            "the schema must reject a report with a {name}"
        );
    }
}

/// The validator is only trustworthy while it refuses to skip a constraint it
/// does not implement, so an unknown keyword or a dangling $ref must fail.
#[test]
fn the_validator_fails_closed_on_constraints_it_cannot_check() {
    let schemas = schemas();
    for schema in [
        serde_json::json!({ "type": "object", "oneOf": [{ "type": "string" }] }),
        serde_json::json!({ "$ref": "missing.schema.json" }),
        serde_json::json!({ "$ref": "#/$defs/absent" }),
        serde_json::json!(true),
    ] {
        let mut errors = vec![];
        validate(
            &schema,
            &serde_json::json!({}),
            "probe",
            &schemas,
            &mut errors,
        );
        assert!(
            !errors.is_empty(),
            "unchecked constraint accepted: {schema}"
        );
    }
    let mut errors = vec![];
    validate(
        &serde_json::json!({ "type": "object", "additionalProperties": false }),
        &serde_json::json!({ "stray": 1 }),
        "probe",
        &schemas,
        &mut errors,
    );
    assert_eq!(errors.len(), 1, "closed objects must reject stray fields");
}
