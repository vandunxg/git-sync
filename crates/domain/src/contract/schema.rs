//! Validator for the JSON Schema subset used by `contracts/dtos.schema.json`.
//!
//! Mirrors `scripts/contract_checks.py` (the blueprint oracle) with two deliberate
//! differences recorded in the P01-T02 report: patterns follow ECMA-262 `$` semantics
//! (no match before a trailing newline), and only the patterns present in the committed
//! schema are supported. An unsupported pattern fails validation instead of passing.

use std::sync::OnceLock;

use serde_json::Value;

const SCHEMA_TEXT: &str = include_str!("../../../../contracts/dtos.schema.json");
const MAX_DEPTH: usize = 80;

/// One validation failure with a JSON path. Paths never include the offending value,
/// so errors are safe to return for secret-bearing requests.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SchemaViolation {
    pub path: String,
    pub reason: &'static str,
}

/// The schema could not be loaded or does not define the requested type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SchemaError {
    Unavailable,
    UnknownDefinition,
}

fn schema() -> Result<&'static Value, SchemaError> {
    static SCHEMA: OnceLock<Option<Value>> = OnceLock::new();
    SCHEMA
        .get_or_init(|| serde_json::from_str(SCHEMA_TEXT).ok())
        .as_ref()
        .ok_or(SchemaError::Unavailable)
}

/// Validates `value` against `$defs/<definition>`. `Ok(vec![])` means valid.
pub fn validate_definition(
    definition: &str,
    value: &Value,
) -> Result<Vec<SchemaViolation>, SchemaError> {
    let root = schema()?;
    let node = root
        .get("$defs")
        .and_then(|defs| defs.get(definition))
        .ok_or(SchemaError::UnknownDefinition)?;
    let mut violations = Vec::new();
    validate_node(value, node, root, "$", 0, &mut violations);
    Ok(violations)
}

fn push(out: &mut Vec<SchemaViolation>, path: &str, reason: &'static str) {
    out.push(SchemaViolation {
        path: path.to_owned(),
        reason,
    });
}

fn type_matches(value: &Value, expected: &str) -> bool {
    match expected {
        "object" => value.is_object(),
        "array" => value.is_array(),
        "string" => value.is_string(),
        "integer" => value.is_i64() || value.is_u64(),
        "number" => value.as_f64().is_some_and(f64::is_finite),
        "boolean" => value.is_boolean(),
        "null" => value.is_null(),
        _ => false,
    }
}

fn validate_node(
    value: &Value,
    node: &Value,
    root: &Value,
    path: &str,
    depth: usize,
    out: &mut Vec<SchemaViolation>,
) {
    if depth > MAX_DEPTH {
        push(out, path, "schema recursion limit");
        return;
    }
    if let Some(reference) = node.get("$ref").and_then(Value::as_str) {
        let target = reference
            .strip_prefix("#/$defs/")
            .and_then(|name| root.get("$defs").and_then(|defs| defs.get(name)));
        match target {
            Some(target) => validate_node(value, target, root, path, depth + 1, out),
            None => push(out, path, "unresolved reference"),
        }
    }
    if let Some(branches) = node.get("anyOf").and_then(Value::as_array) {
        let matched = branches.iter().any(|branch| {
            let mut scratch = Vec::new();
            validate_node(value, branch, root, path, depth + 1, &mut scratch);
            scratch.is_empty()
        });
        if !matched {
            push(out, path, "no anyOf branch matched");
        }
    }
    if let Some(expected) = node.get("type") {
        let ok = match expected {
            Value::String(name) => type_matches(value, name),
            Value::Array(names) => names
                .iter()
                .filter_map(Value::as_str)
                .any(|name| type_matches(value, name)),
            _ => false,
        };
        if !ok {
            push(out, path, "type mismatch");
            return;
        }
    }
    if let Some(constant) = node.get("const")
        && value != constant
    {
        push(out, path, "const mismatch");
    }
    if let Some(members) = node.get("enum").and_then(Value::as_array)
        && !members.iter().any(|member| member == value)
    {
        push(out, path, "enum mismatch");
    }
    match value {
        Value::Object(map) => validate_object(map, node, root, path, depth, out),
        Value::Array(items) => validate_array(items, node, root, path, depth, out),
        Value::String(text) => validate_string(text, node, path, out),
        Value::Number(number) => {
            if let Some(number) = number.as_f64() {
                if node
                    .get("minimum")
                    .and_then(Value::as_f64)
                    .is_some_and(|min| number < min)
                {
                    push(out, path, "below minimum");
                }
                if node
                    .get("maximum")
                    .and_then(Value::as_f64)
                    .is_some_and(|max| number > max)
                {
                    push(out, path, "above maximum");
                }
            }
        }
        _ => {}
    }
}

fn validate_object(
    map: &serde_json::Map<String, Value>,
    node: &Value,
    root: &Value,
    path: &str,
    depth: usize,
    out: &mut Vec<SchemaViolation>,
) {
    let properties = node.get("properties").and_then(Value::as_object);
    if let Some(required) = node.get("required").and_then(Value::as_array) {
        for key in required.iter().filter_map(Value::as_str) {
            if !map.contains_key(key) {
                push(out, &format!("{path}.{key}"), "required");
            }
        }
    }
    let closed = node.get("additionalProperties") == Some(&Value::Bool(false));
    for (key, item) in map {
        let child = format!("{path}.{key}");
        match properties.and_then(|props| props.get(key)) {
            Some(schema) => validate_node(item, schema, root, &child, depth + 1, out),
            None if closed => push(out, &child, "unexpected property"),
            None => {}
        }
    }
}

fn validate_array(
    items: &[Value],
    node: &Value,
    root: &Value,
    path: &str,
    depth: usize,
    out: &mut Vec<SchemaViolation>,
) {
    let len = items.len() as u64;
    if node
        .get("minItems")
        .and_then(Value::as_u64)
        .is_some_and(|min| len < min)
    {
        push(out, path, "too few items");
    }
    if node
        .get("maxItems")
        .and_then(Value::as_u64)
        .is_some_and(|max| len > max)
    {
        push(out, path, "too many items");
    }
    if let Some(item_schema) = node.get("items") {
        for (index, item) in items.iter().enumerate() {
            validate_node(
                item,
                item_schema,
                root,
                &format!("{path}[{index}]"),
                depth + 1,
                out,
            );
        }
    }
}

fn validate_string(text: &str, node: &Value, path: &str, out: &mut Vec<SchemaViolation>) {
    // Length in Unicode scalar values, matching the Python oracle's `len(str)`.
    let len = text.chars().count() as u64;
    if node
        .get("minLength")
        .and_then(Value::as_u64)
        .is_some_and(|min| len < min)
    {
        push(out, path, "too short");
    }
    if node
        .get("maxLength")
        .and_then(Value::as_u64)
        .is_some_and(|max| len > max)
    {
        push(out, path, "too long");
    }
    if let Some(pattern) = node.get("pattern").and_then(Value::as_str) {
        match pattern_matches(pattern, text) {
            Some(true) => {}
            Some(false) => push(out, path, "pattern mismatch"),
            None => push(out, path, "unsupported pattern"),
        }
    }
    if node.get("format").and_then(Value::as_str) == Some("date-time") && !is_rfc3339(text) {
        push(out, path, "invalid timezone-qualified date-time");
    }
}

/// Hand-written matchers for the exact patterns in the committed schema; `None` for any
/// other pattern so a schema change cannot silently weaken native validation.
pub fn pattern_matches(pattern: &str, text: &str) -> Option<bool> {
    match pattern {
        "^[A-Za-z0-9][A-Za-z0-9._:-]{0,127}$" => Some(is_id(text)),
        "^refs/(heads|tags)/.+$" => Some(
            text.strip_prefix("refs/heads/")
                .or_else(|| text.strip_prefix("refs/tags/"))
                // ECMA-262 `.` excludes every line terminator.
                .is_some_and(|rest| {
                    !rest.is_empty() && !rest.contains(['\n', '\r', '\u{2028}', '\u{2029}'])
                }),
        ),
        "^[0-9]+$" => Some(!text.is_empty() && text.bytes().all(|b| b.is_ascii_digit())),
        "^([0-9a-f]{40}|[0-9a-f]{64})$" => Some(
            (text.len() == 40 || text.len() == 64)
                && text.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f')),
        ),
        _ => None,
    }
}

/// `^[A-Za-z0-9][A-Za-z0-9._:-]{0,127}$`
pub fn is_id(text: &str) -> bool {
    let bytes = text.as_bytes();
    match bytes.split_first() {
        Some((first, rest)) => {
            first.is_ascii_alphanumeric()
                && rest.len() <= 127
                && rest
                    .iter()
                    .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b':' | b'-'))
        }
        None => false,
    }
}

/// RFC 3339 `date-time` with mandatory `Z` or numeric offset.
pub fn is_rfc3339(text: &str) -> bool {
    let b = text.as_bytes();
    let digits = |range: std::ops::Range<usize>| -> Option<u32> {
        let slice = b.get(range)?;
        if slice.iter().all(u8::is_ascii_digit) {
            Some(
                slice
                    .iter()
                    .fold(0u32, |acc, d| acc * 10 + u32::from(d - b'0')),
            )
        } else {
            None
        }
    };
    let (Some(year), Some(month), Some(day), Some(hour), Some(minute), Some(second)) = (
        digits(0..4),
        digits(5..7),
        digits(8..10),
        digits(11..13),
        digits(14..16),
        digits(17..19),
    ) else {
        return false;
    };
    if b.get(4) != Some(&b'-')
        || b.get(7) != Some(&b'-')
        || !matches!(b.get(10), Some(b'T' | b't'))
        || b.get(13) != Some(&b':')
        || b.get(16) != Some(&b':')
    {
        return false;
    }
    let leap = (year % 4 == 0 && year % 100 != 0) || year % 400 == 0;
    let days_in_month = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if leap => 29,
        2 => 28,
        _ => return false,
    };
    if day == 0 || day > days_in_month || hour > 23 || minute > 59 || second > 59 {
        return false;
    }
    let mut index = 19;
    if b.get(index) == Some(&b'.') {
        index += 1;
        let start = index;
        while b.get(index).is_some_and(u8::is_ascii_digit) {
            index += 1;
        }
        if index == start {
            return false;
        }
    }
    match b.get(index) {
        Some(b'Z' | b'z') => index + 1 == b.len(),
        Some(b'+' | b'-') => {
            index + 6 == b.len()
                && b.get(index + 3) == Some(&b':')
                && digits(index + 1..index + 3).is_some_and(|h| h <= 23)
                && digits(index + 4..index + 6).is_some_and(|m| m <= 59)
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_schema_pattern_has_a_native_matcher() -> Result<(), SchemaError> {
        fn collect(node: &Value, out: &mut Vec<String>) {
            match node {
                Value::Object(map) => {
                    if let Some(Value::String(pattern)) = map.get("pattern") {
                        out.push(pattern.clone());
                    }
                    map.values().for_each(|child| collect(child, out));
                }
                Value::Array(items) => items.iter().for_each(|child| collect(child, out)),
                _ => {}
            }
        }
        let mut patterns = Vec::new();
        collect(schema()?, &mut patterns);
        assert!(!patterns.is_empty());
        for pattern in patterns {
            assert!(
                pattern_matches(&pattern, "x").is_some(),
                "missing matcher for {pattern}"
            );
        }
        Ok(())
    }

    #[test]
    fn ref_pattern_uses_ecma_dot_semantics() {
        let refs = "^refs/(heads|tags)/.+$";
        assert_eq!(pattern_matches(refs, "refs/tags/v1.0"), Some(true));
        assert_eq!(
            pattern_matches(refs, "refs/heads/feat/\u{1F600}"),
            Some(true)
        );
        for bad in [
            "refs/heads/",
            "refs/remotes/x",
            "refs/heads/a\u{2028}b",
            "refs/heads/a\rb",
        ] {
            assert_eq!(pattern_matches(refs, bad), Some(false), "{bad:?}");
        }
    }

    #[test]
    fn unsupported_pattern_fails_closed() {
        assert_eq!(pattern_matches("^.*$", "anything"), None);
    }

    #[test]
    fn id_rules() {
        assert!(is_id("repo-1:gh.42_x"));
        assert!(is_id(&"a".repeat(128)));
        assert!(!is_id(&"a".repeat(129)));
        assert!(!is_id(""));
        assert!(!is_id("-x"));
        assert!(!is_id("a/b"));
        assert!(!is_id("a\n"));
        assert!(!is_id("ä"));
    }

    #[test]
    fn rfc3339_rules() {
        assert!(is_rfc3339("2026-02-28T23:59:59Z"));
        assert!(is_rfc3339("2024-02-29T00:00:00.5+07:00"));
        assert!(!is_rfc3339("2026-02-29T00:00:00Z"));
        assert!(!is_rfc3339("2026-01-01T00:00:00"));
        assert!(!is_rfc3339("2026-01-01T24:00:00Z"));
        assert!(!is_rfc3339("2026-01-01T00:00:00.Z"));
        assert!(!is_rfc3339("2026-01-01T00:00:00+0700"));
        assert!(!is_rfc3339("2026-01-01T00:00:00Z\n"));
    }

    #[test]
    fn unknown_definition_is_an_error() {
        assert_eq!(
            validate_definition("NoSuchDto", &Value::Null),
            Err(SchemaError::UnknownDefinition)
        );
    }
}
