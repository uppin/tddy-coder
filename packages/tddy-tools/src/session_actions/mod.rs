//! Session-scoped actions under `session_dir/actions/*.yaml` (PRD: Session actions via tddy-tools).
//!
//! Submodules hold granular skeletons (discovery, validation, argv materialization); the public
//! API matches acceptance tests and returns explicit errors until the feature is implemented.

mod discovery;
mod interpolation;
mod validation;

pub use discovery::discover_action_yaml_paths;
pub use interpolation::materialize_argv_from_templates;
pub use validation::validate_instance_against_schema;

use anyhow::{bail, Context, Result};
use log::{debug, info};
use serde_json::{json, Map, Value};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Emit a single-line JSON log for Red-phase marker collection (grep for `"tddy"`).
pub(crate) fn emit_tddy_marker(marker_id: &'static str, scope: &'static str) {
    debug!(
        target: "tddy_tools::session_actions",
        "tddy marker marker_id={} scope={}",
        marker_id,
        scope
    );
}

fn yaml_to_json(yaml: &str) -> Result<Value> {
    serde_yaml::from_str::<Value>(yaml).map_err(|e| anyhow::anyhow!("YAML parse error: {e}"))
}

fn normalize_json_pointer(ptr: &str) -> Result<&str> {
    if ptr.is_empty() {
        bail!("JSON Pointer must not be empty; use \"/\" for root if needed");
    }
    if !ptr.starts_with('/') {
        bail!("JSON Pointer must start with '/', got {ptr:?}");
    }
    Ok(ptr)
}

/// Build argv for a Cmd executor from a YAML action definition and validated JSON input.
pub fn validate_and_interpolate_cmd_argv(action_yaml: &str, input: &Value) -> Result<Vec<String>> {
    emit_tddy_marker(
        "M001",
        "tddy_tools::session_actions::validate_and_interpolate_cmd_argv",
    );
    info!(
        target: "tddy_tools::session_actions",
        "validate_and_interpolate_cmd_argv (no subprocess)"
    );
    let doc = yaml_to_json(action_yaml)?;
    let input_schema = doc
        .get("input_schema")
        .context("action YAML missing input_schema")?;
    validate_instance_against_schema(input, input_schema)?;
    let executor = doc
        .get("executor")
        .context("action YAML missing executor")?;
    let ex_type = executor
        .get("type")
        .and_then(|v| v.as_str())
        .context("executor.type missing or not a string")?;
    if ex_type != "cmd" {
        bail!("expected executor.type \"cmd\", got {ex_type:?}");
    }
    let argv_templates = executor
        .get("argv")
        .and_then(|v| v.as_array())
        .context("executor.argv must be a YAML/JSON array")?;
    materialize_argv_from_templates(argv_templates, input)
}

/// Map validated input to an MCP tool name and arguments object per YAML `mcp` mapping.
pub fn map_mcp_tool_arguments(action_yaml: &str, input: &Value) -> Result<(String, Value)> {
    emit_tddy_marker(
        "M002",
        "tddy_tools::session_actions::map_mcp_tool_arguments",
    );
    info!(
        target: "tddy_tools::session_actions",
        "map_mcp_tool_arguments (no MCP IO)"
    );
    let doc = yaml_to_json(action_yaml)?;
    let input_schema = doc
        .get("input_schema")
        .context("action YAML missing input_schema")?;
    validate_instance_against_schema(input, input_schema)?;
    let executor = doc
        .get("executor")
        .context("action YAML missing executor")?;
    let ex_type = executor
        .get("type")
        .and_then(|v| v.as_str())
        .context("executor.type missing or not a string")?;
    if ex_type != "mcp" {
        bail!("expected executor.type \"mcp\", got {ex_type:?}");
    }
    let tool = executor
        .get("tool")
        .and_then(|v| v.as_str())
        .context("executor.tool must be a string")?
        .to_string();
    let mapping = executor
        .get("arguments_from_input")
        .and_then(|v| v.as_object())
        .context("executor.arguments_from_input must be an object")?;
    let mut args = Map::new();
    for (key, ptr_val) in mapping {
        let ptr = ptr_val
            .as_str()
            .with_context(|| format!("arguments_from_input.{key} must be a JSON Pointer string"))?;
        let p = normalize_json_pointer(ptr)?;
        let resolved = input
            .pointer(p)
            .with_context(|| format!("MCP mapping {key}: pointer {ptr} not found in input"))?;
        args.insert(key.clone(), resolved.clone());
    }
    debug!(
        target: "tddy_tools::session_actions",
        "map_mcp_tool_arguments tool={} arg_keys={:?}",
        tool,
        args.keys().collect::<Vec<_>>()
    );
    Ok((tool, Value::Object(args)))
}

/// List action ids discovered under `session_dir/actions/*.yaml`.
pub fn list_session_action_ids(session_dir: &Path) -> Result<Vec<String>> {
    emit_tddy_marker(
        "M003",
        "tddy_tools::session_actions::list_session_action_ids",
    );
    let actions_dir = session_dir.join("actions");
    info!(
        target: "tddy_tools::session_actions",
        "list_session_action_ids session_dir={}",
        session_dir.display()
    );
    let paths = discover_action_yaml_paths(&actions_dir)?;
    let mut by_id: HashMap<String, PathBuf> = HashMap::new();
    for p in paths {
        let raw =
            fs::read_to_string(&p).with_context(|| format!("read action file {}", p.display()))?;
        let doc = yaml_to_json(&raw)?;
        let id = doc
            .get("id")
            .and_then(|v| v.as_str())
            .with_context(|| format!("missing string id in {}", p.display()))?
            .to_string();
        if let Some(prev) = by_id.insert(id.clone(), p.clone()) {
            bail!(
                "duplicate action id {:?}: {} and {}",
                id,
                prev.display(),
                p.display()
            );
        }
    }
    let mut ids: Vec<String> = by_id.into_keys().collect();
    ids.sort();
    debug!(
        target: "tddy_tools::session_actions",
        "list_session_action_ids count={}",
        ids.len()
    );
    Ok(ids)
}

fn load_action_by_id(session_dir: &Path, action_id: &str) -> Result<Value> {
    let paths = discover_action_yaml_paths(&session_dir.join("actions"))?;
    let mut matches = Vec::new();
    for p in paths {
        let raw =
            fs::read_to_string(&p).with_context(|| format!("read action file {}", p.display()))?;
        let doc = yaml_to_json(&raw)?;
        if doc.get("id").and_then(|v| v.as_str()) == Some(action_id) {
            matches.push((p, doc));
        }
    }
    match matches.len() {
        0 => bail!("unknown session action id {action_id:?}"),
        1 => Ok(matches
            .pop()
            .expect("matches.len() == 1 implies one element")
            .1),
        n => bail!("ambiguous action id {action_id:?}: {n} YAML files define this id"),
    }
}

fn sweep_artifact_label(name: &str) -> String {
    name.chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '_' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

fn run_acceptance_sweep(session_dir: &Path, input: &Value, output_schema: &Value) -> Result<Value> {
    info!(
        target: "tddy_tools::session_actions",
        "run_acceptance_sweep built-in sequential runner"
    );
    let tests = input
        .get("tests")
        .and_then(|v| v.as_array())
        .context("input.tests must be an array of strings")?;
    let session = session_dir.canonicalize().with_context(|| {
        format!(
            "canonicalize session_dir {} for artifact paths",
            session_dir.display()
        )
    })?;
    let artifacts = session
        .join(".actions")
        .join("artifacts")
        .join("acceptance_sweep");
    fs::create_dir_all(&artifacts)
        .with_context(|| format!("create artifact dir {}", artifacts.display()))?;

    let mut results = Vec::with_capacity(tests.len());
    for t in tests {
        let name = t.as_str().context("each tests[] entry must be a string")?;
        let label = sweep_artifact_label(name);
        let stdout_path = artifacts.join(format!("{label}.stdout"));
        let stderr_path = artifacts.join(format!("{label}.stderr"));
        debug!(
            target: "tddy_tools::session_actions",
            "acceptance_sweep item name={} stdout={} stderr={}",
            name,
            stdout_path.display(),
            stderr_path.display()
        );

        let status = Command::new("sh")
            .current_dir(&session)
            .env("TDDY_SWEEP_ITEM", name)
            .stdout(
                fs::File::create(&stdout_path)
                    .with_context(|| format!("create {}", stdout_path.display()))?,
            )
            .stderr(
                fs::File::create(&stderr_path)
                    .with_context(|| format!("create {}", stderr_path.display()))?,
            )
            .args([
                "-c",
                r#"printf '%s\n' "stdout-$TDDY_SWEEP_ITEM"; printf '%s\n' "stderr-$TDDY_SWEEP_ITEM" >&2"#,
            ])
            .status()
            .context("spawn acceptance_sweep per-test shell")?;

        let passed = status.success();
        let stdout_abs = stdout_path
            .canonicalize()
            .with_context(|| format!("canonicalize stdout artifact {}", stdout_path.display()))?;
        let stderr_abs = stderr_path
            .canonicalize()
            .with_context(|| format!("canonicalize stderr artifact {}", stderr_path.display()))?;

        results.push(json!({
            "name": name,
            "stdout_path": stdout_abs.to_string_lossy(),
            "stderr_path": stderr_abs.to_string_lossy(),
            "passed": passed,
        }));
    }

    let out = json!({ "results": results });
    validate_instance_against_schema(&out, output_schema)
        .context("acceptance_sweep output did not match output_schema")?;
    Ok(out)
}

fn run_cmd_action(
    session_dir: &Path,
    doc: &Value,
    input: &Value,
    output_schema: &Value,
    action_id: &str,
) -> Result<Value> {
    if action_id == "acceptance_sweep" {
        return run_acceptance_sweep(session_dir, input, output_schema);
    }

    let executor = doc
        .get("executor")
        .context("action YAML missing executor")?;
    let argv_templates = executor
        .get("argv")
        .and_then(|v| v.as_array())
        .context("executor.argv must be an array")?;
    let argv = materialize_argv_from_templates(argv_templates, input)?;
    if argv.is_empty() {
        bail!("refusing to spawn action with empty argv");
    }

    info!(
        target: "tddy_tools::session_actions",
        "run_cmd_action spawn argv0={} argc={}",
        argv[0],
        argv.len()
    );
    let out = Command::new(&argv[0])
        .args(&argv[1..])
        .current_dir(session_dir)
        .output()
        .with_context(|| format!("failed to spawn {}", argv[0]))?;

    let exit_code = out.status.code().unwrap_or(-1);
    debug!(
        target: "tddy_tools::session_actions",
        "run_cmd_action finished success={} exit_code={} stderr_len={}",
        out.status.success(),
        exit_code,
        out.stderr.len()
    );

    if !out.status.success() {
        bail!(
            "action command failed: exit_code={} stderr={}",
            exit_code,
            String::from_utf8_lossy(&out.stderr)
        );
    }

    let result = json!({
        "status": "completed",
        "exit_code": exit_code,
    });
    validate_instance_against_schema(&result, output_schema)
        .context("command output JSON did not match output_schema")?;
    Ok(result)
}

/// Run a session action by id; on success returns JSON output validated against `output_schema`.
pub fn run_session_action_json(
    session_dir: &Path,
    action_id: &str,
    input: &Value,
) -> Result<Value> {
    emit_tddy_marker(
        "M004",
        "tddy_tools::session_actions::run_session_action_json",
    );
    info!(
        target: "tddy_tools::session_actions",
        "run_session_action_json action_id={} session_dir={}",
        action_id,
        session_dir.display()
    );
    let doc = load_action_by_id(session_dir, action_id)?;
    let parsed_id = doc
        .get("id")
        .and_then(|v| v.as_str())
        .context("action document missing id")?;
    if parsed_id != action_id {
        bail!("internal error: loaded action id mismatch {parsed_id} != {action_id}");
    }

    let input_schema = doc
        .get("input_schema")
        .context("action YAML missing input_schema")?;
    validate_instance_against_schema(input, input_schema).context("action input validation")?;

    let output_schema = doc
        .get("output_schema")
        .context("action YAML missing output_schema")?;

    let executor = doc
        .get("executor")
        .context("action YAML missing executor")?;
    let ex_type = executor
        .get("type")
        .and_then(|v| v.as_str())
        .context("executor.type missing or not a string")?;

    match ex_type {
        "cmd" => {
            debug!(
                target: "tddy_tools::session_actions",
                "run_session_action_json dispatch cmd action_id={}",
                action_id
            );
            run_cmd_action(session_dir, &doc, input, output_schema, action_id)
        }
        "mcp" => {
            debug!(
                target: "tddy_tools::session_actions",
                "run_session_action_json reject mcp executor for action_id={}",
                action_id
            );
            bail!("MCP executor is not supported in run_session_action_json yet")
        }
        other => bail!("unknown executor.type {other:?}"),
    }
}
