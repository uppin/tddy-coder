//! Conversions from external action shapes into [`ActionSpec`].

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde_json::Value;

use crate::spec::{
    ActionInput, ActionOutput, ActionSpec, ChannelMode, OutputKind, SessionActionExtras,
};

/// Fields extracted from a session `ActionManifest` (avoids tddy-core dep in this crate).
#[derive(Debug, Clone)]
pub struct SessionManifestFields {
    pub version: u32,
    pub id: String,
    pub summary: String,
    pub architecture: String,
    pub command: Vec<String>,
    pub input_schema: Option<Value>,
    pub output_schema: Option<Value>,
    pub result_kind: Option<String>,
    pub output_path_arg: Option<String>,
    pub working_dir: Option<PathBuf>,
}

/// Build a session-action [`ActionSpec`] from manifest fields.
pub fn action_spec_from_session_manifest(fields: SessionManifestFields) -> ActionSpec {
    ActionSpec {
        id: fields.id.clone(),
        kind: "session-action".to_string(),
        command: fields.command,
        inputs: Vec::new(),
        outputs: Vec::new(),
        env: BTreeMap::new(),
        working_dir: fields.working_dir,
        channel_mode: ChannelMode::StdoutStderr,
        sandbox: None,
        session: Some(SessionActionExtras {
            summary: fields.summary,
            architecture: fields.architecture,
            input_schema: fields.input_schema,
            output_schema: fields.output_schema,
            result_kind: fields.result_kind,
            output_path_arg: fields.output_path_arg,
            manifest_version: fields.version,
        }),
        pipeline: None,
    }
}

/// Fields extracted from a tddy-build `BuildAction` (avoids circular tddy-build dep).
#[derive(Debug, Clone)]
pub struct BuildActionFields {
    pub id: String,
    pub command: Vec<String>,
    pub env: BTreeMap<String, String>,
    pub input_globs: Vec<(String, Vec<String>)>, // (root, include patterns)
    pub outputs: Vec<(String, OutputKind)>,
    pub working_dir: Option<PathBuf>,
}

/// Build a build-action [`ActionSpec`] from lowered build fields.
pub fn build_action_fields_to_spec(
    repo_root: &std::path::Path,
    fields: BuildActionFields,
) -> ActionSpec {
    let mut inputs = Vec::new();
    for (root, patterns) in fields.input_globs {
        let base = if root.is_empty() {
            repo_root.to_path_buf()
        } else {
            repo_root.join(root)
        };
        for pattern in patterns {
            let host = base.join(&pattern);
            inputs.push(ActionInput {
                host_path: host,
                jail_path: None,
                writable: false,
            });
        }
    }
    let outputs = fields
        .outputs
        .into_iter()
        .map(|(path, kind)| ActionOutput {
            host_path: repo_root.join(path),
            kind,
        })
        .collect();
    let working_dir = fields
        .working_dir
        .filter(|w| !w.as_os_str().is_empty())
        .map(|w| repo_root.join(w));

    ActionSpec {
        id: fields.id,
        kind: "build-action".to_string(),
        command: fields.command,
        inputs,
        outputs,
        env: fields.env,
        working_dir,
        channel_mode: ChannelMode::StdoutStderr,
        sandbox: None,
        pipeline: None,
        session: None,
    }
}
