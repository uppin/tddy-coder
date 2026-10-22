use crate::cli::output_error;
use crate::cli::output_validation_error_with_tip;
use crate::cli::read_input;
use crate::cli::ListSchemasArgs;
use crate::cli::PersistChangesetWorkflowArgs;
use crate::cli::SetSessionContextArgs;
use anyhow::{Context, Result};
use log::info;
use std::path::PathBuf;
use tddy_core::{read_changeset, write_changeset, ChangesetWorkflow};
use tddy_workflow_recipes::{schema, schema_manifest};

pub fn run_list_schemas(_args: ListSchemasArgs) -> Result<()> {
    let goals = schema_manifest::list_registered_goals().context("load schema manifest")?;
    info!(
        target: "tddy_tools::cli",
        "list-schemas ({} goals)",
        goals.len()
    );
    let out = serde_json::json!({ "goals": goals });
    println!("{}", serde_json::to_string(&out)?);
    Ok(())
}

pub fn run_persist_changeset_workflow(args: PersistChangesetWorkflowArgs) -> Result<()> {
    info!(
        target: "tddy_tools::cli",
        "persist-changeset-workflow: session_dir={}",
        args.session_dir.display()
    );
    let json_str = read_input(&args.data, false)?;
    if let Err(errors) = schema::validate_output("changeset-workflow", &json_str) {
        let tip = schema::validation_error_tip("changeset-workflow");
        output_validation_error_with_tip(&errors, &tip);
        std::process::exit(3);
    }
    let workflow: ChangesetWorkflow = match serde_json::from_str(&json_str) {
        Ok(w) => w,
        Err(e) => {
            output_error(&format!("invalid workflow JSON: {}", e), 1);
            unreachable!()
        }
    };
    let mut cs = read_changeset(&args.session_dir)
        .map_err(|e| anyhow::anyhow!("read changeset {}: {}", args.session_dir.display(), e))?;
    cs.workflow = Some(workflow);
    write_changeset(&args.session_dir, &cs)
        .map_err(|e| anyhow::anyhow!("write changeset {}: {}", args.session_dir.display(), e))?;
    Ok(())
}

pub fn run_set_session_context(args: SetSessionContextArgs) -> Result<()> {
    info!(
        target: "tddy_tools::cli",
        "set-session-context: merging payload into workflow session"
    );
    let json_str = read_input(&args.data, false)?;
    let patch: serde_json::Value = match serde_json::from_str(&json_str) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("invalid JSON: {}", e);
            std::process::exit(1);
        }
    };
    if !patch.is_object() {
        eprintln!(
            "invalid payload: expected a JSON object at the top level (non-array, non-scalar)"
        );
        std::process::exit(1);
    }
    let session_dir = std::env::var("TDDY_SESSION_DIR")
        .map_err(|_| anyhow::anyhow!("TDDY_SESSION_DIR is required for set-session-context"))?;
    let session_id = std::env::var("TDDY_WORKFLOW_SESSION_ID").map_err(|_| {
        anyhow::anyhow!("TDDY_WORKFLOW_SESSION_ID is required for set-session-context")
    })?;
    let workflow_dir = PathBuf::from(session_dir).join(".workflow");
    tddy_core::session_context::apply_session_context_merge(&workflow_dir, &session_id, &patch)
}
