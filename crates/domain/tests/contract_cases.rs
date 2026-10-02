//! TEST-CONTRACT-FIXTURES / TEST-IPC (native side): every shared case in
//! contracts/fixtures/cases.json must get the oracle's verdict from the native
//! validator, and every valid object sample must survive a typed serde round trip
//! unchanged (casing, nullability, enum spelling).

use gbm_domain::contract::schema::validate_definition;
use gbm_domain::contract::{COMMANDS, FORBIDDEN_COMMANDS, command_spec, typed_roundtrip};
use serde_json::Value;

const CASES: &str = include_str!("../../../contracts/fixtures/cases.json");

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn cases() -> Result<Vec<Value>, Box<dyn std::error::Error>> {
    let doc: Value = serde_json::from_str(CASES)?;
    let list = doc
        .get("cases")
        .and_then(Value::as_array)
        .ok_or("cases.json has no cases array")?;
    Ok(list.clone())
}

#[test]
fn native_validator_matches_oracle_verdicts() -> TestResult {
    let mut mismatches = Vec::new();
    let all = cases()?;
    assert!(all.len() > 1000, "unexpectedly small case set");
    for case in &all {
        let def = case.get("def").and_then(Value::as_str).ok_or("case.def")?;
        let label = case
            .get("label")
            .and_then(Value::as_str)
            .ok_or("case.label")?;
        let expected = case
            .get("valid")
            .and_then(Value::as_bool)
            .ok_or("case.valid")?;
        let value = case.get("value").ok_or("case.value")?;
        let violations = validate_definition(def, value).map_err(|e| format!("{def}: {e:?}"))?;
        if violations.is_empty() != expected {
            mismatches.push(format!(
                "{def} [{label}] expected valid={expected}: {violations:?}"
            ));
        }
    }
    assert!(
        mismatches.is_empty(),
        "{} mismatches:\n{}",
        mismatches.len(),
        mismatches.join("\n")
    );
    Ok(())
}

#[test]
fn valid_samples_round_trip_through_typed_dtos() -> TestResult {
    let mut failures = Vec::new();
    for case in cases()? {
        if case.get("valid") != Some(&Value::Bool(true)) {
            continue;
        }
        let def = case.get("def").and_then(Value::as_str).ok_or("case.def")?;
        let value = case.get("value").ok_or("case.value")?;
        match typed_roundtrip(def, value) {
            Ok(Some(back)) if &back != value => failures.push(format!("{def}: changed to {back}")),
            Ok(_) => {}
            Err(error) => failures.push(format!("{def}: {error}")),
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    Ok(())
}

#[test]
fn command_table_is_allowlisted_and_typed() -> TestResult {
    for spec in &COMMANDS {
        assert!(spec.name.starts_with("gbm_"), "{}", spec.name);
        assert!(!FORBIDDEN_COMMANDS.contains(&spec.name));
        for def in [spec.request_type, spec.response_type] {
            validate_definition(def, &Value::Null).map_err(|e| format!("{def}: {e:?}"))?;
        }
    }
    for name in FORBIDDEN_COMMANDS {
        assert!(command_spec(name).is_none(), "forbidden {name} resolvable");
    }
    let sensitive: Vec<_> = COMMANDS
        .iter()
        .filter(|c| c.sensitive)
        .map(|c| c.name)
        .collect();
    assert_eq!(sensitive, ["gbm_credentials_validate_store"]);
    Ok(())
}
