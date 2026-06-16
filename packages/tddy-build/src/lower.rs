//! Lower a declared [`BuildTarget`] into the concrete [`BuildAction`]s that build it.
//!
//! Dispatch by `config.type`:
//! - `script` → the declared command (built-in)
//! - `tool` / `group` → no own action (built-in, structural)
//! - any other type → the [`BuildPlugin`](crate::plugin::BuildPlugin) registered for it
//!
//! Any explicit `actions` declared on the target are included first, as-is. The
//! engine has no knowledge of specific ecosystem target types — those live in
//! plugin crates.

use crate::builtin::{self, GROUP, SCRIPT, TOOL};
use crate::error::BuildError;
use crate::manifest::BuildTarget;
use crate::plugin::{LowerContext, PluginRegistry};
use crate::proto::BuildAction;

/// Produce the ordered actions for a single target.
pub fn lower_target(
    target: &BuildTarget,
    registry: &PluginRegistry,
) -> Result<Vec<BuildAction>, BuildError> {
    let mut actions = target.actions.clone();

    if let Some(config) = &target.config {
        match config.r#type.as_str() {
            SCRIPT => actions.push(builtin::script_action(&config.fields)?),
            TOOL | GROUP => {}
            other => {
                let plugin = registry
                    .get(other)
                    .ok_or_else(|| BuildError::Manifest(format!("unknown target type: {other}")))?;
                let ctx = LowerContext {
                    type_name: other,
                    target_id: &target.id,
                    target_name: &target.name,
                    deps: &target.deps,
                    config: &config.fields,
                };
                actions.extend(plugin.lower(&ctx)?);
            }
        }
    }

    Ok(actions)
}
