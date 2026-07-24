//! Plan normalization for the golden snapshots.
//!
//! This mirrors `tests/test_plan_snapshots.py` exactly: same shape, same
//! None-dropping rules, same tokenization, same JSON formatting — the
//! snapshots are the cross-implementation contract, byte for byte.

use serde_json::Value;

use crate::Serve;

/// Mirror of `_normalize(serve)`.
pub fn normalize(serve: &Serve) -> Value {
    serde_json::to_value(serve).expect("plan serializes")
}

/// Recursively sort object keys (Python `json.dumps(sort_keys=True)`).
fn sort_keys(value: Value) -> Value {
    match value {
        Value::Object(map) => {
            let mut entries: Vec<(String, Value)> =
                map.into_iter().map(|(k, v)| (k, sort_keys(v))).collect();
            entries.sort_by(|a, b| a.0.cmp(&b.0));
            Value::Object(entries.into_iter().collect())
        }
        Value::Array(items) => Value::Array(items.into_iter().map(sort_keys).collect()),
        other => other,
    }
}

/// Render exactly like `json.dumps(plan, indent=1, sort_keys=True)` plus
/// the tokenization pass and trailing newline from `_evaluate_plan`.
pub fn render(
    serve: &Serve,
    build_path: &std::path::Path,
    anybuild_dir: &std::path::Path,
    workspace: &std::path::Path,
) -> String {
    let value = sort_keys(normalize(serve));
    let mut buf = Vec::new();
    let formatter = serde_json::ser::PrettyFormatter::with_indent(b" ");
    let mut ser = serde_json::Serializer::with_formatter(&mut buf, formatter);
    serde::Serialize::serialize(&value, &mut ser).expect("plan serializes");
    let mut text = String::from_utf8(buf).expect("valid utf8");
    let build_prefix = format!("{}/", build_path.to_string_lossy());
    text = text.replace(&build_prefix, "");
    text = text.replace(&*anybuild_dir.to_string_lossy(), "<ANYBUILD_DIR>");
    text = text.replace(&*workspace.to_string_lossy(), "<WORKSPACE>");
    text.push('\n');
    text
}
