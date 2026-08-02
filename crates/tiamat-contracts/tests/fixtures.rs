use std::fs;
use std::path::PathBuf;

use tiamat_contracts::{
    compile_schema, schema_path, validate_json_str, EventEnvelope, IntakeManifest, ProjectPlan,
    ValidationError,
};

fn fixture_root() -> PathBuf {
    tiamat_contracts::repo_root().join("fixtures/contracts/v1")
}

fn read_fixture(rel: &str) -> String {
    fs::read_to_string(fixture_root().join(rel)).expect("fixture should exist")
}

#[test]
fn intake_manifest_round_trip_and_schema_validation() {
    let schema = compile_schema(&schema_path("intake-manifest.schema.json")).expect("schema");
    let json_text = read_fixture("intake-manifest.valid.json");
    let value = validate_json_str(&schema, &json_text).expect("valid fixture");
    let manifest: IntakeManifest =
        serde_json::from_value(value).expect("typed round-trip deserialize");
    manifest.validate_schema_version().expect("version");
    let round_trip = serde_json::to_value(&manifest).expect("serialize");
    validate_json_str(&schema, &round_trip.to_string()).expect("serialized value valid");
}

#[test]
fn event_envelope_round_trip_and_schema_validation() {
    let schema = compile_schema(&schema_path("event-envelope.schema.json")).expect("schema");
    let json_text = read_fixture("event-envelope.valid.json");
    let value = validate_json_str(&schema, &json_text).expect("valid fixture");
    let envelope: EventEnvelope =
        serde_json::from_value(value).expect("typed round-trip deserialize");
    envelope.validate_schema_version().expect("version");
}

#[test]
fn project_plan_round_trip_and_schema_validation() {
    let schema = compile_schema(&schema_path("project-plan.schema.json")).expect("schema");
    let json_text = read_fixture("project-plan.valid.json");
    let value = validate_json_str(&schema, &json_text).expect("valid fixture");
    let plan: ProjectPlan = serde_json::from_value(value).expect("typed round-trip deserialize");
    plan.validate_schema_version().expect("version");
}

#[test]
fn incompatible_intake_fixture_rejected_by_schema() {
    let schema = compile_schema(&schema_path("intake-manifest.schema.json")).expect("schema");
    let json_text = read_fixture("invalid/intake-wrong-schema-version.json");
    let err = validate_json_str(&schema, &json_text).expect_err("wrong version");
    assert!(matches!(err, ValidationError::Schema(_)));
}

#[test]
fn invalid_event_uuid_fixture_rejected_by_schema() {
    let schema = compile_schema(&schema_path("event-envelope.schema.json")).expect("schema");
    let json_text = read_fixture("invalid/event-invalid-uuid.json");
    let err = validate_json_str(&schema, &json_text).expect_err("invalid uuid");
    assert!(matches!(err, ValidationError::Schema(_)));
}

#[test]
fn typed_incompatible_schema_version_rejected() {
    let manifest = IntakeManifest {
        schema_version: 99,
        intake_id: uuid::Uuid::new_v4(),
        sources: vec![],
        projects: vec![],
        inventory_artifact: "artifact".to_string(),
    };
    let err = manifest
        .validate_schema_version()
        .expect_err("version mismatch");
    assert_eq!(
        err,
        ValidationError::IncompatibleSchemaVersion {
            expected: 1,
            found: 99
        }
    );
}

#[test]
fn all_valid_fixtures_validate() {
    let cases = [
        ("intake-manifest.schema.json", "intake-manifest.valid.json"),
        ("event-envelope.schema.json", "event-envelope.valid.json"),
        ("project-plan.schema.json", "project-plan.valid.json"),
        ("phase-result.schema.json", "phase-result.valid.json"),
    ];
    for (schema_name, fixture_name) in cases {
        let schema = compile_schema(&schema_path(schema_name)).expect("schema compile");
        let json_text = read_fixture(fixture_name);
        validate_json_str(&schema, &json_text).expect(fixture_name);
    }
}

#[test]
fn all_invalid_fixtures_rejected() {
    let cases = [
        (
            "intake-manifest.schema.json",
            "invalid/intake-wrong-schema-version.json",
        ),
        (
            "event-envelope.schema.json",
            "invalid/event-invalid-uuid.json",
        ),
    ];
    for (schema_name, fixture_name) in cases {
        let schema = compile_schema(&schema_path(schema_name)).expect("schema compile");
        let json_text = read_fixture(fixture_name);
        assert!(
            validate_json_str(&schema, &json_text).is_err(),
            "{fixture_name} should be rejected"
        );
    }
}
