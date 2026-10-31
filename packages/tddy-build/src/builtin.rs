//! Built-in structural target types handled directly by the engine.
//!
//! `script` (a generic command), `tool` (a `PATH` provider), and `group` (a graph
//! aggregation) are part of the build-graph vocabulary itself rather than
//! ecosystem recipes, so they are not plugins. Their config is parsed on demand
//! from the target's open [`crate::manifest::TargetConfig`] fields.

use std::collections::HashMap;

use serde::Deserialize;
use tddy_actions::tool::ToolDef;

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

// TODO(build-tools): delete this legacy shape once the executor resolves tools through
// `tool_target`; until then `tool_bin_dir` keeps today's PATH behaviour for any manifest still
// using it.
#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct LegacyToolTarget {
    bin_dir: String,
    #[allow(dead_code)]
    commands: HashMap<String, String>,
}

/// A `type: tool` target's config (`#agent-tools 4/12`): a tool definition written inline, or a
/// reference to one the tool registry already holds.
///
/// An action names the **target** in `tool_dep_ids`; the executor maps it to a tool id — the inline
/// definition's `id`, or `ref` — and resolves that id's closure through
/// [`tddy_actions::tool::ToolRegistry::resolve_closure`]. An inline id that the registry already
/// holds is refused as a duplicate, never silently preferred.
///
/// The legacy `{bin_dir, commands}` shape is refused by [`tool_target`] with a message naming the
/// target and the replacement: a directory on `PATH` grants every binary in it, which is exactly
/// what a hermetic action must not get.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToolTarget {
    /// The target's config fields (all but `type`) are a whole [`ToolDef`] (boxed: it dwarfs a
    /// `Ref`).
    Inline(Box<ToolDef>),
    /// `ref: <tool id>` — a built-in or a `<tddyhome>/tools/<id>.yaml` override.
    Ref { ref_: String },
}

/// Parse `target_id`'s `type: tool` config into a [`ToolTarget`].
///
/// The legacy `{bin_dir, commands}` shape is a [`BuildError::Manifest`] reading
/// `tool target <id>: the {bin_dir, commands} tool shape is no longer supported — declare a tool
/// definition (schema_version, id, summary, binary) or ref: <tool id> instead`.
pub fn tool_target(target_id: &str, fields: &serde_yaml::Value) -> Result<ToolTarget, BuildError> {
    // TODO(build-tools): implement — `ref` alone is a `Ref`; `bin_dir` or `commands` present is
    // the legacy refusal; anything else is `ToolDef` YAML (errors name `target_id`).
    let _ = (target_id, fields);
    todo!("build-tools: tool_target")
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

/// The `bin_dir` a legacy `tool` target prepends onto dependents' `PATH`.
// TODO(build-tools): delete with `LegacyToolTarget` once the executor resolves tool closures.
pub fn tool_bin_dir(fields: &serde_yaml::Value) -> Result<String, BuildError> {
    let tool: LegacyToolTarget = parse(fields, TOOL)?;
    Ok(tool.bin_dir)
}

/// The member target ids a `group` aggregates (build-order predecessors).
pub fn group_member_ids(fields: &serde_yaml::Value) -> Result<Vec<String>, BuildError> {
    let group: GroupTarget = parse(fields, GROUP)?;
    Ok(group.member_ids)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use pretty_assertions::assert_eq;
    use tddy_actions::tool::{NetworkSection, TargetBinary, ToolBinary};

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

    // TODO(build-tools): delete with `tool_bin_dir` — the legacy shape is refused, see
    // `the_legacy_tool_shape_is_refused_naming_the_target_and_the_migration`.
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
    fn an_inline_tool_config_is_a_whole_tool_definition() {
        // Given — a tool built by another target
        let config = yaml(
            "schema_version: 1\n\
             id: stamp\n\
             summary: stamps its input\n\
             binary:\n  \
               command: stamp\n  \
               target: { target: \"tools:stamp-build\", path: tools/out/stamp }\n",
        );

        // When
        let tool = tool_target("tools:stamp", &config).expect("an inline tool parses");

        // Then
        assert_eq!(
            tool,
            ToolTarget::Inline(Box::new(ToolDef {
                schema_version: 1,
                id: "stamp".into(),
                summary: "stamps its input".into(),
                binary: ToolBinary {
                    command: "stamp".into(),
                    env: None,
                    target: Some(TargetBinary {
                        target: "tools:stamp-build".into(),
                        path: "tools/out/stamp".into(),
                    }),
                },
                deps: vec![],
                reads: vec![],
                run: None,
                extensions: BTreeMap::new(),
                network: NetworkSection::default(),
            }))
        );
    }

    #[test]
    fn a_ref_tool_config_names_a_registry_tool() {
        // Given
        let config = yaml("ref: rg\n");

        // When
        let tool = tool_target("tools:rg", &config).expect("a ref parses");

        // Then
        assert_eq!(tool, ToolTarget::Ref { ref_: "rg".into() });
    }

    #[test]
    fn the_legacy_tool_shape_is_refused_naming_the_target_and_the_migration() {
        // Given
        let config = yaml("bin_dir: tools/bin\ncommands: { stamp: stamp }\n");

        // When
        let err = tool_target("tools:bin", &config).expect_err("the legacy shape is refused");

        // Then
        assert_eq!(
            err.to_string(),
            "manifest error: tool target tools:bin: the {bin_dir, commands} tool shape is no \
             longer supported — declare a tool definition (schema_version, id, summary, binary) \
             or ref: <tool id> instead"
        );
    }

    #[test]
    fn a_ref_with_inline_definition_fields_is_refused_naming_the_target() {
        // Given — `ref` and a `binary` together are neither shape
        let config = yaml("ref: rg\nbinary: { command: rg }\n");

        // When
        let err = tool_target("tools:rg", &config).expect_err("a mixed shape is refused");

        // Then
        assert!(
            matches!(&err, BuildError::Manifest(m) if m.starts_with("tool target tools:rg: ")),
            "the refusal names the target: {err}"
        );
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
