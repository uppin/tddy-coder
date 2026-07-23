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
use crate::capabilities::BuildMode;
use crate::error::BuildError;
use crate::manifest::BuildTarget;
use crate::plugin::{LowerContext, PluginRegistry};
use crate::proto::BuildAction;

/// Produce the ordered actions for a single target in the requested [`BuildMode`].
///
/// For [`BuildMode::Compile`], explicit `actions` run first, followed by the config-lowered compile
/// actions. For Test/Run only the config plugin's mode-specific actions are emitted; the built-in
/// structural types (`script`/`tool`/`group`) support Compile only and reject other modes.
pub fn lower_target(
    target: &BuildTarget,
    mode: BuildMode,
    registry: &PluginRegistry,
) -> Result<Vec<BuildAction>, BuildError> {
    let mut actions = if mode == BuildMode::Compile {
        target.actions.clone()
    } else {
        Vec::new()
    };

    if let Some(config) = &target.config {
        match config.r#type.as_str() {
            SCRIPT | TOOL | GROUP if mode != BuildMode::Compile => {
                return Err(BuildError::Manifest(format!(
                    "target type '{}' does not support {}",
                    config.r#type,
                    mode.label()
                )));
            }
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
                actions.extend(plugin.lower_mode(&ctx, mode)?);
            }
        }
    }

    if let Some(config) = &target.config {
        log::debug!(
            "lowered target {} (type {}) into {} action(s)",
            target.id,
            config.r#type,
            actions.len()
        );
    } else {
        log::debug!(
            "lowered target {} into {} explicit action(s)",
            target.id,
            actions.len()
        );
    }

    Ok(actions)
}
