//! Cursor CLI adapter: resolve, probe, feature-aware command builder, stream parser.

pub mod builder;
pub mod probe;
pub mod process;
pub mod redaction;
pub mod resolve;
pub mod stream;
pub mod timeouts;
pub mod types;

pub use builder::{build_cursor_command, preview_built_command, BuilderError};
pub use probe::{
    discover_features, invalidate_probe_cache, list_cursor_models, normalize_model_id,
    parse_models_output, parse_version_string, probe_cursor_capability,
    probe_cursor_capability_with_configured, probe_with_deps,
};
pub use process::{run_argv_capture, run_argv_capture_env};
pub use resolve::{
    prepare_hosted_cursor_argv, resolve_cursor_executable,
    resolve_cursor_executable_with_configured, resolve_from_configured_and_env,
    strip_lone_dash_argv, unwind_cursor_launcher, UnwoundCursorRuntime,
};
pub use stream::parse_stream_json;
pub use timeouts::{TimeoutSettings, DEFAULT_ARCHITECT_TIMEOUT_MS, DEFAULT_PHASE_TIMEOUT_MS};
pub use types::*;

pub const MODULE: &str = "cursor";
