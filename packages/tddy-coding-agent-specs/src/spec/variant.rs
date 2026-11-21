use crate::slots::tools::render_flag_per_value;
use crate::spec::CodingAgentSpec;
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// The variant a split session's agent runs as: on this host, against a codebase on another daemon
/// or in a jail, so nothing from the checkout may configure it and none of its native filesystem or
/// shell tools may run (`#agent-tools 10/12`).
pub const HOST_AGENT_VARIANT: &str = "host_agent";

/// `coding_agent.variants.<name>`: what a variant adds to the agent's ordinary launch.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AgentVariant {
    /// Fixed flags, appended after the run template's own and before the slot flags.
    #[serde(default)]
    pub args: Vec<String>,
    /// The agent's own tools the variant withdraws, each passed through the `tool_deny` slot.
    #[serde(default)]
    pub withdraw_native: Vec<String>,
    /// The flag that loads the hooks file the daemon writes for this launch, for a variant whose
    /// own `args` keep the agent from finding that file by itself (Claude's `--setting-sources
    /// user` skips `.claude/settings.local.json`). `None`: the variant loads hooks as usual.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hooks_flag: Option<String>,
}

/// Why a variant cannot be rendered.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum VariantError {
    #[error("agent {agent} declares no variant {variant:?}; it declares {declared:?}")]
    Unknown {
        agent: String,
        variant: String,
        declared: Vec<String>,
    },
    #[error(
        "agent {agent}: variant {variant:?} withdraws native tools, but its tool_deny slot is unsupported"
    )]
    DenyUnsupported { agent: String, variant: String },
}

impl CodingAgentSpec {
    /// The flags variant `name` adds to a launch: its `args`, then `tool_deny`'s flag once per
    /// `withdraw_native` tool. Refused for a variant the agent does not declare, naming the ones it
    /// does, and for one that withdraws tools through an `unsupported` deny slot.
    pub fn variant_args(&self, name: &str) -> Result<Vec<String>, VariantError> {
        let variant = self
            .agent
            .variants
            .get(name)
            .ok_or_else(|| VariantError::Unknown {
                agent: self.id.clone(),
                variant: name.to_string(),
                declared: self.agent.variants.keys().cloned().collect(),
            })?;
        let mut args = variant.args.clone();
        if !variant.withdraw_native.is_empty() {
            let deny =
                self.agent
                    .tool_deny
                    .flag()
                    .ok_or_else(|| VariantError::DenyUnsupported {
                        agent: self.id.clone(),
                        variant: name.to_string(),
                    })?;
            args.extend(render_flag_per_value(deny, &variant.withdraw_native));
        }
        Ok(args)
    }

    /// The flags variant `name` loads the hooks file at `hooks_file` with: its `hooks_flag` and the
    /// path, or nothing when it declares none. Refused, as [`Self::variant_args`] is, for a variant
    /// the agent does not declare.
    pub fn variant_hooks_args(
        &self,
        name: &str,
        hooks_file: &std::path::Path,
    ) -> Result<Vec<String>, VariantError> {
        let variant = self
            .agent
            .variants
            .get(name)
            .ok_or_else(|| VariantError::Unknown {
                agent: self.id.clone(),
                variant: name.to_string(),
                declared: self.agent.variants.keys().cloned().collect(),
            })?;
        Ok(match &variant.hooks_flag {
            Some(flag) => vec![flag.clone(), hooks_file.to_string_lossy().into_owned()],
            None => Vec::new(),
        })
    }
}
