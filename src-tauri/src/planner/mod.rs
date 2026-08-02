//! Architect planner: SOL-preferred plan mode, validate/repair, atomic plan files.

mod architect;
mod context;
mod invoke;
mod md_plan;
mod model;
mod parse;
mod persist;
mod prompt;
mod render;
mod types;
mod validate;

pub use architect::{project_graph, run_architect_pipeline, ArchitectPipelineRequest};
pub use context::{package_architect_context, BoundedArchitectContext, CONTEXT_CHAR_BUDGET};
pub use invoke::build_architect_command;
pub use md_plan::{
    compile_master_plan_markdown, extract_master_plan_markdown, synthesize_phase_prompt,
};
pub use model::select_architect_model;
pub use parse::extract_final_json_object;
pub use persist::{
    checkpoint_control_plan, master_plan_md_path, plan_json_path, plan_schedule_md_path,
    write_architect_plan_artifacts, write_plan_artifacts,
};
pub use prompt::{repair_prompt, ARCHITECT_SYSTEM_PROMPT};
pub use render::{
    render_master_plan_markdown, render_plan_schedule_markdown, sha256_hex,
    verify_markdown_projection, verify_schedule_projection,
};
pub use types::*;
pub use validate::validate_plan_json;

pub const MODULE: &str = "planner";
