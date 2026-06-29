use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::ActionError;

/// How an action streams I/O to task channels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ChannelMode {
    /// No output channels (instant/sync tools).
    #[default]
    None,
    /// Separate stdout and stderr channels.
    StdoutStderr,
    /// Single combined stdout+stderr channel.
    Combined,
    /// PTY byte stream (stdin + broadcast output).
    Pty,
}

/// Declared input path for an action (host path + jail mount semantics).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActionInput {
    pub host_path: PathBuf,
    /// Path inside the jail; defaults to basename of host_path when empty.
    #[serde(default)]
    pub jail_path: Option<PathBuf>,
    /// When true the path is writable inside the jail (typically only output_dir).
    #[serde(default)]
    pub writable: bool,
}

/// Declared output artifact.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActionOutput {
    pub host_path: PathBuf,
    pub kind: OutputKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum OutputKind {
    #[default]
    File,
    Directory,
}

/// Optional sandbox confinement request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SandboxRequest {
    /// Writable egress directory inside the jail (persisted host-side).
    pub output_dir: PathBuf,
    /// Additional read-only paths beyond action inputs.
    #[serde(default)]
    pub extra_read_paths: Vec<PathBuf>,
    /// Recipe name (`claude-cli`, `bash`, `tddy-coder`, `build-action`, …).
    #[serde(default)]
    pub recipe: Option<String>,
    /// Bytes written to the confined process stdin after spawn (e.g. plan approval `a\n`).
    #[serde(default)]
    pub stdin: Option<String>,
}

/// Session-action-specific fields merged from `ActionManifest`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionActionExtras {
    pub summary: String,
    pub architecture: String,
    #[serde(default)]
    pub input_schema: Option<Value>,
    #[serde(default)]
    pub output_schema: Option<Value>,
    #[serde(default)]
    pub result_kind: Option<String>,
    #[serde(default)]
    pub output_path_arg: Option<String>,
    #[serde(default)]
    pub manifest_version: u32,
}

/// Pipeline stage for mapper / primary / transform subprocesses.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PipelineStage {
    pub program: String,
    pub args: Vec<String>,
    #[serde(default)]
    pub env: BTreeMap<String, String>,
}

/// Canonical description of a runnable tool.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActionSpec {
    pub id: String,
    pub kind: String,
    pub command: Vec<String>,
    #[serde(default)]
    pub inputs: Vec<ActionInput>,
    #[serde(default)]
    pub outputs: Vec<ActionOutput>,
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    #[serde(default)]
    pub working_dir: Option<PathBuf>,
    #[serde(default)]
    pub channel_mode: ChannelMode,
    #[serde(default)]
    pub sandbox: Option<SandboxRequest>,
    #[serde(default)]
    pub session: Option<SessionActionExtras>,
    #[serde(default)]
    pub pipeline: Option<PipelineSpec>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PipelineSpec {
    #[serde(default)]
    pub input_mapper: Option<PipelineStage>,
    pub primary: PipelineStage,
    #[serde(default)]
    pub output_transform: Option<PipelineStage>,
    #[serde(default)]
    pub capture_channel_ids: Vec<String>,
}

impl ActionSpec {
    pub fn validate(&self) -> Result<(), ActionError> {
        if self.id.is_empty() {
            return Err(ActionError::InvalidSpec("id must not be empty".into()));
        }
        if self.kind.is_empty() {
            return Err(ActionError::InvalidSpec("kind must not be empty".into()));
        }
        if self.pipeline.is_none() && self.command.is_empty() {
            return Err(ActionError::InvalidSpec("command must not be empty".into()));
        }
        for input in &self.inputs {
            if !input.host_path.is_absolute() {
                return Err(ActionError::InvalidSpec(format!(
                    "input host_path must be absolute: {}",
                    input.host_path.display()
                )));
            }
        }
        Ok(())
    }
}
