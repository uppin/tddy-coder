//! The driver's one session: launched through the daemon's own launch code, then read, typed into
//! and stopped.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use tddy_actions::tool::TemplateParams;
use tddy_agent_launch::remote_agent_launch::{
    launch_on_target, RemoteLaunch, REMOTE_AGENT_PID_FILE,
};
use tddy_agent_launch::spec_launch::{launch_host_pty, HostAgent, HostPtySession};
use tddy_cli_sessions::cli_session_manager::pty_handle::PtyHandle;
use tddy_cli_sessions::cli_session_manager::CliSessionManager;
use tddy_coding_agent_specs::slots::RenderedSlots;
use tddy_remote_shell::{ExecRequest, RemoteShell};
use tddy_task::{TaskStatus, TextGrid};

/// The session type the driver registers its terminal under.
const SESSION_TYPE: &str = "session-driver";
/// How often a wait reads the screen again.
const POLL: Duration = Duration::from_millis(200);
/// How long a stop waits for the terminal to end.
const STOP_GRACE: Duration = Duration::from_secs(10);
/// How many of the screen's last non-empty lines a failure quotes.
const LAST_OUTPUT_LINES: usize = 12;

/// Everything a launch of the session needs, fixed by the driver's arguments.
pub struct LaunchSpec {
    pub agent: HostAgent,
    pub params: TemplateParams,
    /// The session's worktree on its target: the agent's working directory.
    pub worktree: PathBuf,
    /// The session's directory on this machine: the working directory of a remote target's client.
    pub session_dir: PathBuf,
    /// The shell that reaches the target; `None` runs the agent on this machine.
    pub shell: Option<Arc<dyn RemoteShell>>,
    /// Set on the agent on top of its spec's environment.
    pub env: Vec<(String, String)>,
    pub session_id: String,
    pub model: String,
    /// How long the agent must stay up for its start to count.
    pub startup_window: Duration,
    /// The terminal's size, which the screen is read at.
    pub rows: u16,
    pub cols: u16,
}

enum State {
    NotStarted,
    Started(Arc<PtyHandle>),
    /// The launch failed; what it said.
    Failed(String),
}

/// Whether a wait saw what it waited for, and the screen it last read.
pub struct Waited {
    pub matched: bool,
    pub exited: bool,
    pub grid: Option<TextGrid>,
}

/// The one session a driver runs.
pub struct DriverSession {
    spec: LaunchSpec,
    manager: Arc<CliSessionManager>,
    state: tokio::sync::Mutex<State>,
}

impl DriverSession {
    pub fn new(spec: LaunchSpec) -> Self {
        Self {
            spec,
            manager: Arc::new(CliSessionManager::new()),
            state: tokio::sync::Mutex::new(State::NotStarted),
        }
    }

    /// Launch the session, refused when it was launched before. Answers how long the launch took,
    /// or what it failed with: for an agent that exited during the startup window, its last output.
    pub async fn start(&self) -> Result<Duration, String> {
        let mut state = self.state.lock().await;
        if !matches!(*state, State::NotStarted) {
            return Err("the session was already started; a driver runs one session".to_string());
        }
        let started = Instant::now();
        match self.launch().await {
            Ok(handle) => {
                *state = State::Started(handle);
                Ok(started.elapsed())
            }
            Err(failure) => {
                *state = State::Failed(failure.clone());
                Err(failure)
            }
        }
    }

    async fn launch(&self) -> Result<Arc<PtyHandle>, String> {
        let spec = &self.spec;
        let session = HostPtySession {
            manager: &self.manager,
            session_id: &spec.session_id,
            session_type: SESSION_TYPE,
            model: &spec.model,
            env: spec.env.clone(),
            os_user: None,
        };
        let handle = match &spec.shell {
            Some(shell) => {
                let launched = launch_on_target(
                    &spec.agent,
                    &spec.params,
                    &RenderedSlots::default(),
                    RemoteLaunch {
                        shell: Arc::clone(shell),
                        worktree: &spec.worktree,
                        session_dir: &spec.session_dir,
                        pointer: None,
                        startup_window: spec.startup_window,
                    },
                    session,
                    None,
                )
                .await
                .map_err(|error| error.to_string())?;
                launched.handle.resize(spec.rows, spec.cols);
                launched.handle
            }
            None => {
                let handle = launch_host_pty(
                    &spec.agent,
                    &spec.params,
                    &RenderedSlots::default(),
                    &spec.worktree,
                    session,
                    None,
                )
                .await
                .map_err(|error| error.to_string())?;
                handle.resize(spec.rows, spec.cols);
                // The remote launch judges the startup window itself; a local one is judged here,
                // the same way: an agent that ends within it failed to start.
                wait_out_the_startup_window(spec, &handle).await;
                if *handle.pty_done.borrow() {
                    return Err(format!(
                        "the agent exited while starting ({}): {}",
                        exit_description(&handle.task_status()),
                        last_output(&handle)
                    ));
                }
                handle
            }
        };
        Ok(handle)
    }

    /// The session's state as `session_status` answers it.
    pub async fn status(&self) -> serde_json::Value {
        match &*self.state.lock().await {
            State::NotStarted => serde_json::json!({ "state": "not_started" }),
            State::Failed(failure) => {
                serde_json::json!({ "state": "exited", "failure": failure })
            }
            State::Started(handle) if *handle.pty_done.borrow() => serde_json::json!({
                "state": "exited",
                "exit": exit_description(&handle.task_status()),
                "last_output": last_output(handle),
            }),
            State::Started(_) => serde_json::json!({ "state": "running" }),
        }
    }

    async fn handle(&self) -> Result<Arc<PtyHandle>, String> {
        match &*self.state.lock().await {
            State::Started(handle) => Ok(Arc::clone(handle)),
            State::NotStarted => {
                Err("the session has not been started: call start_session".to_string())
            }
            State::Failed(failure) => Err(format!("the session failed to start: {failure}")),
        }
    }

    /// What the session's terminal shows now.
    pub async fn grid(&self) -> Result<TextGrid, String> {
        let handle = self.handle().await?;
        screen(&handle).ok_or_else(|| "the terminal has not reported its size yet".to_string())
    }

    /// Type `bytes` into the session's terminal, refused once the session has ended.
    pub async fn send(&self, bytes: Vec<u8>) -> Result<(), String> {
        let handle = self.handle().await?;
        if *handle.pty_done.borrow() {
            return Err("the session has exited; nothing was sent".to_string());
        }
        handle.send_input(bytes.into(), 0);
        Ok(())
    }

    /// Wait until the screen holds `text`, with whitespace ignored on both sides — a full-screen
    /// TUI places words by moving the cursor, so the gaps between them are not spaces — or until
    /// `timeout` passes or the session ends.
    pub async fn wait_for(&self, text: &str, timeout: Duration) -> Result<Waited, String> {
        let handle = self.handle().await?;
        let wanted: String = text.split_whitespace().collect();
        let deadline = Instant::now() + timeout;
        loop {
            let grid = screen(&handle);
            let shown: String = grid
                .as_ref()
                .map(|grid| grid.lines.concat())
                .unwrap_or_default()
                .split_whitespace()
                .collect();
            let matched = shown.contains(&wanted);
            let exited = *handle.pty_done.borrow();
            if matched || exited || Instant::now() >= deadline {
                return Ok(Waited {
                    matched,
                    exited,
                    grid,
                });
            }
            tokio::time::sleep(POLL).await;
        }
    }

    /// Stop the session: its agent on a remote target (by the pid the launch recorded there), then
    /// its terminal here, waiting for the terminal to end.
    pub async fn stop(&self) -> Result<(), String> {
        let handle = self.handle().await?;
        if let Some(shell) = &self.spec.shell {
            let pid_file = self.spec.worktree.join(REMOTE_AGENT_PID_FILE);
            let kill = format!(
                "pid=$(cat '{}' 2>/dev/null) && kill \"$pid\" 2>/dev/null; true",
                pid_file.to_string_lossy().replace('\'', "'\"'\"'")
            );
            shell.exec(ExecRequest::new(kill)).await.map_err(|error| {
                format!("the agent on the target could not be stopped: {error}")
            })?;
        }
        self.manager.stop_session(&self.spec.session_id).await;
        let deadline = Instant::now() + STOP_GRACE;
        while !*handle.pty_done.borrow() {
            if Instant::now() >= deadline {
                return Err(format!(
                    "the session's terminal did not end within {STOP_GRACE:?}"
                ));
            }
            tokio::time::sleep(POLL).await;
        }
        Ok(())
    }
}

async fn wait_out_the_startup_window(spec: &LaunchSpec, handle: &Arc<PtyHandle>) {
    let deadline = Instant::now() + spec.startup_window;
    while Instant::now() < deadline && !*handle.pty_done.borrow() {
        tokio::time::sleep(POLL.min(spec.startup_window)).await;
    }
}

/// The screen of `handle`'s terminal, once its size is known.
fn screen(handle: &PtyHandle) -> Option<TextGrid> {
    handle.capture.lock().ok()?.text_grid()
}

/// The last non-empty lines `handle`'s terminal showed: its screen, or the raw output when the
/// screen has no size yet.
fn last_output(handle: &PtyHandle) -> String {
    let lines: Vec<String> = match screen(handle) {
        Some(grid) => grid.lines,
        None => handle
            .capture
            .lock()
            .map(|capture| {
                String::from_utf8_lossy(capture.buffered_bytes())
                    .lines()
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default(),
    };
    let lines: Vec<&str> = lines
        .iter()
        .map(|line| line.trim_end())
        .filter(|line| !line.is_empty())
        .collect();
    lines[lines.len().saturating_sub(LAST_OUTPUT_LINES)..].join("\n")
}

/// How the terminal's process ended, in words.
fn exit_description(status: &TaskStatus) -> String {
    match status {
        TaskStatus::Completed {
            exit_code: Some(code),
        } => format!("exit status {code}"),
        TaskStatus::Completed { exit_code: None } => "ended by a signal".to_string(),
        TaskStatus::Failed { message } => format!("failed: {message}"),
        TaskStatus::Cancelled => "stopped".to_string(),
        TaskStatus::Pending | TaskStatus::Running => "ended".to_string(),
    }
}
