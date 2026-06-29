//! Process-based action runtime — spawns `tokio::process::Command` from an [`ActionSpec`].

use std::process::Stdio;
use std::sync::Arc;

use async_trait::async_trait;
use bytes::Bytes;
use tddy_task::{
    ChannelKind, TaskBody, TaskChannel, TaskContext, TaskHandle, TaskRegistry, TaskStatus,
};

use crate::result_kind::apply_result_kind;
use crate::spec::{ActionSpec, ChannelMode};

/// Spawn a process action as a registered task.
pub struct ProcessRuntime;

impl ProcessRuntime {
    pub async fn spawn(
        registry: &TaskRegistry,
        spec: ActionSpec,
        session_id: impl Into<String>,
    ) -> Result<Arc<TaskHandle>, crate::ActionError> {
        spec.validate()?;
        let channels = build_channels(&spec);
        let body = ProcessActionBody { spec };
        let kind = body.spec.kind.clone();
        Ok(registry.spawn(body, kind, session_id, channels).await)
    }
}

fn build_channels(spec: &ActionSpec) -> Vec<Arc<TaskChannel>> {
    match spec.channel_mode {
        ChannelMode::None => vec![],
        ChannelMode::Combined => {
            vec![TaskChannel::output_only(
                "0",
                "combined",
                ChannelKind::Combined,
            )]
        }
        ChannelMode::StdoutStderr => vec![
            TaskChannel::output_only("stdout", "stdout", ChannelKind::Stdout),
            TaskChannel::output_only("stderr", "stderr", ChannelKind::Stderr),
        ],
        ChannelMode::Pty => {
            let (ch, _stdin_rx) = TaskChannel::pty("0", "pty");
            vec![ch]
        }
    }
}

struct ProcessActionBody {
    spec: ActionSpec,
}

#[async_trait]
impl TaskBody for ProcessActionBody {
    async fn run(self: Box<Self>, ctx: TaskContext) -> TaskStatus {
        let spec = self.spec;
        if spec.command.is_empty() {
            return TaskStatus::Failed {
                message: "empty command".into(),
            };
        }

        let mut cmd = tokio::process::Command::new(&spec.command[0]);
        cmd.args(&spec.command[1..]);
        for (k, v) in &spec.env {
            cmd.env(k, v);
        }
        if let Some(ref wd) = spec.working_dir {
            cmd.current_dir(wd);
        }
        cmd.stdin(Stdio::piped());
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

        let mut child = match cmd.spawn() {
            Ok(c) => c,
            Err(e) => {
                return TaskStatus::Failed {
                    message: format!("spawn failed: {e}"),
                };
            }
        };

        if let Some(pid) = child.id() {
            ctx.register_child_pid(pid);
        }

        let stdout_ch = ctx.channel("stdout").or_else(|| ctx.channel("0"));
        let stderr_ch = ctx.channel("stderr");
        let combined_ch = if stdout_ch.is_none() && stderr_ch.is_none() {
            ctx.channel("0")
        } else {
            None
        };

        let mut stdout = child.stdout.take();
        let mut stderr = child.stderr.take();

        let cancel = ctx.cancel_token();
        let status = tokio::select! {
          _ = cancel.cancelled() => {
            let _ = child.kill().await;
            return TaskStatus::Cancelled;
          }
          out = async {
            use tokio::io::AsyncReadExt;
            let mut buf = Vec::new();
            if let Some(mut s) = stdout.take() {
              let _ = s.read_to_end(&mut buf).await;
            }
            buf
          } => out,
        };

        let err_bytes = {
            use tokio::io::AsyncReadExt;
            let mut buf = Vec::new();
            if let Some(mut s) = stderr.take() {
                let _ = s.read_to_end(&mut buf).await;
            }
            buf
        };

        if let Some(ch) = &stdout_ch {
            ch.write(Bytes::from(status.clone()));
        }
        if let Some(ch) = &stderr_ch {
            ch.write(Bytes::from(err_bytes.clone()));
        }
        if let Some(ch) = &combined_ch {
            let mut combined = status.clone();
            combined.extend_from_slice(&err_bytes);
            ch.write(Bytes::from(combined));
        }

        let exit = match child.wait().await {
            Ok(s) => s.code().unwrap_or(-1),
            Err(e) => {
                return TaskStatus::Failed {
                    message: format!("wait failed: {e}"),
                };
            }
        };

        if let Some(pid) = child.id() {
            ctx.deregister_child_pid(pid);
        }

        let stdout_s = String::from_utf8_lossy(&status).into_owned();
        let stderr_s = String::from_utf8_lossy(&err_bytes).into_owned();
        let result_kind = spec.session.as_ref().and_then(|s| s.result_kind.as_deref());
        let record = apply_result_kind(result_kind, &stdout_s, &stderr_s, exit);
        ctx.set_result(record.to_string());

        TaskStatus::Completed {
            exit_code: Some(exit),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use tddy_task::TaskRegistry;

    #[tokio::test]
    async fn process_action_runs_echo_and_captures_stdout() {
        // Given
        let registry = TaskRegistry::new();
        let spec = ActionSpec {
            id: "echo-test".into(),
            kind: "test".into(),
            command: vec!["echo".into(), "hello-actions".into()],
            inputs: vec![],
            outputs: vec![],
            env: BTreeMap::new(),
            working_dir: None,
            channel_mode: ChannelMode::StdoutStderr,
            sandbox: None,
            session: None,
            pipeline: None,
        };

        // When
        let handle = ProcessRuntime::spawn(&registry, spec, "test-session")
            .await
            .expect("spawn");

        // Then
        wait_terminal(&handle).await;
        assert!(matches!(
            handle.status(),
            TaskStatus::Completed { exit_code: Some(0) }
        ));
        let stdout = handle
            .channel("stdout")
            .expect("stdout channel")
            .replay_capture();
        assert!(String::from_utf8_lossy(&stdout).contains("hello-actions"));
    }

    async fn wait_terminal(handle: &TaskHandle) {
        let mut rx = handle.status_watch();
        loop {
            if rx.borrow().is_terminal() {
                return;
            }
            if rx.changed().await.is_err() {
                return;
            }
        }
    }
}
