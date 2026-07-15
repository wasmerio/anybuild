//! Python-compatible JSON serialization for the committed fixtures.
//!
//! `fixtures/manifest.json` was originally written by Python's
//! `json.dump(..., indent=1)` (which implies `ensure_ascii=True`). The
//! fixture-update tooling rewrites the file in place, so it must reproduce
//! that exact byte format or every regeneration produces a whole-file diff.

use serde_json::ser::PrettyFormatter;
use serde_json::Serializer;
use serde_json::Value;

use serde::Serialize;

/// Serialize like Python's `json.dump(value, f, indent=1)`: one-space
/// indent, `": "` / `","` separators, non-ASCII escaped as `\uXXXX`, and
/// no trailing newline.
pub fn to_python_json(value: &Value) -> String {
    let mut out = Vec::new();
    let formatter = PrettyFormatter::with_indent(b" ");
    let mut serializer = Serializer::with_formatter(&mut out, formatter);
    value
        .serialize(&mut serializer)
        .expect("JSON value serializes");
    let text = String::from_utf8(out).expect("serde_json emits UTF-8");
    escape_non_ascii(&text)
}

/// Replace every non-ASCII char with its `\uXXXX` escape (surrogate pairs
/// above the BMP), matching `ensure_ascii=True`. Non-ASCII bytes can only
/// occur inside string literals, so a whole-document pass is safe.
fn escape_non_ascii(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        if ch.is_ascii() {
            out.push(ch);
        } else {
            let mut units = [0u16; 2];
            for unit in ch.encode_utf16(&mut units) {
                out.push_str(&format!("\\u{unit:04x}"));
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn matches_python_json_dump_indent_1() {
        let value = json!({
            "name": "shipit \u{2014} tool",
            "emoji": "\u{1F680}",
            "nested": {"port": 8080, "flag": null},
            "list": [1, "two"],
            "empty_list": [],
            "empty_obj": {},
        });
        // Python: json.dumps(value, indent=1) (ensure_ascii defaults on).
        let expected = "{\n \"name\": \"shipit \\u2014 tool\",\n \"emoji\": \"\\ud83d\\ude80\",\n \"nested\": {\n  \"port\": 8080,\n  \"flag\": null\n },\n \"list\": [\n  1,\n  \"two\"\n ],\n \"empty_list\": [],\n \"empty_obj\": {}\n}";
        assert_eq!(to_python_json(&value), expected);
    }
}
