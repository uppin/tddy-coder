//! Session action job execution: manifest validation, blocking invoke, async spawn, wait, stop.

use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use log::{debug, info};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use crate::error::WorkflowError;
use crate::read_changeset;
use crate::session_actions::{
    ensure_action_architecture, finalize_invocation_record, parse_action_manifest_file,
    resolve_action_manifest_path, resolve_allowlisted_path, run_manifest_command,
    validate_action_arguments_json, ActionManifest, SessionActionsError,
};

use super::error::SessionActionJobsError;
use super::{
    AsyncStartBody, BlockingOutcomeBody, SessionActionInvokeOptions, SessionActionInvokeOutcome,
    SessionActionStopOutcome, SessionActionWaitOutcome,
};

fn session_jobs_root(session_dir: &Path) -> PathBuf {
    session_dir.join("session_action_jobs")
}

fn job_workspace(session_dir: &Path, job_id: &str) -> PathBuf {
    session_jobs_root(session_dir).join("jobs").join(job_id)
}

fn job_record_path(job_dir: &Path) -> PathBuf {
    job_dir.join("job.json")
}

pub(crate) fn ensure_jobs_layout(session_dir: &Path) -> Result<(), SessionActionJobsError> {
    let root = session_jobs_root(session_dir);
    fs::create_dir_all(root.join("jobs"))?;
    info!(
        target: "tddy_core::session_action_jobs",
        "ensure_jobs_layout session_dir={} root={}",
        session_dir.display(),
        root.display()
    );
    Ok(())
}

fn load_repo_root(session_dir: &Path) -> Result<Option<PathBuf>, SessionActionJobsError> {
    match read_changeset(session_dir) {
        Ok(cs) => Ok(cs
            .repo_path
            .filter(|s| !s.trim().is_empty())
            .map(PathBuf::from)),
        Err(WorkflowError::ChangesetMissing(_)) => Ok(None),
        Err(e) => Err(SessionActionJobsError::ChangesetRead(e.to_string())),
    }
}

fn resolve_repo_for_invoke(
    session_dir: &Path,
    repo_root_hint: Option<&Path>,
) -> Result<Option<PathBuf>, SessionActionJobsError> {
    if let Some(p) = repo_root_hint.filter(|p| !p.as_os_str().is_empty()) {
        return Ok(Some(p.to_path_buf()));
    }
    load_repo_root(session_dir)
}

fn resolve_invoke(
    session_dir: &Path,
    repo_root_hint: Option<&Path>,
    action_id: &str,
    args: &Value,
) -> Result<(ActionManifest, Option<PathBuf>), SessionActionJobsError> {
    let manifest_path = resolve_action_manifest_path(session_dir, action_id)?;
    let manifest = parse_action_manifest_file(&manifest_path)?;
    validate_action_arguments_json(&manifest.input_schema, args)?;

    let repo = resolve_repo_for_invoke(session_dir, repo_root_hint)?;

    if let Some(bind) = manifest.output_path_arg.as_deref() {
        let v = args.get(bind).and_then(|x| x.as_str()).ok_or_else(|| {
            SessionActionsError::ArgumentsViolateSchema(format!(
                "missing string field `{bind}` for output path binding (required by manifest)"
            ))
        })?;
        resolve_allowlisted_path(session_dir, repo.as_deref(), v, "output_binding")?;
    }

    ensure_action_architecture(&manifest.architecture)?;
    debug!(
        target: "tddy_core::session_action_jobs",
        "resolve_invoke ok action_id={} manifest_id={}",
        action_id,
        manifest.id
    );
    Ok((manifest, repo))
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
enum JobPhase {
    Running,
    Completed,
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct PersistedJob {
    job_id: String,
    action_id: String,
    phase: JobPhase,
    pid: Option<u32>,
    exit_code: Option<i32>,
}

fn read_job(job_dir: &Path) -> Result<Option<PersistedJob>, SessionActionJobsError> {
    let p = job_record_path(job_dir);
    if !p.is_file() {
        return Ok(None);
    }
    let text = fs::read_to_string(&p)?;
    serde_json::from_str(&text)
        .map(Some)
        .map_err(|e| SessionActionJobsError::JobState(e.to_string()))
}

fn write_job_atomic(job_dir: &Path, job: &PersistedJob) -> Result<(), SessionActionJobsError> {
    fs::create_dir_all(job_dir)?;
    let tmp = job_dir.join("job.json.tmp");
    let final_path = job_record_path(job_dir);
    let payload = serde_json::to_string_pretty(job)
        .map_err(|e| SessionActionJobsError::JobState(e.to_string()))?;
    let mut f = File::create(&tmp)?;
    f.write_all(payload.as_bytes())?;
    f.sync_all()?;
    fs::rename(&tmp, &final_path)?;
    debug!(
        target: "tddy_core::session_action_jobs",
        "write_job_atomic job_id={} phase={:?}",
        job.job_id,
        job.phase
    );
    Ok(())
}

#[cfg(unix)]
fn reap_pid_nonblocking(pid: u32) -> Result<Option<i32>, SessionActionJobsError> {
    let mut status: i32 = 0;
    let r = unsafe { libc::waitpid(pid as libc::pid_t, &mut status, libc::WNOHANG) };
    if r == 0 {
        return Ok(None);
    }
    if r < 0 {
        let err = std::io::Error::last_os_error();
        if err.raw_os_error() == Some(libc::ECHILD) {
            return Ok(None);
        }
        return Err(SessionActionJobsError::Io(err));
    }
    if libc::WIFEXITED(status) {
        return Ok(Some(libc::WEXITSTATUS(status) as i32));
    }
    if libc::WIFSIGNALED(status) {
        return Ok(Some(128 + libc::WTERMSIG(status) as i32));
    }
    Ok(Some(-1))
}

#[cfg(unix)]
fn kill_process_group(pid: u32) -> Result<(), SessionActionJobsError> {
    let r = unsafe { libc::kill(-(pid as libc::pid_t), libc::SIGKILL) };
    if r < 0 {
        let err = std::io::Error::last_os_error();
        if err.raw_os_error() == Some(libc::ESRCH) {
            debug!(
                target: "tddy_core::session_action_jobs",
                "kill_process_group pid={} already ESRCH",
                pid
            );
            return Ok(());
        }
        return Err(SessionActionJobsError::Io(err));
    }
    Ok(())
}

#[cfg(not(unix))]
fn reap_pid_nonblocking(_pid: u32) -> Result<Option<i32>, SessionActionJobsError> {
    Err(SessionActionJobsError::JobState(
        "job wait/stop requires unix".into(),
    ))
}

#[cfg(not(unix))]
fn kill_process_group(_pid: u32) -> Result<(), SessionActionJobsError> {
    Err(SessionActionJobsError::JobState(
        "job stop requires unix".into(),
    ))
}

fn spawn_manifest_background(
    session_dir: &Path,
    repo_root: Option<&Path>,
    manifest: &ActionManifest,
    _args: &Value,
    stdout_path: &Path,
    stderr_path: &Path,
) -> Result<u32, SessionActionJobsError> {
    let cwd = repo_root
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| session_dir.to_path_buf());

    let program = manifest
        .command
        .first()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .ok_or(SessionActionsError::EmptyCommand)?;

    let stdout = File::create(stdout_path)?;
    let stderr = File::create(stderr_path)?;

    let mut cmd = Command::new(program);
    if manifest.command.len() > 1 {
        cmd.args(&manifest.command[1..]);
    }
    cmd.current_dir(&cwd);
    cmd.stdin(Stdio::null());
    cmd.stdout(Stdio::from(stdout));
    cmd.stderr(Stdio::from(stderr));

    #[cfg(unix)]
    unsafe {
        use std::os::unix::process::CommandExt;
        cmd.pre_exec(|| {
            if libc::setpgid(0, 0) != 0 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }

    info!(
        target: "tddy_core::session_action_jobs",
        "spawn_manifest_background id={} program={} cwd={}",
        manifest.id,
        program,
        cwd.display()
    );

    let child = cmd.spawn().map_err(|e| SessionActionsError::CommandSpawn {
        program: program.to_string(),
        detail: e.to_string(),
    })?;
    let pid = child.id();
    std::mem::forget(child);
    Ok(pid)
}

fn start_async_job(
    session_dir: &Path,
    manifest: &ActionManifest,
    args: &Value,
    repo: Option<&Path>,
    job_id: &str,
) -> Result<(PathBuf, PathBuf), SessionActionJobsError> {
    ensure_jobs_layout(session_dir)?;
    let job_dir = job_workspace(session_dir, job_id);
    fs::create_dir_all(&job_dir)?;
    let stdout_path = job_dir.join("stdout.log");
    let stderr_path = job_dir.join("stderr.log");
    File::create(&stdout_path)?;
    File::create(&stderr_path)?;

    let pid = spawn_manifest_background(
        session_dir,
        repo,
        manifest,
        args,
        &stdout_path,
        &stderr_path,
    )?;

    let job = PersistedJob {
        job_id: job_id.to_string(),
        action_id: manifest.id.clone(),
        phase: JobPhase::Running,
        pid: Some(pid),
        exit_code: None,
    };
    write_job_atomic(&job_dir, &job)?;
    Ok((stdout_path, stderr_path))
}

fn disposition(job: &PersistedJob) -> SessionActionWaitOutcome {
    match job.phase {
        JobPhase::Completed => SessionActionWaitOutcome::Completed {
            exit_code: job.exit_code,
        },
        JobPhase::Cancelled => SessionActionWaitOutcome::Failed {
            exit_code: job.exit_code,
            error_summary: Some("stopped".into()),
        },
        JobPhase::Running => unreachable!("disposition called on running without poll"),
    }
}

fn try_advance_running_job(
    job_dir: &Path,
    job: &mut PersistedJob,
) -> Result<bool, SessionActionJobsError> {
    if job.phase != JobPhase::Running {
        return Ok(true);
    }
    let Some(pid) = job.pid else {
        return Err(SessionActionJobsError::JobState(
            "running job missing pid".into(),
        ));
    };
    if let Some(code) = reap_pid_nonblocking(pid)? {
        job.phase = JobPhase::Completed;
        job.exit_code = Some(code);
        write_job_atomic(job_dir, job)?;
        debug!(
            target: "tddy_core::session_action_jobs",
            "reaped pid={} exit_code={:?}",
            pid,
            job.exit_code
        );
        return Ok(true);
    }
    Ok(false)
}

pub(crate) fn invoke_session_action(
    session_dir: &Path,
    repo_root: Option<&Path>,
    action_id: &str,
    args: &Value,
    options: SessionActionInvokeOptions,
) -> Result<SessionActionInvokeOutcome, SessionActionJobsError> {
    info!(
        target: "tddy_core::session_action_jobs",
        "invoke_session_action action_id={} async_start={}",
        action_id,
        options.async_start
    );
    let (manifest, repo) = resolve_invoke(session_dir, repo_root, action_id, args)?;
    let repo_ref = repo.as_deref();

    if options.async_start {
        let job_id = Uuid::now_v7().to_string();
        let (stdout_path, stderr_path) =
            start_async_job(session_dir, &manifest, args, repo_ref, &job_id)?;
        return Ok(SessionActionInvokeOutcome::AsyncStarted(AsyncStartBody {
            job_id,
            status: "running".into(),
            stdout_path,
            stderr_path,
        }));
    }

    let record = finish_blocking_record(session_dir, repo_ref, &manifest, args)?;
    Ok(SessionActionInvokeOutcome::Blocking(
        BlockingOutcomeBody::Record(record),
    ))
}

fn finish_blocking_record(
    session_dir: &Path,
    repo: Option<&Path>,
    manifest: &ActionManifest,
    args: &Value,
) -> Result<Value, SessionActionJobsError> {
    let mut record = run_manifest_command(session_dir, repo, manifest, args)?;
    finalize_invocation_record(manifest, &mut record)?;
    Ok(record)
}

pub(crate) fn wait_session_action_job(
    session_dir: &Path,
    job_id: &str,
    timeout_ms: Option<u64>,
) -> Result<SessionActionWaitOutcome, SessionActionJobsError> {
    let job_dir = job_workspace(session_dir, job_id);
    if !job_dir.is_dir() {
        return Err(SessionActionJobsError::UnknownJob(job_id.to_string()));
    }
    let deadline = timeout_ms
        .filter(|&ms| ms > 0)
        .map(|ms| Instant::now() + Duration::from_millis(ms));
    info!(
        target: "tddy_core::session_action_jobs",
        "wait_session_action_job job_id={} timeout_ms={:?}",
        job_id,
        timeout_ms
    );
    loop {
        let mut job = read_job(&job_dir)?.ok_or_else(|| {
            SessionActionJobsError::JobState(format!(
                "missing job.json under {}",
                job_dir.display()
            ))
        })?;
        if job.phase != JobPhase::Running {
            return Ok(disposition(&job));
        }
        if try_advance_running_job(&job_dir, &mut job)? {
            let job = read_job(&job_dir)?.ok_or_else(|| {
                SessionActionJobsError::JobState("job vanished after advance".into())
            })?;
            return Ok(disposition(&job));
        }
        if let Some(dl) = deadline {
            if Instant::now() >= dl {
                debug!(
                    target: "tddy_core::session_action_jobs",
                    "wait_session_action_job timed_out job_id={} still_running",
                    job_id
                );
                return Ok(SessionActionWaitOutcome::TimedOut {
                    still_running: true,
                });
            }
        }
        thread::sleep(Duration::from_millis(10));
    }
}

pub(crate) fn stop_session_action_job(
    session_dir: &Path,
    job_id: &str,
) -> Result<SessionActionStopOutcome, SessionActionJobsError> {
    let job_dir = job_workspace(session_dir, job_id);
    if !job_dir.is_dir() {
        return Err(SessionActionJobsError::UnknownJob(job_id.to_string()));
    }
    let mut job = read_job(&job_dir)?.ok_or_else(|| {
        SessionActionJobsError::JobState(format!("missing job.json under {}", job_dir.display()))
    })?;

    info!(
        target: "tddy_core::session_action_jobs",
        "stop_session_action_job job_id={} phase={:?}",
        job_id,
        job.phase
    );

    match job.phase {
        JobPhase::Completed | JobPhase::Cancelled => Ok(SessionActionStopOutcome::AlreadyFinished),
        JobPhase::Running => {
            let pid = job.pid.ok_or_else(|| {
                SessionActionJobsError::JobState("running job missing pid".into())
            })?;
            if try_advance_running_job(&job_dir, &mut job)? {
                return Ok(SessionActionStopOutcome::AlreadyFinished);
            }
            kill_process_group(pid)?;
            for _ in 0..500 {
                if reap_pid_nonblocking(pid)?.is_some() {
                    break;
                }
                thread::sleep(Duration::from_millis(5));
            }
            job.phase = JobPhase::Cancelled;
            job.exit_code = None;
            write_job_atomic(&job_dir, &job)?;
            Ok(SessionActionStopOutcome::Stopped)
        }
    }
}
