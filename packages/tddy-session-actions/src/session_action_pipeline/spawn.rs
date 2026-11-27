use crate::session_action_pipeline::open_truncate_for_child;
use crate::session_action_pipeline::SessionActionPipelineError;
use log::{debug, info};
use serde_json::Value;
use std::collections::HashMap;
use std::path::Path;
use std::path::PathBuf;
use std::process::{Command, Stdio};

/// Run the configured input mapper subprocess: schema-valid JSON on stdin; stdout must be one JSON
/// document matching the `args`/`env` envelope (validated before the primary action is spawned).
pub fn run_input_mapper_for_envelope(
    mapper_cmd: &[String],
    input: &Value,
    channels: &HashMap<String, PathBuf>,
) -> Result<(Vec<String>, HashMap<String, String>), SessionActionPipelineError> {
    let (program, args) = mapper_cmd.split_first().ok_or_else(|| {
        SessionActionPipelineError::EnvelopeValidation(
            "mapper command argv must be non-empty".into(),
        )
    })?;

    info!(
        target: "tddy_core::session_action_pipeline",
        "run_input_mapper_for_envelope program={} channel_ids={:?}",
        program,
        channels.keys().collect::<Vec<_>>()
    );

    let manifest_json = serde_json::to_string(channels).map_err(|e| {
        SessionActionPipelineError::EnvelopeValidation(format!(
            "could not serialize channel manifest for subprocess: {e}"
        ))
    })?;

    let mut cmd = Command::new(program);
    cmd.args(args);
    cmd.stdin(Stdio::piped());
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());
    cmd.env_clear();
    cmd.env("TDDY_SESSION_CHANNEL_MANIFEST_JSON", manifest_json.as_str());

    let mut child = cmd.spawn().map_err(SessionActionPipelineError::Io)?;

    if let Some(mut stdin) = child.stdin.take() {
        serde_json::to_writer(&mut stdin, input)
            .map_err(|e| SessionActionPipelineError::Io(std::io::Error::other(e)))?;
    }

    let output = child
        .wait_with_output()
        .map_err(SessionActionPipelineError::Io)?;
    let code = output.status.code().unwrap_or(-1);
    let stderr_lossy = String::from_utf8_lossy(&output.stderr).into_owned();

    if !output.status.success() {
        return Err(SessionActionPipelineError::InputMapperFailed {
            exit_code: code,
            stderr: stderr_lossy,
        });
    }

    let stdout_text = String::from_utf8_lossy(&output.stdout);
    let trimmed = stdout_text.trim();
    debug!(
        target: "tddy_core::session_action_pipeline",
        "run_input_mapper_for_envelope mapper_stdout_len={}",
        trimmed.len()
    );

    let value: Value = serde_json::from_str(trimmed).map_err(|e| {
        SessionActionPipelineError::MapperInvalidJson(format!("{e}; stdout prefix={:?}", {
            let mut s = trimmed.chars().take(120).collect::<String>();
            if trimmed.len() > 120 {
                s.push_str("...");
            }
            s
        }))
    })?;

    parse_invocation_envelope(&value)
}

fn parse_invocation_envelope(
    value: &Value,
) -> Result<(Vec<String>, HashMap<String, String>), SessionActionPipelineError> {
    let obj = value.as_object().ok_or_else(|| {
        SessionActionPipelineError::EnvelopeValidation(
            "invocation envelope must be a JSON object".into(),
        )
    })?;

    let unexpected: Vec<String> = obj
        .keys()
        .filter(|k| *k != "args" && *k != "env")
        .cloned()
        .collect();
    if !unexpected.is_empty() {
        return Err(SessionActionPipelineError::EnvelopeValidation(format!(
            "invocation envelope has unexpected keys (expected only args, env): {unexpected:?}"
        )));
    }

    let args_val = obj
        .get("args")
        .ok_or_else(|| SessionActionPipelineError::EnvelopeValidation("missing `args`".into()))?;
    let args_arr = args_val.as_array().ok_or_else(|| {
        SessionActionPipelineError::EnvelopeValidation("`args` must be a JSON array".into())
    })?;

    let mut argv: Vec<String> = Vec::with_capacity(args_arr.len());
    for (i, item) in args_arr.iter().enumerate() {
        let s = item.as_str().ok_or_else(|| {
            SessionActionPipelineError::EnvelopeValidation(format!(
                "`args[{i}]` must be a JSON string"
            ))
        })?;
        argv.push(s.to_string());
    }

    let env_val = obj
        .get("env")
        .ok_or_else(|| SessionActionPipelineError::EnvelopeValidation("missing `env`".into()))?;
    let env_obj = env_val.as_object().ok_or_else(|| {
        SessionActionPipelineError::EnvelopeValidation("`env` must be a JSON object".into())
    })?;

    let mut env_map: HashMap<String, String> = HashMap::with_capacity(env_obj.len());
    for (k, v) in env_obj {
        let s = v.as_str().ok_or_else(|| {
            SessionActionPipelineError::EnvelopeValidation(format!(
                "`env[{k:?}]` must be a JSON string"
            ))
        })?;
        env_map.insert(k.clone(), s.to_string());
    }

    Ok((argv, env_map))
}

/// After the primary action completes, run the output transform with access to capture channel paths;
/// stdout must parse as JSON and validate against `output_schema`.
pub fn run_output_transform_and_validate(
    transform_cmd: &[String],
    channels: &HashMap<String, PathBuf>,
    output_schema: &Value,
) -> Result<Value, SessionActionPipelineError> {
    let (program, args) = transform_cmd.split_first().ok_or_else(|| {
        SessionActionPipelineError::EnvelopeValidation(
            "transform command argv must be non-empty".into(),
        )
    })?;

    info!(
        target: "tddy_core::session_action_pipeline",
        "run_output_transform_and_validate program={} channel_ids={:?}",
        program,
        channels.keys().collect::<Vec<_>>()
    );

    let manifest_json = serde_json::to_string(channels).map_err(|e| {
        SessionActionPipelineError::EnvelopeValidation(format!(
            "could not serialize channel manifest for subprocess: {e}"
        ))
    })?;

    let mut cmd = Command::new(program);
    cmd.args(args);
    cmd.stdin(Stdio::null());
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());
    cmd.env_clear();
    cmd.env("TDDY_SESSION_CHANNEL_MANIFEST_JSON", manifest_json.as_str());

    let output = cmd.output().map_err(SessionActionPipelineError::Io)?;
    let code = output.status.code().unwrap_or(-1);
    let stderr_lossy = String::from_utf8_lossy(&output.stderr).into_owned();

    if !output.status.success() {
        return Err(SessionActionPipelineError::OutputTransformFailed {
            exit_code: code,
            stderr: stderr_lossy,
        });
    }

    let stdout_text = String::from_utf8_lossy(&output.stdout);
    let trimmed = stdout_text.trim();

    let parsed: Value = serde_json::from_str(trimmed).map_err(|e| {
        SessionActionPipelineError::MapperInvalidJson(format!("transform stdout: {e}"))
    })?;

    let validator = jsonschema::Validator::new(output_schema).map_err(|e| {
        SessionActionPipelineError::TransformOutputSchema(format!(
            "invalid output JSON Schema: {e}"
        ))
    })?;

    validator
        .validate(&parsed)
        .map_err(|e| SessionActionPipelineError::TransformOutputSchema(e.to_string()))?;

    debug!(
        target: "tddy_core::session_action_pipeline",
        "run_output_transform_and_validate ok"
    );
    Ok(parsed)
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
