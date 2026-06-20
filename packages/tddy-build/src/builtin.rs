//! Built-in structural target types handled directly by the engine.
//!
//! `script` (a generic command), `tool` (a `PATH` provider), and `group` (a graph
//! aggregation) are part of the build-graph vocabulary itself rather than
//! ecosystem recipes, so they are not plugins. Their config is parsed on demand
//! from the target's open [`crate::manifest::TargetConfig`] fields.

use std::collections::HashMap;

use serde::Deserialize;

use crate::error::BuildError;
use crate::proto::{ActionType, BuildAction};

/// `config.type` tag for the generic command target.
pub const SCRIPT: &str = "script";
/// `config.type` tag for a `PATH`-providing tool target.
pub const TOOL: &str = "tool";
/// `config.type` tag for a target-group aggregation.
pub const GROUP: &str = "group";

/// Whether `type_name` is one of the engine's built-in structural types.
pub fn is_builtin(type_name: &str) -> bool {
    matches!(type_name, SCRIPT | TOOL | GROUP)
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct ScriptTarget {
    command: Vec<String>,
    env: HashMap<String, String>,
    working_dir_rel: Vec<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct ToolTarget {
    bin_dir: String,
    #[allow(dead_code)]
    commands: HashMap<String, String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct GroupTarget {
    member_ids: Vec<String>,
}

fn parse<T>(fields: &serde_yaml::Value, what: &str) -> Result<T, BuildError>
where
    T: for<'de> Deserialize<'de>,
{
    serde_yaml::from_value(fields.clone())
        .map_err(|e| BuildError::Manifest(format!("invalid {what} config: {e}")))
}

/// Lower a `script` target's config into its single command action.
pub fn script_action(fields: &serde_yaml::Value) -> Result<BuildAction, BuildError> {
    let script: ScriptTarget = parse(fields, SCRIPT)?;
    // Empty `working_dir_rel` joins to "" — the proto default (run in repo root).
    let working_dir = script.working_dir_rel.join("/");
    Ok(BuildAction {
        id: "script".to_string(),
        description: "script".to_string(),
        r#type: ActionType::Command as i32,
        command: script.command,
        env: script.env,
        working_dir,
        ..Default::default()
    })
}

/// The `bin_dir` a `tool` target prepends onto dependents' `PATH`.
pub fn tool_bin_dir(fields: &serde_yaml::Value) -> Result<String, BuildError> {
    let tool: ToolTarget = parse(fields, TOOL)?;
    Ok(tool.bin_dir)
}

/// The member target ids a `group` aggregates (build-order predecessors).
pub fn group_member_ids(fields: &serde_yaml::Value) -> Result<Vec<String>, BuildError> {
    let group: GroupTarget = parse(fields, GROUP)?;
    Ok(group.member_ids)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn yaml(text: &str) -> serde_yaml::Value {
        serde_yaml::from_str(text).expect("valid yaml")
    }

    #[test]
    fn script_passes_command_env_and_working_dir() {
        // Given
        let config = yaml("command: [echo, hi]\nenv: { K: V }\nworking_dir_rel: [sub, dir]\n");

        // When
        let action = script_action(&config).expect("script lowers");

        // Then
        assert_eq!(action.command, vec!["echo".to_string(), "hi".to_string()]);
        assert_eq!(action.working_dir, "sub/dir");
        assert_eq!(action.env.get("K").map(String::as_str), Some("V"));
        assert_eq!(action.r#type, ActionType::Command as i32);
    }

    #[test]
    fn tool_extracts_bin_dir() {
        // Given
        let config = yaml("bin_dir: tools/bin\ncommands: { greet: greet }\n");

        // When
        let bin_dir = tool_bin_dir(&config).expect("tool parses");

        // Then
        assert_eq!(bin_dir, "tools/bin");
    }

    #[test]
    fn group_extracts_member_ids() {
        // Given
        let config = yaml("member_ids: [a, b]\n");

        // When
        let members = group_member_ids(&config).expect("group parses");

        // Then
        assert_eq!(members, vec!["a".to_string(), "b".to_string()]);
    }

    #[test]
    fn unknown_builtin_field_is_rejected() {
        // Given
        let config = yaml("command: [echo]\nbogus: 1\n");

        // When
        let err = script_action(&config).expect_err("unknown field must error");

        // Then
        assert!(matches!(err, BuildError::Manifest(_)));
    }
}
