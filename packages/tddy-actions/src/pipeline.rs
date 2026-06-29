//! Pipeline action runtime — input mapper → primary → output transform.

use std::process::Stdio;
use std::sync::Arc;

use async_trait::async_trait;
use bytes::Bytes;
use tddy_task::{
    ChannelKind, TaskBody, TaskChannel, TaskContext, TaskHandle, TaskRegistry, TaskStatus,
};

use crate::error::ActionError;
use crate::spec::{ActionSpec, PipelineStage};

/// Spawn a pipeline action as a registered task.
pub struct PipelineRuntime;

impl PipelineRuntime {
    pub async fn spawn(
        registry: &TaskRegistry,
        spec: ActionSpec,
        session_id: impl Into<String>,
    ) -> Result<Arc<TaskHandle>, ActionError> {
        if spec.pipeline.is_none() {
            return Err(ActionError::InvalidSpec(
                "pipeline spec required for PipelineRuntime".into(),
            ));
        }
        spec.validate()?;
        let channels = vec![
            TaskChannel::output_only("stdout", "stdout", ChannelKind::Stdout),
            TaskChannel::output_only("stderr", "stderr", ChannelKind::Stderr),
        ];
        let kind = spec.kind.clone();
        let body = PipelineActionBody { spec };
        Ok(registry.spawn(body, kind, session_id, channels).await)
    }
}

struct PipelineActionBody {
    spec: ActionSpec,
}

#[async_trait]
impl TaskBody for PipelineActionBody {
    async fn run(self: Box<Self>, ctx: TaskContext) -> TaskStatus {
        let pipeline = match self.spec.pipeline {
            Some(p) => p,
            None => {
                return TaskStatus::Failed {
                    message: "missing pipeline".into(),
                };
            }
        };

        let envelope_json = if let Some(mapper) = pipeline.input_mapper {
            match run_stage(&mapper, None).await {
                Ok(out) => out,
                Err(e) => {
                    return TaskStatus::Failed {
                        message: e.to_string(),
                    };
                }
            }
        } else {
            serde_json::json!({ "args": self.spec.command, "env": self.spec.env }).to_string()
        };

        let envelope: serde_json::Value = match serde_json::from_str(&envelope_json) {
            Ok(v) => v,
            Err(e) => {
                return TaskStatus::Failed {
                    message: format!("invalid envelope: {e}"),
                };
            }
        };

        let args: Vec<String> = envelope
            .get("args")
            .and_then(|a| a.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_else(|| self.spec.command.clone());

        let primary = PipelineStage {
            program: args.first().cloned().unwrap_or_default(),
            args: args.into_iter().skip(1).collect(),
            env: self.spec.env.clone(),
        };

        let (stdout, stderr, exit) = match run_stage_capture(&primary).await {
            Ok(v) => v,
            Err(e) => {
                return TaskStatus::Failed {
                    message: e.to_string(),
                };
            }
        };

        if let Some(ch) = ctx.channel("stdout") {
            ch.write(Bytes::from(stdout.clone()));
        }
        if let Some(ch) = ctx.channel("stderr") {
            ch.write(Bytes::from(stderr.clone()));
        }

        let mut result_stdout = stdout.clone();
        if let Some(transform) = pipeline.output_transform {
            match run_stage(&transform, Some(&stdout)).await {
                Ok(out) => result_stdout = out,
                Err(e) => {
                    return TaskStatus::Failed {
                        message: e.to_string(),
                    };
                }
            }
        }

        if let Some(schema) = self
            .spec
            .session
            .as_ref()
            .and_then(|s| s.output_schema.as_ref())
        {
            let parsed: serde_json::Value = match serde_json::from_str(result_stdout.trim()) {
                Ok(v) => v,
                Err(e) => {
                    return TaskStatus::Failed {
                        message: format!("transform output not json: {e}"),
                    };
                }
            };
            if let Err(e) = validate_json_schema(schema, &parsed) {
                return TaskStatus::Failed {
                    message: format!("output_schema validation failed: {e}"),
                };
            }
        }

        ctx.set_result(
            serde_json::json!({
              "exit_code": exit,
              "stdout": stdout,
              "stderr": stderr,
            })
            .to_string(),
        );

        TaskStatus::Completed {
            exit_code: Some(exit),
        }
    }
}

async fn run_stage(stage: &PipelineStage, stdin: Option<&str>) -> Result<String, ActionError> {
    let mut cmd = tokio::process::Command::new(&stage.program);
    cmd.args(&stage.args);
    cmd.env_clear();
    for (k, v) in &stage.env {
        cmd.env(k, v);
    }
    if let Some(input) = stdin {
        cmd.stdin(Stdio::piped());
        cmd.stdout(Stdio::piped());
        let mut child = cmd.spawn().map_err(|e| ActionError::Spawn(e.to_string()))?;
        if let Some(mut stdin_pipe) = child.stdin.take() {
            use tokio::io::AsyncWriteExt;
            stdin_pipe
                .write_all(input.as_bytes())
                .await
                .map_err(ActionError::Io)?;
        }
        let out = child
            .wait_with_output()
            .await
            .map_err(|e| ActionError::Spawn(e.to_string()))?;
        if !out.status.success() {
            return Err(ActionError::Spawn(format!(
                "stage {} exited {}",
                stage.program,
                out.status.code().unwrap_or(-1)
            )));
        }
        return Ok(String::from_utf8_lossy(&out.stdout).into_owned());
    }

    let out = cmd
        .output()
        .await
        .map_err(|e| ActionError::Spawn(e.to_string()))?;
    if !out.status.success() {
        return Err(ActionError::Spawn(format!(
            "stage {} exited {}",
            stage.program,
            out.status.code().unwrap_or(-1)
        )));
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

async fn run_stage_capture(stage: &PipelineStage) -> Result<(String, String, i32), ActionError> {
    let mut cmd = tokio::process::Command::new(&stage.program);
    cmd.args(&stage.args);
    cmd.env_clear();
    for (k, v) in &stage.env {
        cmd.env(k, v);
    }
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
    let out = cmd
        .output()
        .await
        .map_err(|e| ActionError::Spawn(e.to_string()))?;
    Ok((
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
        out.status.code().unwrap_or(-1),
    ))
}

fn validate_json_schema(
    schema: &serde_json::Value,
    instance: &serde_json::Value,
) -> Result<(), String> {
    let compiled = jsonschema::validator_for(schema).map_err(|e| e.to_string())?;
    if let Err(error) = compiled.validate(instance) {
        return Err(error.to_string());
    }
    Ok(())
}
