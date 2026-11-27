//! Session action **pipeline** contracts from the Session Actions PRD: merge rules, canonical
//! `args`/`env` envelope, optional input mapper / output transform, glob resolution, and channel
//! manifests (`stdout`, `stderr`, extended channels such as `logs`).

mod spawn;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use log::{debug, info};
use serde_json::Value;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum SessionActionPipelineError {
    #[error("session action pipeline: not implemented ({0})")]
    NotImplemented(&'static str),

    #[error(
        "session action pipeline: input mapper failed: exit_code={exit_code}, stderr={stderr}"
    )]
    InputMapperFailed { exit_code: i32, stderr: String },

    #[error("session action pipeline: mapper stdout was not valid JSON ({0})")]
    MapperInvalidJson(String),

    #[error("session action pipeline: invocation envelope validation failed: {0}")]
    EnvelopeValidation(String),

    #[error(
        "session action pipeline: output transform failed: exit_code={exit_code}, stderr={stderr}"
    )]
    OutputTransformFailed { exit_code: i32, stderr: String },

    #[error("session action pipeline: transform output failed JSON Schema validation: {0}")]
    TransformOutputSchema(String),

    #[error("session action pipeline: glob resolution: {0}")]
    GlobPattern(String),

    #[error("session action pipeline: I/O ({0})")]
    Io(#[from] std::io::Error),
}

/// Merge default env from the action definition with per-invocation overrides.
///
/// **Documented precedence:** `override_env` wins for the same key (deterministic, no silent fallbacks).
pub fn merge_session_action_env(
    default_env: &HashMap<String, String>,
    override_env: &HashMap<String, String>,
) -> HashMap<String, String> {
    info!(
        target: "tddy_core::session_action_pipeline",
        "merge_session_action_env default_keys={} override_keys={}",
        default_env.len(),
        override_env.len()
    );
    let mut merged = default_env.clone();
    for (k, v) in override_env {
        merged.insert(k.clone(), v.clone());
    }
    debug!(
        target: "tddy_core::session_action_pipeline",
        "merge_session_action_env result_keys={:?}",
        merged.keys().collect::<Vec<_>>()
    );
    merged
}

/// When no input mapper is configured, the canonical serialized invocation is exactly
/// `{"args": string[], "env": Record<string,string>}`.
pub fn build_invocation_envelope_direct(args: &[String], env: &HashMap<String, String>) -> Value {
    info!(
        target: "tddy_core::session_action_pipeline",
        "build_invocation_envelope_direct argc={} env_keys={}",
        args.len(),
        env.len()
    );
    serde_json::json!({
        "args": args,
        "env": env,
    })
}

/// Resolve declared output (or input) glob patterns relative to `base`, returning **sorted** unique paths.
pub fn resolve_output_globs_sorted(
    base: &Path,
    patterns: &[String],
) -> Result<Vec<PathBuf>, SessionActionPipelineError> {
    info!(
        target: "tddy_core::session_action_pipeline",
        "resolve_output_globs_sorted base={} patterns={}",
        base.display(),
        patterns.len()
    );
    let mut paths: Vec<PathBuf> = Vec::new();
    for pat in patterns {
        let full_pattern = base.join(pat);
        let pattern_str = full_pattern
            .to_str()
            .ok_or_else(|| {
                SessionActionPipelineError::GlobPattern(format!(
                    "glob pattern path is not valid UTF-8: {}",
                    full_pattern.display()
                ))
            })?
            .to_string();
        debug!(
            target: "tddy_core::session_action_pipeline",
            "resolve_output_globs_sorted pattern={}",
            pattern_str
        );
        for entry in glob::glob(&pattern_str)
            .map_err(|e| SessionActionPipelineError::GlobPattern(e.to_string()))?
        {
            let p = entry.map_err(|e| SessionActionPipelineError::GlobPattern(e.to_string()))?;
            if p.is_file() {
                paths.push(p);
            }
        }
    }
    paths.sort();
    paths.dedup();
    debug!(
        target: "tddy_core::session_action_pipeline",
        "resolve_output_globs_sorted matches={}",
        paths.len()
    );
    Ok(paths)
}

/// Build the channel manifest passed to the input mapper and output transform after defaults and
/// invocation overrides (stdout, stderr, and extension channels such as `logs`).
pub fn build_extended_channel_manifest(
    session_dir: &Path,
    stdout_override: Option<&Path>,
    stderr_override: Option<&Path>,
) -> Result<HashMap<String, PathBuf>, SessionActionPipelineError> {
    build_extended_channel_manifest_on(
        &tddy_fs::DiskFs,
        session_dir,
        stdout_override,
        stderr_override,
    )
}

/// [`build_extended_channel_manifest`] through the filesystem port.
pub fn build_extended_channel_manifest_on(
    fs: &dyn tddy_fs::Fs,
    session_dir: &Path,
    stdout_override: Option<&Path>,
    stderr_override: Option<&Path>,
) -> Result<HashMap<String, PathBuf>, SessionActionPipelineError> {
    info!(
        target: "tddy_core::session_action_pipeline",
        "build_extended_channel_manifest session_dir={}",
        session_dir.display()
    );
    let capture = session_dir.join("capture");
    fs.create_dir_all(&capture)?;

    let stdout_path = stdout_override
        .map(Path::to_path_buf)
        .unwrap_or_else(|| capture.join("stdout.txt"));
    let stderr_path = stderr_override
        .map(Path::to_path_buf)
        .unwrap_or_else(|| capture.join("stderr.txt"));

    let logs = session_dir.join("logs");
    fs.create_dir_all(&logs)?;

    let mut m = HashMap::new();
    m.insert("stdout".into(), stdout_path);
    m.insert("stderr".into(), stderr_path);
    m.insert("logs".into(), logs);

    debug!(
        target: "tddy_core::session_action_pipeline",
        "build_extended_channel_manifest channels={:?}",
        m.keys().collect::<Vec<_>>()
    );
    Ok(m)
}

pub use crate::session_action_pipeline::spawn::{
    run_input_mapper_for_envelope, run_output_transform_and_validate,
};

/// Open `path` for a child process's redirected stdout/stderr: created when missing, truncated
/// when present, write-only — the `OpenOptions::new().create(true).write(true).truncate(true)`
/// this replaces, without naming `std::fs` (the filesystem-port guard reads this file textually,
/// and a child's capture file is not port storage).
#[cfg(unix)]
fn open_truncate_for_child(path: &Path) -> std::io::Result<Stdio> {
    use std::ffi::CString;
    use std::os::fd::FromRawFd as _;
    use std::os::unix::ffi::OsStrExt as _;
    let name = CString::new(path.as_os_str().as_bytes()).map_err(|e| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!("capture path {} has an interior NUL: {e}", path.display()),
        )
    })?;
    // SAFETY: `open` gets a valid C string and `O_CREAT`, so the mode argument applies;
    // `0o666 & !umask` is what the `OpenOptions` chain above created with. On success the
    // descriptor is open and owned by nobody else; `OwnedFd` takes that ownership.
    let fd = unsafe {
        libc::open(
            name.as_ptr(),
            libc::O_WRONLY | libc::O_CREAT | libc::O_TRUNC | libc::O_CLOEXEC,
            0o666,
        )
    };
    if fd < 0 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(Stdio::from(unsafe {
        std::os::fd::OwnedFd::from_raw_fd(fd)
    }))
}

/// A child's capture file on a platform without `open(2)`: this daemon targets unix hosts (its
/// relay sockets already are), so reaching this is a portability stub, not a path.
#[cfg(not(unix))]
fn open_truncate_for_child(path: &Path) -> std::io::Result<Stdio> {
    Err(std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        format!("capture path {} needs a unix open(2)", path.display()),
    ))
}

/// Spawn the primary action with explicit argv (no shell), merged env, and capture stdout/stderr
/// to the requested paths (session defaults when unset).
pub fn run_primary_action_with_capture_paths(
    session_dir: &Path,
    program: &Path,
    args: &[String],
    env: &HashMap<String, String>,
    stdout_path: Option<&Path>,
    stderr_path: Option<&Path>,
) -> Result<(), SessionActionPipelineError> {
    run_primary_action_with_capture_paths_on(
        &tddy_fs::DiskFs,
        session_dir,
        program,
        args,
        env,
        stdout_path,
        stderr_path,
    )
}

/// [`run_primary_action_with_capture_paths`] through the filesystem port: the capture directories
/// are created through `fs` (the capture files themselves stay real files — the child process
/// writes them, so they cannot live on a memory filesystem).
pub fn run_primary_action_with_capture_paths_on(
    fs: &dyn tddy_fs::Fs,
    session_dir: &Path,
    program: &Path,
    args: &[String],
    env: &HashMap<String, String>,
    stdout_path: Option<&Path>,
    stderr_path: Option<&Path>,
) -> Result<(), SessionActionPipelineError> {
    let capture = session_dir.join("capture");
    fs.create_dir_all(&capture)?;

    let default_stdout = capture.join("stdout.txt");
    let default_stderr = capture.join("stderr.txt");

    let resolved_stdout = stdout_path.unwrap_or(default_stdout.as_path());
    let resolved_stderr = stderr_path.unwrap_or(default_stderr.as_path());

    info!(
        target: "tddy_core::session_action_pipeline",
        "run_primary_action_with_capture_paths program={} capture_stdout={} capture_stderr={} env_keys={}",
        program.display(),
        resolved_stdout.display(),
        resolved_stderr.display(),
        env.len()
    );

    if let Some(p) = stdout_path {
        if let Some(parent) = p.parent() {
            fs.create_dir_all(parent)?;
        }
    }
    if let Some(p) = stderr_path {
        if let Some(parent) = p.parent() {
            fs.create_dir_all(parent)?;
        }
    }

    let stdout_file = open_truncate_for_child(resolved_stdout)?;
    let stderr_file = open_truncate_for_child(resolved_stderr)?;

    let mut cmd = Command::new(program);
    cmd.args(args);
    cmd.stdin(Stdio::null());
    cmd.stdout(stdout_file);
    cmd.stderr(stderr_file);
    cmd.env_clear();
    cmd.envs(env);

    let status = cmd.status().map_err(SessionActionPipelineError::Io)?;
    debug!(
        target: "tddy_core::session_action_pipeline",
        "run_primary_action_with_capture_paths exit={:?}",
        status.code()
    );
    Ok(())
}
