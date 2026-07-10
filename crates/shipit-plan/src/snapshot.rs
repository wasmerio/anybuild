//! Plan normalization for the golden snapshots.
//!
//! This mirrors `tests/test_plan_snapshots.py` exactly: same shape, same
//! None-dropping rules, same tokenization, same JSON formatting — the
//! snapshots are the cross-implementation contract, byte for byte.

use serde_json::{json, Map, Value};

use crate::{Package, RunStep, Serve, Step};

fn path_str(path: &std::path::Path) -> Value {
    Value::String(path.to_string_lossy().into_owned())
}

fn opt<T: Into<Value>>(value: Option<T>) -> Value {
    match value {
        Some(v) => v.into(),
        None => Value::Null,
    }
}

fn str_list(items: &Option<Vec<String>>) -> Value {
    match items {
        Some(items) => json!(items),
        None => Value::Null,
    }
}

fn package_obj(package: &Package) -> Value {
    json!({
        "name": package.name,
        "version": package.version,
        "architecture": package.architecture,
    })
}

fn run_step_obj(step: &RunStep) -> Map<String, Value> {
    let mut map = Map::new();
    map.insert("command".into(), Value::String(step.command.clone()));
    map.insert("inputs".into(), str_list(&step.inputs));
    map.insert("outputs".into(), str_list(&step.outputs));
    map.insert("group".into(), opt(step.group.clone()));
    map
}

/// `dataclasses.asdict(step)` plus the `__type__` tag.
fn norm_step(step: &Step) -> Value {
    let mut map = match step {
        Step::Run(s) => run_step_obj(s),
        Step::Copy(s) => {
            let mut map = Map::new();
            map.insert("source".into(), Value::String(s.source.clone()));
            map.insert("target".into(), Value::String(s.target.clone()));
            map.insert("ignore".into(), str_list(&s.ignore));
            map.insert("base".into(), Value::String(s.base.clone()));
            map
        }
        Step::Env(s) => {
            let mut map = Map::new();
            map.insert("variables".into(), json!(s.variables));
            map
        }
        Step::Path(s) => {
            let mut map = Map::new();
            map.insert("path".into(), Value::String(s.path.clone()));
            map
        }
        Step::Use(s) => {
            let mut map = Map::new();
            map.insert(
                "dependencies".into(),
                Value::Array(s.dependencies.iter().map(package_obj).collect()),
            );
            map
        }
        Step::Workdir(s) => {
            let mut map = Map::new();
            map.insert("path".into(), path_str(&s.path));
            map
        }
        Step::WriteFile(s) => {
            let mut map = Map::new();
            map.insert("path".into(), Value::String(s.path.clone()));
            map.insert("content".into(), Value::String(s.content.clone()));
            map
        }
    };
    map.insert("__type__".into(), Value::String(step.type_name().into()));
    Value::Object(map)
}

/// Drop None-valued object keys recursively (None list *entries* stay
/// visible), mirroring `_jsonable`.
fn drop_nulls(value: Value) -> Value {
    match value {
        Value::Object(map) => Value::Object(
            map.into_iter()
                .filter(|(_, v)| !v.is_null())
                .map(|(k, v)| (k, drop_nulls(v)))
                .collect(),
        ),
        Value::Array(items) => Value::Array(items.into_iter().map(drop_nulls).collect()),
        other => other,
    }
}

/// Mirror of `_normalize(serve)`.
pub fn normalize(serve: &Serve) -> Value {
    let mut map = Map::new();
    map.insert("name".into(), Value::String(serve.name.clone()));
    map.insert("provider".into(), Value::String(serve.provider.clone()));
    map.insert("cwd".into(), opt(serve.cwd.clone()));
    map.insert(
        "build".into(),
        Value::Array(serve.build.iter().map(norm_step).collect()),
    );
    map.insert(
        "deps".into(),
        Value::Array(
            serve
                .deps
                .iter()
                .map(|dep| Value::String(dep.to_string()))
                .collect(),
        ),
    );
    map.insert("commands".into(), json!(serve.commands));
    map.insert(
        "prepare".into(),
        Value::Array(
            serve
                .prepare
                .iter()
                .flatten()
                .map(|s| {
                    let mut obj = run_step_obj(s);
                    obj.insert("__type__".into(), Value::String("RunStep".into()));
                    Value::Object(obj)
                })
                .collect(),
        ),
    );
    map.insert(
        "env".into(),
        json!(serve.env.clone().unwrap_or_default()),
    );
    map.insert(
        "mounts".into(),
        Value::Array(
            serve
                .mounts
                .iter()
                .flatten()
                .map(|m| {
                    json!({
                        "name": m.name,
                        "build_path": path_str(&m.build_path),
                        "serve_path": path_str(&m.serve_path),
                    })
                })
                .collect(),
        ),
    );
    map.insert(
        "volumes".into(),
        Value::Array(
            serve
                .volumes
                .iter()
                .flatten()
                .map(|v| {
                    json!({
                        "name": v.name,
                        "path": path_str(&v.path),
                        "serve_path": path_str(&v.serve_path),
                    })
                })
                .collect(),
        ),
    );
    map.insert(
        "services".into(),
        Value::Array(
            serve
                .services
                .iter()
                .flatten()
                .map(|s| json!({"name": s.name, "provider": s.provider}))
                .collect(),
        ),
    );
    drop_nulls(Value::Object(map))
}

/// Recursively sort object keys (Python `json.dumps(sort_keys=True)`).
fn sort_keys(value: Value) -> Value {
    match value {
        Value::Object(map) => {
            let mut entries: Vec<(String, Value)> = map
                .into_iter()
                .map(|(k, v)| (k, sort_keys(v)))
                .collect();
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
    shipit_dir: &std::path::Path,
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
    text = text.replace(&*shipit_dir.to_string_lossy(), "<SHIPIT_DIR>");
    text = text.replace(&*workspace.to_string_lossy(), "<WORKSPACE>");
    text.push('\n');
    text
}
