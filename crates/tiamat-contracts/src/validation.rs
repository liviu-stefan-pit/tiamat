use std::path::{Path, PathBuf};
use std::sync::OnceLock;

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

/// Schema bodies embedded at compile time so packaged validation never needs
/// `schemas/` on disk (or `CARGO_MANIFEST_DIR` path resolution at runtime).
const EMBEDDED_INTAKE_MANIFEST: &str = include_str!("../../../schemas/intake-manifest.schema.json");
const EMBEDDED_EVENT_ENVELOPE: &str = include_str!("../../../schemas/event-envelope.schema.json");
const EMBEDDED_PROJECT_PLAN: &str = include_str!("../../../schemas/project-plan.schema.json");
const EMBEDDED_PHASE_RESULT: &str = include_str!("../../../schemas/phase-result.schema.json");

/// Canonical schema file names recognized for embedded validation.
pub const EMBEDDED_SCHEMA_NAMES: &[&str] = &[
    "intake-manifest.schema.json",
    "event-envelope.schema.json",
    "project-plan.schema.json",
    "phase-result.schema.json",
];

/// Return the compile-time embedded schema text for `name`.
/// Fails closed when the schema is unknown / unavailable.
pub fn embedded_schema_text(name: &str) -> Result<&'static str, ValidationError> {
    let basename = Path::new(name)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or(name);
    match basename {
        "intake-manifest.schema.json" => Ok(EMBEDDED_INTAKE_MANIFEST),
        "event-envelope.schema.json" => Ok(EMBEDDED_EVENT_ENVELOPE),
        "project-plan.schema.json" => Ok(EMBEDDED_PROJECT_PLAN),
        "phase-result.schema.json" => Ok(EMBEDDED_PHASE_RESULT),
        other => Err(ValidationError::SchemaLoad {
            path: other.to_string(),
            reason: "schema not embedded; packaged validation fails closed".into(),
        }),
    }
}

fn compile_schema_text(name: &str, schema_text: &str) -> Result<Validator, ValidationError> {
    let schema_value: Value =
        serde_json::from_str(schema_text).map_err(|err| ValidationError::SchemaLoad {
            path: name.to_string(),
            reason: err.to_string(),
        })?;
    jsonschema::options()
        .should_validate_formats(true)
        .build(&schema_value)
        .map_err(|err| ValidationError::Schema(err.to_string()))
}

/// Compile a schema by canonical file name using the embedded copy.
/// Never reads the filesystem. Fails closed if the name is not embedded.
pub fn compile_schema_named(name: &str) -> Result<Validator, ValidationError> {
    let text = embedded_schema_text(name)?;
    compile_schema_text(name, text)
}

/// Compile a schema for packaged / production validation.
///
/// Uses the embedded schema matching the path's file name. Does **not** read
/// from disk, so packaged apps work without a repo `schemas/` tree.
/// Unknown names fail closed.
pub fn compile_schema(schema_path: &Path) -> Result<Validator, ValidationError> {
    let name = schema_path
        .file_name()
        .and_then(|s| s.to_str())
        .ok_or_else(|| ValidationError::SchemaLoad {
            path: schema_path.display().to_string(),
            reason: "schema path has no file name".into(),
        })?;
    compile_schema_named(name)
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

/// Repo root resolved at compile time for **tests / fixtures only**.
/// Packaged schema validation must use [`compile_schema_named`] / [`compile_schema`].
pub fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("contracts crate should live under repo root")
        .to_path_buf()
}

/// Path under the source-tree `schemas/` directory (tests / tooling).
/// Prefer [`compile_schema_named`] for runtime validation.
pub fn schema_path(name: &str) -> PathBuf {
    repo_root().join("schemas").join(name)
}

/// Cached validators for hot paths (optional convenience).
pub fn cached_schema(name: &'static str) -> Result<&'static Validator, ValidationError> {
    match name {
        "intake-manifest.schema.json" => Ok(intake_validator()),
        "event-envelope.schema.json" => Ok(event_validator()),
        "project-plan.schema.json" => Ok(plan_validator()),
        "phase-result.schema.json" => Ok(phase_result_validator()),
        other => Err(ValidationError::SchemaLoad {
            path: other.to_string(),
            reason: "schema not embedded; packaged validation fails closed".into(),
        }),
    }
}

fn intake_validator() -> &'static Validator {
    static V: OnceLock<Validator> = OnceLock::new();
    V.get_or_init(|| compile_schema_named("intake-manifest.schema.json").expect("embedded schema"))
}

fn event_validator() -> &'static Validator {
    static V: OnceLock<Validator> = OnceLock::new();
    V.get_or_init(|| compile_schema_named("event-envelope.schema.json").expect("embedded schema"))
}

fn plan_validator() -> &'static Validator {
    static V: OnceLock<Validator> = OnceLock::new();
    V.get_or_init(|| compile_schema_named("project-plan.schema.json").expect("embedded schema"))
}

fn phase_result_validator() -> &'static Validator {
    static V: OnceLock<Validator> = OnceLock::new();
    V.get_or_init(|| compile_schema_named("phase-result.schema.json").expect("embedded schema"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn embedded_schemas_compile_without_disk() {
        for name in EMBEDDED_SCHEMA_NAMES {
            compile_schema_named(name).unwrap_or_else(|e| panic!("{name}: {e}"));
        }
    }

    #[test]
    fn compile_schema_uses_filename_not_disk_contents() {
        // Path need not exist — only the basename is used against the embed.
        let phantom = Path::new("Z:/definitely/missing/schemas/phase-result.schema.json");
        assert!(!phantom.exists());
        let validator = compile_schema(phantom).expect("embedded by basename");
        let ok = json!({
            "schemaVersion": 1,
            "phaseId": "P01",
            "status": "passed",
            "summary": "ok",
            "changedFiles": [],
            "evidenceIds": [],
            "acceptanceSatisfied": [],
            "artifacts": [],
            "immutable": true
        });
        validate_json(&validator, &ok).expect("valid payload");
    }

    #[test]
    fn unknown_schema_fails_closed() {
        let err = compile_schema_named("no-such.schema.json").expect_err("fail closed");
        assert!(matches!(err, ValidationError::SchemaLoad { .. }));
        assert!(err.to_string().contains("not embedded"));
    }

    #[test]
    fn embedded_text_non_empty() {
        for name in EMBEDDED_SCHEMA_NAMES {
            let text = embedded_schema_text(name).unwrap();
            assert!(text.contains("\"$schema\""), "{name} looks empty/wrong");
        }
    }
}
