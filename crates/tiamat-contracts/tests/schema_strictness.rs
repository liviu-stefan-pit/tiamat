use serde_json::json;
use tiamat_contracts::{compile_schema, schema_path, validate_json};

#[test]
fn schema_rejects_extra_properties() {
    let schema = compile_schema(&schema_path("intake-manifest.schema.json")).expect("schema");
    let value = json!({
        "schemaVersion": 1,
        "intakeId": "a1b2c3d4-e5f6-4789-a012-3456789abcde",
        "sources": [],
        "projects": [],
        "inventoryArtifact": "artifact",
        "unexpected": true
    });
    assert!(validate_json(&schema, &value).is_err());
}
