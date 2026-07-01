//! Confined action execution — `spawn_plan` + egress-log or runner PTY bridging.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use bytes::Bytes;
use tddy_actions::{apply_result_kind, ActionSpec, ChannelMode};
use tddy_sandbox::{
    egress_log_path, SandboxError, SandboxHandle, SandboxPlan, SANDBOX_EXEC_STDERR_LOG,
    SANDBOX_EXEC_STDOUT_LOG,
};
use tddy_sandbox_runner::{run_host_relay, HostRelayConfig, NullToolHandler};
use tddy_task::{
    ChannelKind, TaskBody, TaskChannel, TaskContext, TaskHandle, TaskRegistry, TaskStatus,
};
use tokio::sync::mpsc;

use crate::sandbox_plan_builder::{
    build_action_runner_plan, build_action_sandbox_plan, ActionSandboxLayout,
};
use crate::sandbox_session::{
    connect_sandbox_session_client, terminate_sandbox_process, wait_for_sandbox_ready,
};

/// Spawn a sandboxed action as a registered task using platform `spawn_plan`.
pub async fn spawn_sandbox_action(
    registry: &TaskRegistry,
    spec: ActionSpec,
    session_id: impl Into<String>,
) -> Result<Arc<TaskHandle>, String> {
    if spec.sandbox.is_none() {
        return Err("missing sandbox request".to_string());
    }
    spec.validate().map_err(|e| e.to_string())?;

    let session_id = session_id.into();
    let (channels, pty_stdin_rx) = build_channels(&spec);
    let body = SandboxActionBody {
        spec,
        session_id: session_id.clone(),
        pty_stdin_rx,
    };
    let kind = body.spec.kind.clone();
    Ok(registry.spawn(body, kind, session_id, channels).await)
}

fn build_channels(
    spec: &ActionSpec,
) -> (
    Vec<Arc<TaskChannel>>,
    Option<mpsc::UnboundedReceiver<Bytes>>,
) {
    match spec.channel_mode {
        ChannelMode::None => (vec![], None),
        ChannelMode::Combined => (
            vec![TaskChannel::output_only(
                "0",
                "combined",
                ChannelKind::Combined,
            )],
            None,
        ),
        ChannelMode::StdoutStderr => (
            vec![
                TaskChannel::output_only("stdout", "stdout", ChannelKind::Stdout),
                TaskChannel::output_only("stderr", "stderr", ChannelKind::Stderr),
            ],
            None,
        ),
        ChannelMode::Pty => {
            let (ch, stdin_rx) = TaskChannel::pty("0", "pty");
            (
                vec![ch],
                Some(stdin_rx.expect("pty channel must have stdin")),
            )
        }
    }
}

struct SandboxActionBody {
    spec: ActionSpec,
    session_id: String,
    pty_stdin_rx: Option<mpsc::UnboundedReceiver<Bytes>>,
}

#[async_trait]
impl TaskBody for SandboxActionBody {
    async fn run(self: Box<Self>, ctx: TaskContext) -> TaskStatus {
        let sandbox = match self.spec.sandbox.as_ref() {
            Some(s) => s,
            None => {
                return TaskStatus::Failed {
                    message: "missing sandbox request".into(),
                };
            }
        };

        let layout = ActionSandboxLayout::under_output_dir(&sandbox.output_dir, &self.spec.id);

        if self.spec.channel_mode == ChannelMode::Pty {
            return run_pty_confined_action(
                &self.spec,
                &layout,
                &self.session_id,
                sandbox.output_dir.clone(),
                self.pty_stdin_rx,
                &ctx,
            )
            .await;
        }

        let plan = match build_action_sandbox_plan(
            &self.spec,
            &layout,
            &self.session_id,
            self.spec.env.clone(),
        ) {
            Ok(p) => p,
            Err(e) => {
                return TaskStatus::Failed {
                    message: format!("sandbox plan: {e}"),
                };
            }
        };

        let egress_dir = sandbox.output_dir.clone();
        let stdout_ch = ctx.channel("stdout").or_else(|| ctx.channel("0"));
        let stderr_ch = ctx.channel("stderr");
        let combined_ch = if stdout_ch.is_none() && stderr_ch.is_none() {
            ctx.channel("0")
        } else {
            None
        };

        let cancel = ctx.cancel_token();
        let pid_slot = ctx.pid_slot();
        let result = tokio::task::spawn_blocking(move || {
            run_confined_plan(
                plan,
                egress_dir,
                stdout_ch,
                stderr_ch,
                combined_ch,
                cancel,
                pid_slot,
            )
        })
        .await;

        match result {
            Ok(Ok(outcome)) => outcome.into_task_status(&self.spec, &ctx),
            Ok(Err(e)) => TaskStatus::Failed { message: e },
            Err(e) => TaskStatus::Failed {
                message: format!("sandbox task join: {e}"),
            },
        }
    }
}

async fn run_pty_confined_action(
    spec: &ActionSpec,
    layout: &ActionSandboxLayout,
    session_id: &str,
    _egress_dir: PathBuf,
    pty_stdin_rx: Option<mpsc::UnboundedReceiver<Bytes>>,
    ctx: &TaskContext,
) -> TaskStatus {
    let artifacts = match build_action_runner_plan(spec, layout, session_id, spec.env.clone()) {
        Ok(a) => a,
        Err(e) => {
            return TaskStatus::Failed {
                message: format!("sandbox runner plan: {e}"),
            };
        }
    };

    let ready_marker = artifacts.ready_marker.clone();
    let grpc_socket = artifacts.grpc_socket.clone();
    let egress = artifacts.egress_dir.clone();

    let mut handle = match spawn_confined_plan(artifacts.plan) {
        Ok(h) => h,
        Err(e) => {
            return TaskStatus::Failed {
                message: format!("sandbox spawn: {e}"),
            };
        }
    };

    let pid = handle.pid();
    ctx.register_child_pid(pid);

    if let Err(e) = wait_for_sandbox_ready(
        &mut handle,
        &ready_marker,
        Duration::from_secs(120),
        &egress,
    )
    .await
    {
        terminate_sandbox_process(pid);
        ctx.deregister_child_pid(pid);
        return TaskStatus::Failed { message: e };
    }

    let client = match connect_sandbox_session_client(&ready_marker, &grpc_socket).await {
        Ok(c) => c,
        Err(e) => {
            terminate_sandbox_process(pid);
            ctx.deregister_child_pid(pid);
            return TaskStatus::Failed { message: e };
        }
    };

    let pty_ch = match ctx.channel("0") {
        Some(ch) => ch,
        None => {
            terminate_sandbox_process(pid);
            ctx.deregister_child_pid(pid);
            return TaskStatus::Failed {
                message: "missing pty channel".into(),
            };
        }
    };

    let (term_tx, mut term_rx) = mpsc::unbounded_channel::<Bytes>();
    let pty_out = pty_ch.clone();
    tokio::spawn(async move {
        while let Some(chunk) = term_rx.recv().await {
            pty_out.write(chunk);
        }
    });

    let stdin_rx = pty_stdin_rx.unwrap_or_else(|| {
        let (_tx, rx) = mpsc::unbounded_channel();
        rx
    });

    let relay_handle = match run_host_relay(
        client,
        NullToolHandler,
        HostRelayConfig::new(session_id, term_tx),
        stdin_rx,
    )
    .await
    {
        Ok(h) => h,
        Err(e) => {
            terminate_sandbox_process(pid);
            ctx.deregister_child_pid(pid);
            return TaskStatus::Failed { message: e };
        }
    };

    let cancel = ctx.cancel_token();
    let pid_slot = ctx.pid_slot();
    let wait_result = tokio::task::spawn_blocking(move || {
        wait_for_child_exit(&mut handle, cancel, pid, pid_slot)
    })
    .await;

    // Let the relay drain terminal output after the runner exits (gRPC session closes).
    let _ = tokio::time::timeout(Duration::from_secs(2), relay_handle).await;

    match wait_result {
        Ok(Ok(ConfinedOutcome::Cancelled)) => TaskStatus::Cancelled,
        Ok(Ok(ConfinedOutcome::Completed {
            stdout,
            stderr,
            exit_code,
        })) => {
            let stdout_s = String::from_utf8_lossy(&stdout).into_owned();
            let stderr_s = String::from_utf8_lossy(&stderr).into_owned();
            let result_kind = spec.session.as_ref().and_then(|s| s.result_kind.as_deref());
            let record = apply_result_kind(result_kind, &stdout_s, &stderr_s, exit_code);
            ctx.set_result(record.to_string());
            TaskStatus::Completed {
                exit_code: Some(exit_code),
            }
        }
        Ok(Err(e)) => TaskStatus::Failed { message: e },
        Err(e) => TaskStatus::Failed {
            message: format!("sandbox pty wait join: {e}"),
        },
    }
}

fn wait_for_child_exit(
    handle: &mut SandboxHandle,
    cancel: tokio_util::sync::CancellationToken,
    pid: u32,
    pid_slot: std::sync::Arc<std::sync::Mutex<Vec<u32>>>,
) -> Result<ConfinedOutcome, String> {
    loop {
        if cancel.is_cancelled() {
            terminate_sandbox_process(pid);
            pid_slot.lock().unwrap().retain(|&p| p != pid);
            return Ok(ConfinedOutcome::Cancelled);
        }
        if handle.try_exit_diagnostic().is_some() {
            let exit_code = handle
                .child_mut()
                .wait()
                .map_err(|e| e.to_string())?
                .code()
                .unwrap_or(-1);
            pid_slot.lock().unwrap().retain(|&p| p != pid);
            return Ok(ConfinedOutcome::Completed {
                stdout: Vec::new(),
                stderr: Vec::new(),
                exit_code,
            });
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

enum ConfinedOutcome {
    Completed {
        stdout: Vec<u8>,
        stderr: Vec<u8>,
        exit_code: i32,
    },
    Cancelled,
}

impl ConfinedOutcome {
    fn into_task_status(self, spec: &ActionSpec, ctx: &TaskContext) -> TaskStatus {
        match self {
            ConfinedOutcome::Cancelled => TaskStatus::Cancelled,
            ConfinedOutcome::Completed {
                stdout,
                stderr,
                exit_code,
            } => {
                let stdout_s = String::from_utf8_lossy(&stdout).into_owned();
                let stderr_s = String::from_utf8_lossy(&stderr).into_owned();
                let result_kind = spec.session.as_ref().and_then(|s| s.result_kind.as_deref());
                let record = apply_result_kind(result_kind, &stdout_s, &stderr_s, exit_code);
                ctx.set_result(record.to_string());
                TaskStatus::Completed {
                    exit_code: Some(exit_code),
                }
            }
        }
    }
}

fn run_confined_plan(
    plan: SandboxPlan,
    egress_dir: PathBuf,
    stdout_ch: Option<Arc<TaskChannel>>,
    stderr_ch: Option<Arc<TaskChannel>>,
    combined_ch: Option<Arc<TaskChannel>>,
    cancel: tokio_util::sync::CancellationToken,
    pid_slot: std::sync::Arc<std::sync::Mutex<Vec<u32>>>,
) -> Result<ConfinedOutcome, String> {
    let mut handle = spawn_confined_plan(plan).map_err(|e| e.to_string())?;
    let pid = handle.pid();
    pid_slot.lock().unwrap().push(pid);

    let stdout_path = egress_log_path(&egress_dir, SANDBOX_EXEC_STDOUT_LOG);
    let stderr_path = egress_log_path(&egress_dir, SANDBOX_EXEC_STDERR_LOG);
    let mut stdout_off = 0usize;
    let mut stderr_off = 0usize;

    let outcome = loop {
        if cancel.is_cancelled() {
            terminate_sandbox_process(pid);
            pid_slot.lock().unwrap().retain(|&p| p != pid);
            break ConfinedOutcome::Cancelled;
        }

        if handle.try_exit_diagnostic().is_some() {
            let exit_code = handle
                .child_mut()
                .wait()
                .map_err(|e| e.to_string())?
                .code()
                .unwrap_or(-1);
            let _ = pump_log_tail(&stdout_path, stdout_off, &stdout_ch, &combined_ch, false);
            let _ = pump_log_tail(&stderr_path, stderr_off, &stderr_ch, &combined_ch, true);
            let stdout = std::fs::read(&stdout_path).unwrap_or_default();
            let stderr = std::fs::read(&stderr_path).unwrap_or_default();
            pid_slot.lock().unwrap().retain(|&p| p != pid);
            break ConfinedOutcome::Completed {
                stdout,
                stderr,
                exit_code,
            };
        }

        stdout_off = pump_log_tail(&stdout_path, stdout_off, &stdout_ch, &combined_ch, false);
        stderr_off = pump_log_tail(&stderr_path, stderr_off, &stderr_ch, &combined_ch, true);
        std::thread::sleep(Duration::from_millis(50));
    };

    Ok(outcome)
}

fn pump_log_tail(
    path: &Path,
    mut offset: usize,
    primary_ch: &Option<Arc<TaskChannel>>,
    combined_ch: &Option<Arc<TaskChannel>>,
    is_stderr: bool,
) -> usize {
    let data = std::fs::read(path).unwrap_or_default();
    if data.len() > offset {
        let chunk = Bytes::from(data[offset..].to_vec());
        offset = data.len();
        if let Some(ch) = primary_ch {
            ch.write(chunk.clone());
        }
        if let Some(ch) = combined_ch {
            let _ = is_stderr;
            ch.write(chunk);
        }
    }
    offset
}

#[cfg(target_os = "macos")]
fn spawn_confined_plan(plan: SandboxPlan) -> Result<SandboxHandle, SandboxError> {
    tddy_sandbox_darwin::spawn_plan(plan)
}

#[cfg(target_os = "linux")]
fn spawn_confined_plan(plan: SandboxPlan) -> Result<SandboxHandle, SandboxError> {
    tddy_sandbox_cgroups::spawn_plan(plan)
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn spawn_confined_plan(_plan: SandboxPlan) -> Result<SandboxHandle, SandboxError> {
    Err(SandboxError::Unsupported {
        platform: std::env::consts::OS.to_string(),
        message: "sandbox action execution requires macOS or Linux".to_string(),
    })
}
