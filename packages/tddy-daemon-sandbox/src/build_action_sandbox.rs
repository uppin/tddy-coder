//! `tddy-build`'s hermetic-action port, over the action sandbox runtime (`#agent-tools 4/12`).
//!
//! Product doc: `docs/ft/build/tddy-build.md` § Hermetic actions.
//!
//! [`DaemonActionSandbox`] lowers one build action into a process-mode jail plan:
//!
//! - every input the action declares is a read-only grant;
//! - its declared outputs are the only writable paths;
//! - its tool closure is lowered by `tddy_sandbox_recipes::lower_tools` — each binary a literal
//!   executable read, reachable by name from a jail-local `bin/` first on `PATH`;
//! - its network is `tddy_sandbox_recipes::lower_network` of the same closure, so a closure of only
//!   `egress: none` tools has no network at all. A closure that declares hosts is refused: a build
//!   action has no egress shim to hold it to them.
//!
//! The jail is the platform backend's (Seatbelt on macOS, cgroups on Linux) — the same one
//! `sandbox_action::spawn_sandbox_action` runs a sandboxed `ActionSpec` in. The action's stdout and
//! stderr are captured by the jailed command itself, into files in its own jail directory, rather
//! than through the backend's logs: the Seatbelt wrapper appends a line of its own to the stderr log,
//! and the cgroups backend does not capture the child's output at all.

use std::fs::OpenOptions;
use std::path::{Path, PathBuf};
use std::time::Duration;

use futures_util::future::BoxFuture;
use tddy_actions::tool::{Egress, ResolvedTool};
use tddy_actions::{ActionSpec, OutputKind};
use tddy_build::executor::ActionOutcome;
use tddy_build::{ActionSandbox, BuildError};
use tddy_sandbox::{
    process_jail_env, EgressPolicy, MountSpec, ReadReason, ReadSpec, SandboxError, SandboxPlan,
};
use tddy_sandbox_recipes::{
    build_process_plan, lower_network, lower_tools, HostBinaryLibraries, ProcessPlanRequest,
    SandboxRecipe,
};

use crate::sandbox_session::spawn_sandbox_plan;

/// How often a jailed command is polled for its exit.
const EXIT_POLL: Duration = Duration::from_millis(50);

/// The shell the jailed command runs under. It points stdout and stderr at the two files it is given,
/// then replaces itself with the action's command, so the captures are the command's own.
const LAUNCHER: &str = r#"exec >"$1" 2>"$2"; shift 2; exec "$@""#;

/// Runs `tddy-build` hermetic actions in a real jail.
#[derive(Debug, Clone)]
pub struct DaemonActionSandbox {
    /// Where each action's jail (its writable tree, scratch and captured output) is created, one
    /// directory per action id. Must be absolute.
    work_dir: PathBuf,
}

impl DaemonActionSandbox {
    /// A sandbox laying out each action's jail under `work_dir`.
    pub fn new(work_dir: PathBuf) -> Self {
        Self { work_dir }
    }
}

impl ActionSandbox for DaemonActionSandbox {
    fn run<'a>(
        &'a self,
        spec: &'a ActionSpec,
        tools: &'a [ResolvedTool],
    ) -> BoxFuture<'a, Result<ActionOutcome, BuildError>> {
        Box::pin(run_in_jail(&self.work_dir, spec, tools))
    }
}

/// One action's jail tree, under `<work_dir>/<action id>`.
struct ActionJail {
    /// The jail's writable tree: the backend's `project_root`.
    root: PathBuf,
    /// `HOME`, `TMPDIR`, and the `bin/` of tool links.
    scratch: PathBuf,
    /// The backend's egress directory. The captured stdout and stderr are written here too.
    out: PathBuf,
    profile: PathBuf,
}

impl ActionJail {
    fn under(work_dir: &Path, action_id: &str) -> Self {
        let root = work_dir.join(jail_dir_name(action_id));
        Self {
            scratch: root.join(".work"),
            out: root.join("out"),
            profile: root.join("sandbox.sb"),
            root,
        }
    }

    fn bin_dir(&self) -> PathBuf {
        self.scratch.join("bin")
    }

    fn stdout(&self) -> PathBuf {
        self.out.join("stdout")
    }

    fn stderr(&self) -> PathBuf {
        self.out.join("stderr")
    }
}

/// The directory name for an action id. Anything that could name a path outside the jail's own
/// directory is replaced, so no id can escape `work_dir`.
fn jail_dir_name(action_id: &str) -> String {
    action_id
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '-' | '_') {
                c
            } else {
                '_'
            }
        })
        .collect()
}

async fn run_in_jail(
    work_dir: &Path,
    spec: &ActionSpec,
    tools: &[ResolvedTool],
) -> Result<ActionOutcome, BuildError> {
    spec.validate().map_err(|e| jail_error(&spec.id, e))?;
    let jail = ActionJail::under(work_dir, &spec.id);
    prepare_jail(&jail, spec)?;
    let plan = plan_action(&jail, spec, tools)?;

    let exit_code = tokio::task::spawn_blocking(move || run_to_exit(plan))
        .await
        .map_err(|e| BuildError::Exec(format!("hermetic action {}: {e}", spec.id)))?
        .map_err(|e| jail_error(&spec.id, e))?;

    Ok(ActionOutcome {
        action_id: spec.id.clone(),
        cached: false,
        exit_code,
        argv: spec.command.clone(),
        stdout: read_capture(&jail.stdout()),
        stderr: read_capture(&jail.stderr()),
    })
}

/// Create the jail's directories, clear what an earlier run captured, and create each declared output.
/// A mount needs its path to exist, so an output file is created empty and the action writes into it.
fn prepare_jail(jail: &ActionJail, spec: &ActionSpec) -> Result<(), BuildError> {
    let dirs = [
        jail.root.clone(),
        jail.scratch.join("home"),
        jail.scratch.join("tmp"),
        jail.out.clone(),
    ];
    for dir in &dirs {
        create_dir(dir)?;
    }
    for capture in [jail.stdout(), jail.stderr()] {
        match std::fs::remove_file(&capture) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(io_error(&capture, e)),
        }
    }
    for output in &spec.outputs {
        match output.kind {
            OutputKind::Directory => create_dir(&output.host_path)?,
            OutputKind::File => {
                if let Some(parent) = output.host_path.parent() {
                    create_dir(parent)?;
                }
                OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(&output.host_path)
                    .map_err(|e| io_error(&output.host_path, e))?;
            }
        }
    }
    Ok(())
}

/// The plan for `spec`: its command under the capture launcher, its inputs read-only, its outputs
/// writable, its working directory, and the tool closure's grants and network.
fn plan_action(
    jail: &ActionJail,
    spec: &ActionSpec,
    tools: &[ResolvedTool],
) -> Result<SandboxPlan, BuildError> {
    let (egress, oauth_inbound) = lower_network(tools);
    refuse_hosts(tools, &egress)?;
    let grants = lower_tools(tools, &jail.bin_dir(), &HostBinaryLibraries);

    let cwd = spec
        .working_dir
        .clone()
        .unwrap_or_else(|| jail.root.clone());
    // The working directory is not mounted, but a relative path inside it resolves through each of
    // its ancestors. Each is granted for lookup only: a jail that cannot look them up cannot run the
    // command that names them, and a listing grant would show the neighbours of its own checkout.
    let mut extra_reads: Vec<ReadSpec> = spec
        .inputs
        .iter()
        .map(|input| ReadSpec::subpath(input.host_path.clone(), ReadReason::Custom))
        .collect();
    extra_reads.extend(
        cwd.ancestors()
            .map(|dir| ReadSpec::metadata(dir.to_path_buf(), ReadReason::Custom)),
    );
    extra_reads.extend(grants.reads);

    let mut jail_env = process_jail_env(&jail.scratch.join("home"), &jail.scratch.join("tmp"));
    let baseline_path = jail_env.get("PATH").cloned().unwrap_or_default();
    jail_env.insert(
        "PATH".to_string(),
        format!("{}:{baseline_path}", jail.bin_dir().display()),
    );
    // A shell with no `PWD` asks the kernel for its directory, which walks every ancestor and so
    // needs listing rights the jail does not grant. With `PWD` naming the directory it starts in, a
    // shell checks that name against the directory and uses it.
    jail_env.insert("PWD".to_string(), cwd.display().to_string());
    let mut env = spec.env.clone();
    env.extend(jail_env);

    let mut plan = build_process_plan(ProcessPlanRequest {
        project_root: jail.root.clone(),
        scratch_dir: jail.scratch.clone(),
        egress_dir: jail.out.clone(),
        profile_path: jail.profile.clone(),
        command: launcher_argv(&spec.command, &jail.stdout(), &jail.stderr()),
        env,
        // FIXME(build-tools): the backend also grants `process-exec` on every writable mount, so an
        // output the action wrote is executable inside the same jail. A write grant without an exec
        // grant would close that; the plan has no such grant yet.
        mounts: spec
            .outputs
            .iter()
            .map(|output| MountSpec::read_write(output.host_path.clone()))
            .collect(),
        // The shell recipe is the baseline every confined command runs in: the OS's own binaries
        // are executable, and the closure's binaries are the only addition.
        recipe: SandboxRecipe::Shell,
        host_home: None,
        cwd: Some(cwd),
        extra_reads,
        stdin: None,
    })
    .map_err(|e| jail_error(&spec.id, e))?;
    plan.symlinks.extend(grants.symlinks);
    // A plan's network defaults to any egress, so the closure's own policy is set explicitly.
    plan.network.egress = egress;
    plan.network.allow_oauth_inbound = oauth_inbound;
    Ok(plan)
}

/// A tool that declares hosts needs an egress shim to hold it to them, and a build action runs with
/// none. Refused, rather than run with those hosts silently unreachable.
// TODO(build-tools): start an egress shim for build actions, so a declared host is reachable.
fn refuse_hosts(tools: &[ResolvedTool], egress: &EgressPolicy) -> Result<(), BuildError> {
    if matches!(egress, EgressPolicy::None) {
        return Ok(());
    }
    let declaring: Vec<&str> = tools
        .iter()
        .filter(|tool| !matches!(tool.network.egress, Egress::None))
        .map(|tool| tool.id.as_str())
        .collect();
    Err(BuildError::Exec(format!(
        "tools {} declare network hosts, which a hermetic build action cannot reach: it has no egress shim yet",
        declaring.join(", ")
    )))
}

/// The argv the jail runs: the launcher shell, its two capture files, then the action's command.
fn launcher_argv(command: &[String], stdout: &Path, stderr: &Path) -> Vec<String> {
    let mut argv = vec![
        "/bin/sh".to_string(),
        "-c".to_string(),
        LAUNCHER.to_string(),
        "tddy-build-action".to_string(),
        stdout.display().to_string(),
        stderr.display().to_string(),
    ];
    argv.extend(command.iter().cloned());
    argv
}

/// Spawn the jail and wait for its command to exit. A command ended by a signal answers -1.
fn run_to_exit(plan: SandboxPlan) -> Result<i32, SandboxError> {
    let mut handle = spawn_sandbox_plan(plan)?;
    loop {
        match handle.child_mut().try_wait() {
            Ok(Some(status)) => return Ok(status.code().unwrap_or(-1)),
            Ok(None) => std::thread::sleep(EXIT_POLL),
            Err(e) => return Err(SandboxError::Io(format!("wait for the jailed action: {e}"))),
        }
    }
}

/// What the launcher captured into `path`, decoded lossily. Empty when the command wrote nothing, or
/// when the launcher never ran; the exit code tells those apart.
fn read_capture(path: &Path) -> String {
    String::from_utf8_lossy(&std::fs::read(path).unwrap_or_default()).into_owned()
}

fn jail_error(action_id: &str, error: impl std::fmt::Display) -> BuildError {
    BuildError::Exec(format!("hermetic action {action_id}: {error}"))
}

fn create_dir(path: &Path) -> Result<(), BuildError> {
    std::fs::create_dir_all(path).map_err(|e| io_error(path, e))
}

fn io_error(path: &Path, error: std::io::Error) -> BuildError {
    BuildError::Io(format!("{}: {error}", path.display()))
}
