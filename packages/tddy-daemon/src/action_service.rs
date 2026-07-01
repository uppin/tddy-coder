//! ActionServiceImpl — maps the `actions.ActionService` RPC trait to `tddy_actions` runtimes.

use std::collections::BTreeMap;
use std::path::PathBuf;

use async_trait::async_trait;
use serde_json::Value;
use tddy_actions::{
    action_spec_from_session_manifest, ActionCatalog, ProcessRuntime, SessionManifestFields,
};
use tddy_actions::{ActionSpec, ChannelMode};
use tddy_rpc::{Request, Response, Status};
use tddy_service::proto::actions::{
    ActionKindInfo, ActionService, ActionTaskInfo, GetActionRequest, GetActionResponse,
    ListActionKindsRequest, ListActionKindsResponse, StartActionRequest, StartActionResponse,
};
use tddy_task::{TaskRegistry, TaskStatus};

use crate::sandbox_runtime::{attach_sandbox_request, SandboxRuntime};
use crate::task_service::SessionUserResolver;

/// Implementation of the `actions.ActionService` RPC service.
pub struct ActionServiceImpl {
    registry: TaskRegistry,
    catalog: ActionCatalog,
    user_resolver: SessionUserResolver,
}

impl ActionServiceImpl {
    pub fn new(
        registry: TaskRegistry,
        catalog: ActionCatalog,
        user_resolver: SessionUserResolver,
    ) -> Self {
        Self {
            registry,
            catalog,
            user_resolver,
        }
    }

    fn authenticate(&self, token: &str) -> Result<String, Status> {
        (self.user_resolver)(token)
            .ok_or_else(|| Status::unauthenticated("invalid or expired session token"))
    }
}

fn kind_metadata(kind: &str) -> ActionKindInfo {
    let (summary, has_input_schema) = match kind {
        "claude-cli" => ("Interactive Claude CLI session (PTY)", true),
        "bash" => ("Run a shell command", true),
        "tddy-coder" => ("Run tddy-coder planning or workflow", true),
        "build-action" => ("Run a tddy-build action", true),
        "session-action" => ("Run a session manifest action", true),
        "pipeline-action" => ("Run a multi-stage pipeline action", true),
        "execute_tool" => ("Run a fast tool invocation", true),
        _ => ("Unknown action kind", false),
    };
    ActionKindInfo {
        kind: kind.to_string(),
        summary: summary.to_string(),
        has_input_schema,
    }
}

fn task_status_to_proto(status: &TaskStatus) -> i32 {
    match status {
        TaskStatus::Pending => 1,
        TaskStatus::Running => 2,
        TaskStatus::Completed { .. } => 3,
        TaskStatus::Failed { .. } => 4,
        TaskStatus::Cancelled => 5,
    }
}

fn handle_to_action_info(handle: &tddy_task::TaskHandle) -> ActionTaskInfo {
    let status = handle.status();
    let exit_code = match &status {
        TaskStatus::Completed { exit_code } => exit_code.unwrap_or(0),
        _ => 0,
    };
    let error_message = match &status {
        TaskStatus::Failed { message } => message.clone(),
        _ => String::new(),
    };
    let result_json = handle
        .result_json
        .lock()
        .unwrap()
        .clone()
        .unwrap_or_default();
    ActionTaskInfo {
        task_id: handle.id.0.clone(),
        kind: handle.kind.clone(),
        session_id: handle.session_id.clone(),
        status: task_status_to_proto(&status),
        exit_code,
        error_message,
        created_unix_ms: handle.created_unix_ms,
        result_json,
    }
}

fn parse_params_json(params_json: &str) -> Result<Value, Status> {
    if params_json.trim().is_empty() {
        return Ok(Value::Object(serde_json::Map::new()));
    }
    serde_json::from_str(params_json)
        .map_err(|e| Status::invalid_argument(format!("invalid params_json: {e}")))
}

fn string_field(params: &Value, key: &str) -> Result<String, Status> {
    params
        .get(key)
        .and_then(|v| v.as_str())
        .map(str::to_string)
        .ok_or_else(|| Status::invalid_argument(format!("params require string field '{key}'")))
}

fn optional_string(params: &Value, key: &str) -> Option<String> {
    params.get(key).and_then(|v| v.as_str()).map(str::to_string)
}

fn optional_path(params: &Value, key: &str) -> Option<PathBuf> {
    params.get(key).and_then(|v| v.as_str()).map(PathBuf::from)
}

fn parse_extra_read_paths(params: &Value) -> Result<Vec<PathBuf>, Status> {
    let Some(value) = params.get("extra_read_paths") else {
        return Ok(vec![]);
    };
    let arr = value.as_array().ok_or_else(|| {
        Status::invalid_argument("params.extra_read_paths must be a string array")
    })?;
    arr.iter()
        .map(|v| {
            v.as_str()
                .map(PathBuf::from)
                .ok_or_else(|| Status::invalid_argument("extra_read_paths entries must be strings"))
        })
        .collect()
}

fn parse_channel_mode(params: &Value) -> ChannelMode {
    match params.get("channel_mode").and_then(|v| v.as_str()) {
        Some("pty") => ChannelMode::Pty,
        Some("stdout_stderr") => ChannelMode::StdoutStderr,
        Some("none") => ChannelMode::None,
        _ => ChannelMode::Combined,
    }
}

fn parse_command_argv(params: &Value) -> Result<Vec<String>, Status> {
    match params.get("command") {
        Some(Value::String(script)) => Ok(vec!["sh".into(), "-c".into(), script.clone()]),
        Some(Value::Array(items)) => {
            let argv: Vec<String> = items
                .iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect();
            if argv.is_empty() {
                return Err(Status::invalid_argument(
                    "params.command array must not be empty",
                ));
            }
            Ok(argv)
        }
        _ => Err(Status::invalid_argument(
            "params require 'command' as a string or string array",
        )),
    }
}

fn spec_for_bash(params: &Value) -> Result<ActionSpec, Status> {
    let id = optional_string(params, "id").unwrap_or_else(|| "bash-action".to_string());
    let command = parse_command_argv(params)?;
    Ok(ActionSpec {
        id,
        kind: "bash".to_string(),
        command,
        inputs: vec![],
        outputs: vec![],
        env: BTreeMap::new(),
        working_dir: optional_path(params, "working_dir"),
        channel_mode: parse_channel_mode(params),
        sandbox: None,
        session: None,
        pipeline: None,
    })
}

fn spec_for_tddy_coder(params: &Value) -> Result<ActionSpec, Status> {
    let id = optional_string(params, "id").unwrap_or_else(|| "tddy-coder-action".to_string());
    let goal = string_field(params, "goal")?;
    let binary =
        optional_string(params, "coder_binary").unwrap_or_else(|| "tddy-coder".to_string());
    let mut command = vec![binary, "--goal".to_string(), goal];
    if let Some(agent) = optional_string(params, "agent") {
        command.push("--agent".to_string());
        command.push(agent);
    }
    if let Some(recipe) = optional_string(params, "recipe") {
        command.push("--recipe".to_string());
        command.push(recipe);
    }
    if let Some(output_dir) = optional_string(params, "output_dir") {
        command.push("--output-dir".to_string());
        command.push(output_dir);
    }
    if let Some(feature) = optional_string(params, "feature") {
        command.push("--prompt".to_string());
        command.push(feature);
    }
    Ok(ActionSpec {
        id,
        kind: "tddy-coder".to_string(),
        command,
        inputs: vec![],
        outputs: vec![],
        env: BTreeMap::new(),
        working_dir: optional_path(params, "working_dir"),
        channel_mode: ChannelMode::Combined,
        sandbox: None,
        session: None,
        pipeline: None,
    })
}

fn spec_for_session_action(params: &Value) -> Result<ActionSpec, Status> {
    let fields = SessionManifestFields {
        version: params.get("version").and_then(|v| v.as_u64()).unwrap_or(1) as u32,
        id: string_field(params, "id")?,
        summary: optional_string(params, "summary").unwrap_or_default(),
        architecture: optional_string(params, "architecture").unwrap_or_default(),
        command: match params.get("command") {
            Some(Value::Array(items)) => items
                .iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect(),
            _ => {
                return Err(Status::invalid_argument(
                    "session-action requires command array",
                ))
            }
        },
        input_schema: params.get("input_schema").cloned(),
        output_schema: params.get("output_schema").cloned(),
        result_kind: optional_string(params, "result_kind"),
        output_path_arg: optional_string(params, "output_path_arg"),
        working_dir: optional_path(params, "working_dir"),
    };
    Ok(action_spec_from_session_manifest(fields))
}

fn spec_from_start_request(
    kind: &str,
    params_json: &str,
    sandbox: bool,
) -> Result<ActionSpec, Status> {
    let params = parse_params_json(params_json)?;
    let mut spec = match kind {
        "bash" => spec_for_bash(&params)?,
        "tddy-coder" => spec_for_tddy_coder(&params)?,
        "session-action" => spec_for_session_action(&params)?,
        other => {
            return Err(Status::invalid_argument(format!(
                "StartAction does not support kind '{other}' yet"
            )));
        }
    };
    if sandbox {
        let output_dir = optional_path(&params, "output_dir")
            .ok_or_else(|| Status::invalid_argument("sandbox actions require params.output_dir"))?;
        let recipe = optional_string(&params, "sandbox_recipe");
        let extra_read_paths = parse_extra_read_paths(&params)?;
        let stdin = optional_string(&params, "stdin");
        spec = attach_sandbox_request(spec, output_dir, recipe, extra_read_paths, stdin);
    }
    spec.validate()
        .map_err(|e| Status::invalid_argument(e.to_string()))?;
    Ok(spec)
}

#[async_trait]
impl ActionService for ActionServiceImpl {
    async fn list_action_kinds(
        &self,
        request: Request<ListActionKindsRequest>,
    ) -> Result<Response<ListActionKindsResponse>, Status> {
        let req = request.into_inner();
        let _user = self.authenticate(&req.session_token)?;
        let kinds = self
            .catalog
            .list_kinds()
            .into_iter()
            .map(kind_metadata)
            .collect();
        Ok(Response::new(ListActionKindsResponse { kinds }))
    }

    async fn start_action(
        &self,
        request: Request<StartActionRequest>,
    ) -> Result<Response<StartActionResponse>, Status> {
        let req = request.into_inner();
        let _user = self.authenticate(&req.session_token)?;
        if req.session_id.trim().is_empty() {
            return Err(Status::invalid_argument("session_id must not be empty"));
        }
        if req.kind.trim().is_empty() {
            return Err(Status::invalid_argument("kind must not be empty"));
        }
        if !self.catalog.list_kinds().contains(&req.kind.as_str()) {
            return Err(Status::invalid_argument(format!(
                "unknown action kind: {}",
                req.kind
            )));
        }

        let spec = spec_from_start_request(&req.kind, &req.params_json, req.sandbox)?;
        let handle = if req.sandbox {
            SandboxRuntime::spawn(&self.registry, spec, req.session_id)
                .await
                .map_err(|e| Status::internal(e.to_string()))?
        } else {
            ProcessRuntime::spawn(&self.registry, spec, req.session_id)
                .await
                .map_err(|e| Status::internal(e.to_string()))?
        };
        Ok(Response::new(StartActionResponse {
            task_id: handle.id.0.clone(),
            ok: true,
            message: String::new(),
        }))
    }

    async fn get_action(
        &self,
        request: Request<GetActionRequest>,
    ) -> Result<Response<GetActionResponse>, Status> {
        let req = request.into_inner();
        let _user = self.authenticate(&req.session_token)?;
        let task_id = tddy_task::TaskId(req.task_id);
        let handle = self
            .registry
            .get(&task_id)
            .await
            .ok_or_else(|| Status::not_found("task not found"))?;
        Ok(Response::new(GetActionResponse {
            action: Some(handle_to_action_info(&handle)),
        }))
    }
}
