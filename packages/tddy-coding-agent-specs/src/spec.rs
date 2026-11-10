//! The `coding_agent` extension: its schema, its owner, and the validated view of one agent.

pub(crate) mod acp;
pub(crate) mod argv;
pub(crate) mod headless_section;
pub(crate) mod pass_env;
pub(crate) mod route_slot;
pub(crate) mod slot_shapes;

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use tddy_actions::tool::{ExtensionOwner, RunSection, ToolDef};
use tddy_agent_backend::adapters::{AcpHooksAdapters, AdapterRegistry, LaunchAdapters};
use thiserror::Error;

use crate::capability::Capability;
use crate::headless::validate_headless;
use crate::route::RouteSupport;
use crate::slots::conversation::ConversationSlot;
use crate::slots::hooks;
use crate::slots::tools::render_flag_per_value;
use crate::spec::acp::AcpSection;
use crate::spec::headless_section::HeadlessSection;
use crate::spec::slot_shapes::InstructionsSlot;

/// The `extensions:` key this crate owns.
pub const CODING_AGENT_EXTENSION: &str = "coding_agent";

/// The [`ExtensionOwner`] of [`CODING_AGENT_EXTENSION`]: a section that does not deserialize as a
/// [`CodingAgentSection`] (an unknown field or slot kind) or that names an unknown hooks adapter is
/// refused, so a bad agent file never loads.
#[derive(Debug, Clone, Copy, Default)]
pub struct CodingAgentExtension;

impl ExtensionOwner for CodingAgentExtension {
    fn name(&self) -> &str {
        CODING_AGENT_EXTENSION
    }

    fn validate(&self, tool_id: &str, section: &serde_yaml::Value) -> Result<(), String> {
        // A spec whose `run.transport` is `acp` but which declares no `acp` section is refused where
        // an ACP launch reads the section: this seam sees the `coding_agent` section alone.
        // TODO(acp-agents): `ExtensionOwner::validate` would need the run section to refuse it here.
        let agent = parse_section(tool_id, section).map_err(|error| match error {
            SpecError::Invalid { message, .. } => message,
            other => other.to_string(),
        })?;
        if let Some(name) = agent
            .acp
            .as_ref()
            .and_then(|acp| acp.hooks_adapter.as_ref())
        {
            if !AcpHooksAdapters::builtin().contains(name) {
                return Err(format!(
                    "acp.hooks_adapter {name} is not an ACP hooks adapter this build has"
                ));
            }
        }
        let launch_adapters = LaunchAdapters::builtin();
        if let Some(name) = &agent.models.adapter {
            if launch_adapters.models(name).is_none() {
                return Err(format!(
                    "models.adapter {name} is not a launch adapter this build has"
                ));
            }
        }
        if agent.models.list_command.is_some() != agent.models.adapter.is_some() {
            return Err(
                "models.list_command and models.adapter are declared together or not at all"
                    .to_string(),
            );
        }
        if let Some(resume) = &agent.resume {
            if launch_adapters.resume_token(&resume.adapter).is_none() {
                return Err(format!(
                    "resume.adapter {} is not a launch adapter this build has",
                    resume.adapter
                ));
            }
        }
        match &agent.conversation {
            Some(ConversationSlot::Minted { adapter, .. })
                if launch_adapters.conversation_id(adapter).is_none() =>
            {
                return Err(format!(
                    "conversation.adapter {adapter} is not a launch adapter this build has"
                ));
            }
            Some(ConversationSlot::ResumeToken) if agent.resume.is_none() => {
                return Err(
                    "conversation kind resume_token needs a resume.adapter to read its token with"
                        .to_string(),
                );
            }
            _ => {}
        }
        if agent.placements.is_empty() {
            return Err(
                "placements is empty: an agent runs on at least one of host, jail".to_string(),
            );
        }
        if let Some(headless) = &agent.headless {
            validate_headless(tool_id, headless, &AdapterRegistry::builtin())
                .map_err(|error| error.to_string())?;
        }
        Ok(())
    }
}

/// One coding agent: the tool's id, its run section, and its `coding_agent` section.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodingAgentSpec {
    pub id: String,
    pub run: RunSection,
    pub agent: CodingAgentSection,
}

impl CodingAgentSpec {
    /// The coding-agent view of `def`: refused when it has no `run:` section or no
    /// `coding_agent` extension, or when that extension does not validate.
    pub fn of(def: &ToolDef) -> Result<Self, SpecError> {
        let run = def.run.clone().ok_or_else(|| SpecError::NoRunSection {
            tool: def.id.clone(),
        })?;
        let section = def.extensions.get(CODING_AGENT_EXTENSION).ok_or_else(|| {
            SpecError::NotACodingAgent {
                tool: def.id.clone(),
            }
        })?;
        Ok(Self {
            id: def.id.clone(),
            run,
            agent: parse_section(&def.id, section)?,
        })
    }
}

/// `section` as a [`CodingAgentSection`] whose hooks adapter this build has.
fn parse_section(
    tool_id: &str,
    section: &serde_yaml::Value,
) -> Result<CodingAgentSection, SpecError> {
    let invalid = |message: String| SpecError::Invalid {
        tool: tool_id.to_string(),
        message,
    };
    let agent: CodingAgentSection =
        serde_yaml::from_value(section.clone()).map_err(|e| invalid(e.to_string()))?;
    hooks::adapter(&agent.hooks.adapter).map_err(|e| invalid(e.to_string()))?;
    validate_env_entries(&agent.pass_env)
        .map_err(|e| format!("pass_env: {e}"))
        .map_err(invalid)?;
    if let Some(route) = &agent.route {
        route_slot_errors(route)
            .map_err(|e| format!("route: {e}"))
            .map_err(invalid)?;
    }
    Ok(agent)
}

/// Why a tool is not a usable coding agent.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum SpecError {
    #[error("tool {tool} has no coding_agent extension")]
    NotACodingAgent { tool: String },
    #[error("tool {tool} has no run section")]
    NoRunSection { tool: String },
    #[error("tool {tool}: coding_agent: {message}")]
    Invalid { tool: String, message: String },
}

/// The `coding_agent` section of an agent's tool file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CodingAgentSection {
    /// Where the tool server registration goes, and in what shape.
    pub mcp: McpSlot,
    /// The flag each pre-approved tool is passed with — or `unsupported`.
    pub tool_allow: ToolSlot,
    /// The flag each withdrawn tool is passed with — or `unsupported`.
    pub tool_deny: ToolSlot,
    /// The flag naming the MCP tool the agent asks before running anything not pre-approved — or
    /// `unsupported`.
    pub permission_prompt: ToolSlot,
    pub instructions: InstructionsSlot,
    pub hooks: HooksSlot,
    pub models: ModelsSection,
    /// What context sync copies from a codebase into the agent's working directory.
    #[serde(default)]
    pub context_globs: Vec<String>,
    /// How the agent runs headless — one invocation per turn, its output read by a named adapter
    /// (`#agent-tools 6/12`). `None` for an agent with no headless mode.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub headless: Option<HeadlessSection>,
    /// What a provider route needs of this agent (`#agent-tools 8/12`): whether a jailed launch
    /// requires one, and how the route's file and key reach the agent. `None` for an agent whose
    /// route is environment only and optional in a jail.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub route: Option<RouteSlot>,
    /// How a resume finds the agent's own conversation (`#agent-tools 8/12`): the adapter that reads
    /// the token off the exited session's terminal. `None` for an agent resumed by an id the
    /// daemon owns (Claude's session id, Cursor's chat id) or by its worktree (`--continue`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resume: Option<ResumeSlot>,
    /// Where the agent may run (`#agent-tools 8/12`). Absent means both; `[host]` refuses a
    /// sandboxed start up front, naming the placements ([`CodingAgentSpec::require_placement`]).
    #[serde(default = "Placement::anywhere")]
    pub placements: Vec<Placement>,
    /// Named ways of running this same agent with fixed extra flags and native tools withdrawn
    /// (`#agent-tools 10/12`) — `host_agent`, the agent a split session runs on this host against a
    /// codebase that lives elsewhere. Empty for an agent with none.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub variants: BTreeMap<String, AgentVariant>,
    /// How the agent runs over the Agent Client Protocol (`#agent-tools 11/12`): its launch
    /// command, the hooks adapter its driver runs with, and how a managed launch withdraws its
    /// native tools. `None` for an agent with no ACP mode.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub acp: Option<AcpSection>,
    /// What a session of the agent offers (`#agent-tools 12/12`): every gate that used to compare
    /// a session type with `"claude-cli"` reads this instead ([`CodingAgentSpec::has`]). A word that
    /// is not a [`Capability`] is refused at load. Empty for an agent that offers none.
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub capabilities: BTreeSet<Capability>,
    /// How long an orchestrated session's permission hook waits for its orchestrator's answer
    /// before the agent shows its own prompt, in seconds (`#agent-tools 12/12`; was daemon.yaml
    /// `claude_cli.answer_wait_secs`). [`DEFAULT_ANSWER_WAIT_SECS`] when absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub answer_wait_secs: Option<u64>,
    /// The operator-facing name a route refusal leads with (`#agent-tools 13/19`): `Claude Code`,
    /// `Muse Code`. Read through [`CodingAgentSpec::product`], which answers the id when absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub product: Option<String>,
    /// How the session's own conversation is identified across its start and resumes
    /// (`#agent-tools 14/19`): the daemon's session id, a minted id, the worktree, or a resume
    /// token. Read through [`CodingAgentSpec::conversation`], which derives it when absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub conversation: Option<ConversationSlot>,
    /// The permission modes the agent's launch takes, in picker order — the values its argv's
    /// `{permission_mode}` binding may be given. Empty for an agent whose launch binds no
    /// permission mode. A picker offers these rather than naming an agent's modes in its own
    /// code (the sweep: agent metadata lives only in YAML).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub permission_modes: Vec<String>,
    /// This agent leaves transcripts the host can total token usage from, when a transcript
    /// reader for its format exists. A summary that reads transcripts gates on this rather
    /// than naming the agent.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub transcript_usage: bool,
    /// The daemon's variables this agent's process is also started with, on top of the base set
    /// ([`crate::AgentEnvBase`]) — its provider keys and the agent-specific
    /// variables it reads (`#agent-tools 28/28`). Each entry is a name or a `PREFIX_*` pattern; one
    /// matching `TDDY_*` is refused at load. Empty for an agent that reads none.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pass_env: Vec<String>,
}

/// How long an orchestrated session's permission hook waits for its orchestrator's answer when the
/// agent's spec sets no `coding_agent.answer_wait_secs`.
pub const DEFAULT_ANSWER_WAIT_SECS: u64 = 120;

impl CodingAgentSpec {
    /// `coding_agent.answer_wait_secs`, or [`DEFAULT_ANSWER_WAIT_SECS`].
    pub fn answer_wait_secs(&self) -> u64 {
        self.agent
            .answer_wait_secs
            .unwrap_or(DEFAULT_ANSWER_WAIT_SECS)
    }

    /// The capabilities the agent declares.
    pub fn capabilities(&self) -> &BTreeSet<Capability> {
        &self.agent.capabilities
    }

    /// Whether the agent declares `capability` — the question every agent gate asks.
    pub fn has(&self, capability: Capability) -> bool {
        self.agent.capabilities.contains(&capability)
    }

    /// What a provider route can do with the agent: `coding_agent.route.providers` and `.shape`, or
    /// no providers when the spec has no `route` section — an agent that cannot be routed.
    pub fn route_support(&self) -> RouteSupport {
        match &self.agent.route {
            Some(route) => RouteSupport {
                providers: route.providers.clone(),
                shape: route.shape,
            },
            None => RouteSupport::default(),
        }
    }

    /// `coding_agent.conversation`; when the spec declares none, [`ConversationSlot::ResumeToken`]
    /// for a spec with a `resume` slot and [`ConversationSlot::Worktree`] otherwise — what the
    /// generic launch resumed by before the slot existed.
    pub fn conversation(&self) -> ConversationSlot {
        match (&self.agent.conversation, &self.agent.resume) {
            (Some(declared), _) => declared.clone(),
            (None, Some(_)) => ConversationSlot::ResumeToken,
            (None, None) => ConversationSlot::Worktree,
        }
    }

    /// `coding_agent.product`, or the agent id when the spec declares none.
    pub fn product(&self) -> &str {
        self.agent.product.as_deref().unwrap_or(&self.id)
    }
}

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
}

pub use crate::spec::route_slot::{Placement, ResumeSlot, RouteSlot};

use crate::spec::pass_env::validate_env_entries;
use crate::spec::route_slot::route_slot_errors;
pub use crate::spec::slot_shapes::{HooksSlot, McpSlot, ToolSlot};

pub use crate::spec::argv::ModelsSection;

#[cfg(test)]
mod tool_slot_tests {
    use pretty_assertions::assert_eq;

    use crate::spec::slot_shapes::FlagSlot;
    use crate::spec::slot_shapes::ToolSlot;
    use crate::spec::slot_shapes::Unsupported;

    #[test]
    fn a_tool_slot_reads_a_flag_map() {
        // When
        let slot: ToolSlot = serde_yaml::from_str("{ flag: --disallowedTools }").unwrap();

        // Then
        assert_eq!(
            slot,
            ToolSlot::Flag(FlagSlot {
                flag: "--disallowedTools".to_string(),
            })
        );
    }

    #[test]
    fn a_tool_slot_reads_the_word_unsupported() {
        // When
        let slot: ToolSlot = serde_yaml::from_str("unsupported").unwrap();

        // Then
        assert_eq!(slot, ToolSlot::Unsupported(Unsupported::Unsupported));
        assert_eq!(slot.flag(), None);
    }

    #[test]
    fn any_other_word_is_refused() {
        // When
        let refused = serde_yaml::from_str::<ToolSlot>("none");

        // Then
        assert!(refused.is_err(), "`none` must not read as a tool slot");
    }
}

#[cfg(test)]
mod placement_tests {
    use pretty_assertions::assert_eq;
    use tddy_actions::tool::RunSection;

    use super::{CodingAgentSection, CodingAgentSpec};
    use crate::spec::route_slot::Placement;
    use crate::spec::route_slot::PlacementError;

    /// The coding-agent view of a built-in spec file, read straight from its YAML: its `run` and
    /// `extensions.coding_agent` sections deserialized as they are declared.
    fn the_builtin_spec(id: &str, yaml: &str) -> CodingAgentSpec {
        let document: serde_yaml::Value = serde_yaml::from_str(yaml).unwrap();
        let run: RunSection = serde_yaml::from_value(document["run"].clone()).unwrap();
        let agent: CodingAgentSection =
            serde_yaml::from_value(document["extensions"]["coding_agent"].clone()).unwrap();
        CodingAgentSpec {
            id: id.to_string(),
            run,
            agent,
        }
    }

    fn warp() -> CodingAgentSpec {
        the_builtin_spec("warp", crate::WARP_SPEC_YAML)
    }

    #[test]
    fn a_host_only_spec_refuses_the_jail_naming_its_placements() {
        // When
        let refused = warp().require_placement(Placement::Jail);

        // Then
        assert_eq!(
            (
                refused.clone(),
                refused.map_err(|error| error.to_string()).unwrap_err()
            ),
            (
                Err(PlacementError::NotPlaced {
                    tool: "warp".to_string(),
                    placement: Placement::Jail,
                    placements: vec![Placement::Host],
                }),
                "warp cannot run in the jail: its spec's placements are [host]".to_string(),
            )
        );
    }

    #[test]
    fn a_host_only_spec_admits_the_host() {
        // When
        let admitted = warp().require_placement(Placement::Host);

        // Then
        assert_eq!(admitted, Ok(()));
    }

    #[test]
    fn a_spec_that_declares_no_placements_runs_on_the_host_and_in_a_jail() {
        // Given — claude.yaml declares no `placements`
        let claude = the_builtin_spec("claude", include_str!("../specs/claude.yaml"));

        // When
        let admitted = (
            claude.agent.placements.clone(),
            claude.require_placement(Placement::Host),
            claude.require_placement(Placement::Jail),
        );

        // Then
        assert_eq!(
            admitted,
            (vec![Placement::Host, Placement::Jail], Ok(()), Ok(()))
        );
    }
}

#[cfg(test)]
mod extension_validation_tests {
    use pretty_assertions::assert_eq;
    use tddy_actions::tool::ExtensionOwner;

    use super::CodingAgentExtension;

    /// The `coding_agent` section of the built-in OpenCode spec, as YAML.
    fn the_opencode_section() -> serde_yaml::Value {
        let document: serde_yaml::Value = serde_yaml::from_str(crate::OPENCODE_SPEC_YAML).unwrap();
        document["extensions"]["coding_agent"].clone()
    }

    fn validate(section: &serde_yaml::Value) -> Result<(), String> {
        CodingAgentExtension.validate("opencode", section)
    }

    #[test]
    fn the_builtin_opencode_section_validates() {
        // When
        let outcome = validate(&the_opencode_section());

        // Then
        assert_eq!(outcome, Ok(()));
    }

    #[test]
    fn a_models_adapter_the_build_does_not_hold_is_refused_naming_it() {
        // Given
        let mut section = the_opencode_section();
        section["models"]["adapter"] = serde_yaml::Value::String("no-such-adapter".to_string());

        // When
        let refused = validate(&section);

        // Then
        assert_eq!(
            refused,
            Err(
                "models.adapter no-such-adapter is not a launch adapter this build has".to_string()
            )
        );
    }

    #[test]
    fn a_models_list_command_without_its_adapter_is_refused() {
        // Given
        let mut section = the_opencode_section();
        section["models"]
            .as_mapping_mut()
            .unwrap()
            .remove("adapter");

        // When
        let refused = validate(&section);

        // Then
        assert_eq!(
            refused,
            Err(
                "models.list_command and models.adapter are declared together or not at all"
                    .to_string()
            )
        );
    }

    #[test]
    fn a_resume_adapter_the_build_does_not_hold_is_refused_naming_it() {
        // Given
        let mut section = the_opencode_section();
        section["resume"] = serde_yaml::from_str("adapter: no-such-adapter").unwrap();

        // When
        let refused = validate(&section);

        // Then
        assert_eq!(
            refused,
            Err(
                "resume.adapter no-such-adapter is not a launch adapter this build has".to_string()
            )
        );
    }

    #[test]
    fn an_empty_placements_list_is_refused() {
        // Given
        let mut section = the_opencode_section();
        section["placements"] = serde_yaml::Value::Sequence(Vec::new());

        // When
        let refused = validate(&section);

        // Then
        assert_eq!(
            refused,
            Err("placements is empty: an agent runs on at least one of host, jail".to_string())
        );
    }
}
