//! Convert [`BuildAction`] proto messages into [`tddy_actions::ActionSpec`].

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use tddy_actions::{build_action_fields_to_spec, BuildActionFields, OutputKind};

use crate::proto::{BuildAction, OutputKind as ProtoOutputKind};

fn proto_output_kind(kind: i32) -> OutputKind {
    match ProtoOutputKind::try_from(kind).unwrap_or(ProtoOutputKind::Unspecified) {
        ProtoOutputKind::Directory => OutputKind::Directory,
        _ => OutputKind::File,
    }
}

/// Build an [`tddy_actions::ActionSpec`] from a lowered build action.
pub fn build_action_to_spec(repo_root: &Path, action: &BuildAction) -> tddy_actions::ActionSpec {
    let input_globs = action
        .inputs
        .iter()
        .map(|fs| (fs.root.clone(), fs.include.clone()))
        .collect();
    let outputs = action
        .outputs
        .iter()
        .map(|o| (o.path.clone(), proto_output_kind(o.kind)))
        .collect();
    let working_dir = if action.working_dir.is_empty() {
        None
    } else {
        Some(PathBuf::from(&action.working_dir))
    };
    let env: BTreeMap<String, String> = action
        .env
        .iter()
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();

    build_action_fields_to_spec(
        repo_root,
        BuildActionFields {
            id: action.id.clone(),
            command: action.command.clone(),
            env,
            input_globs,
            outputs,
            working_dir,
        },
    )
}
