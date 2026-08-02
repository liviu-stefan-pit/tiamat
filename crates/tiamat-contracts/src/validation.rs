use std::path::Path;

use jsonschema::Validator;
use serde_json::Value;
use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ValidationError {
    #[error("incompatible schema version: expected {expected}, found {found}")]
    IncompatibleSchemaVersion { expected: u32, found: u32 },
    #[error("JSON Schema validation failed: {0}")]
    Schema(String),
    #[error("failed to parse JSON: {0}")]
    Parse(String),
    #[error("failed to load schema from {path}: {reason}")]
    SchemaLoad { path: String, reason: String },
}

pub fn compile_schema(schema_path: &Path) -> Result<Validator, ValidationError> {
    let schema_text =
        std::fs::read_to_string(schema_path).map_err(|err| ValidationError::SchemaLoad {
            path: schema_path.display().to_string(),
            reason: err.to_string(),
        })?;
    let schema_value: Value =
        serde_json::from_str(&schema_text).map_err(|err| ValidationError::SchemaLoad {
            path: schema_path.display().to_string(),
            reason: err.to_string(),
        })?;
    jsonschema::options()
        .should_validate_formats(true)
        .build(&schema_value)
        .map_err(|err| ValidationError::Schema(err.to_string()))
}

pub fn validate_json(validator: &Validator, json: &Value) -> Result<(), ValidationError> {
    if let Err(err) = validator.validate(json) {
        return Err(ValidationError::Schema(err.to_string()));
    }
    Ok(())
}

pub fn validate_json_str(validator: &Validator, json_text: &str) -> Result<Value, ValidationError> {
    let value: Value =
        serde_json::from_str(json_text).map_err(|err| ValidationError::Parse(err.to_string()))?;
    validate_json(validator, &value)?;
    Ok(value)
}

pub fn repo_root() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("contracts crate should live under repo root")
        .to_path_buf()
}

pub fn schema_path(name: &str) -> std::path::PathBuf {
    repo_root().join("schemas").join(name)
}
