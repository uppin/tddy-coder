//! What an `opencode-cli`, `muse-cli` or `warp-cli` launch binds when it renders its tool
//! definition (`tddy-coding-agent-specs/specs/{opencode,muse,warp}.yaml`) — on the host through
//! [`crate::spec_launch::launch_host_pty`], in a jail through
//! [`crate::launch_ports::LaunchSessions::launch_jailed`] — and the spec-driven refusal of a
//! placement the spec excludes.
//!
//! PRD: `docs/ft/daemon/1-WIP/PRD-2026-10-31-harness-clis-from-specs.md` (`#agent-tools 8/12`).
//!
//! Nothing here builds an argv: these are the template parameters the three specs name. A resume
//! is `resume` for the agents that continue their worktree's session (`--continue`), and
//! `resume_token` for Warp, whose token the `warp-resume-token` adapter read off the last run.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use tddy_actions::tool::TemplateParams;
use tddy_coding_agent_specs::{
    load_agent_registry, spec_for_session_type, CodingAgentSpec, Placement,
};
use tddy_core::adapters::{LaunchAdapters, ResumeTokenAdapter};
use tddy_core::provider_route::Harness;
use tddy_daemon_kernel::config::DaemonConfig;
use tddy_rpc::Status;

use crate::launch_ports::LaunchSessions;
use crate::spec_launch::{HostAgent, HostCodingAgents};

/// The `opencode` tool's id in the agent registry.
pub const OPENCODE_AGENT_ID: &str = "opencode";
/// The `muse` tool's id in the agent registry.
pub const MUSE_AGENT_ID: &str = "muse";
/// The `warp` tool's id in the agent registry.
pub const WARP_AGENT_ID: &str = "warp";

/// One start or resume of a CLI harness, as its spec's template is rendered for it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct HarnessLaunch<'a> {
    /// The model the operator chose; empty when none was.
    pub model: &'a str,
    /// The first turn, when the session starts with one.
    pub prompt: Option<&'a str>,
    /// A resume of an existing session rather than its first start.
    pub resume: bool,
    /// The token the agent's last run printed (`harness_resume_token`), when one was recorded.
    pub resume_token: Option<&'a str>,
}

/// The parameters `opencode.yaml`, `muse.yaml` and `warp.yaml`'s `run.argv` is rendered with:
/// `model` and `prompt` each bound when non-empty (trimmed) on a start; `resume` set on a resume,
/// with `resume_token` bound when a non-blank one was recorded. A resume binds neither model nor
/// prompt — the agent continues the session it had.
pub fn harness_template_params(launch: &HarnessLaunch<'_>) -> TemplateParams {
    if launch.resume {
        return TemplateParams::new()
            .with_flag("resume", true)
            .with_opt("resume_token", non_blank(launch.resume_token));
    }
    TemplateParams::new()
        .with_opt("model", non_blank(Some(launch.model)))
        .with_opt("prompt", non_blank(launch.prompt))
}

/// `value` trimmed, or `None` when nothing but whitespace is left.
fn non_blank(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|value| !value.is_empty())
}

/// `Ok` when `spec`'s placements include `placement`; otherwise `FAILED_PRECONDITION` naming the
/// session type and the placements the spec declares — the refusal of a sandboxed `warp-cli`
/// start, read from `warp.yaml`'s `placements: [host]` instead of `sandboxed_agent_kind`'s match.
pub fn require_spec_placement(
    spec: &CodingAgentSpec,
    session_type: &str,
    placement: Placement,
) -> Result<(), Status> {
    spec.require_placement(placement).map_err(|error| {
        Status::failed_precondition(format!(
            "the sandbox runner does not host {session_type}: {error}"
        ))
    })
}

/// A CLI session type the specs launch, as the launch path reads it: its name as a static string, the
/// spec that launches it and the provider harness its route is resolved for. It replaces the
/// `CliHarness` descriptor on the spec-driven paths; every field has a home in the spec or the route.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct CliSessionType {
    /// The session type's name (`opencode-cli`). A route harness's id is its session type's name,
    /// so this is the static string the worktree and branch policy take.
    pub name: &'static str,
    /// The id of the spec that launches the session type (`opencode`).
    pub spec_id: &'static str,
    /// The provider harness the session's route is resolved for.
    pub route: Harness,
}

/// The CLI session type `session_type` names, refused when no spec launches it: there is no other
/// launch for a session type, so a missing spec is an error, not a fall back to a hand-built one.
pub(crate) fn cli_session_type(session_type: &str) -> Result<CliSessionType, Status> {
    let spec_id = cli_session_spec_id(session_type)?;
    let route = Harness::from_agent_or_session_type(session_type).ok_or_else(|| {
        Status::failed_precondition(format!("no provider route serves {session_type}"))
    })?;
    Ok(CliSessionType {
        name: route.id(),
        spec_id,
        route,
    })
}

/// The coding agent a CLI session type launches from: its spec's id.
fn cli_session_spec_id(session_type: &str) -> Result<&'static str, Status> {
    spec_for_session_type(session_type).ok_or_else(|| {
        Status::failed_precondition(format!("no coding agent spec launches {session_type}"))
    })
}

/// The binary `spec_id` runs for a `session_type` session: `cli_harnesses.<session type>.binary_path`
/// after the binary's own environment variable, as the daemon has always read them, when that names a
/// path. `None` leaves the binary to the spec's own resolution on the host.
pub(crate) fn configured_cli_binary(
    agents: &HostCodingAgents,
    config: &DaemonConfig,
    session_type: &str,
    spec_id: &str,
) -> Option<PathBuf> {
    let binary_env = agents
        .registry()
        .get(spec_id)
        .and_then(|def| def.binary.env.clone())
        .unwrap_or_default();
    let configured = config.cli_harness_binary(session_type, "", &binary_env);
    configured.contains('/').then(|| PathBuf::from(configured))
}

/// A CLI session's coding agent as this host satisfies it: the session type, the agent resolved from
/// the installed coding agents, and those agents (a jail reads their registry and host).
pub(crate) struct CliSessionAgent {
    pub kind: CliSessionType,
    pub agent: HostAgent,
    pub agents: Arc<HostCodingAgents>,
}

/// The coding agent a CLI session of `kind` launches, resolved before anything is cut or spawned.
/// Refused when the host has no coding agents: the spec is the only launch these sessions have, so
/// there is no hand-built one to use.
pub(crate) fn resolve_session_agent(
    agents: Option<&Arc<HostCodingAgents>>,
    config: &DaemonConfig,
    kind: CliSessionType,
) -> Result<CliSessionAgent, Status> {
    let agents = agents.cloned().ok_or_else(|| {
        Status::failed_precondition(format!(
            "this host has no coding agents installed: a {} session is launched from its tool \
             definition",
            kind.name
        ))
    })?;
    let binary = configured_cli_binary(&agents, config, kind.name, kind.spec_id);
    let agent = agents
        .resolve(kind.spec_id, binary.as_deref())
        .map_err(|e| Status::failed_precondition(e.to_string()))?;
    Ok(CliSessionAgent {
        kind,
        agent,
        agents,
    })
}

/// The resume-token adapter `spec` reads a resume token with; `None` for an agent resumed by its
/// worktree (`--continue`), which has no token to read.
pub(crate) fn resume_token_adapter(
    spec: &CodingAgentSpec,
) -> Result<Option<Arc<dyn ResumeTokenAdapter>>, Status> {
    let Some(slot) = &spec.agent.resume else {
        return Ok(None);
    };
    LaunchAdapters::builtin()
        .resume_token(&slot.adapter)
        .map(Some)
        .ok_or_else(|| {
            Status::internal(format!(
                "{} names the resume adapter {}, which this build does not have",
                spec.id, slot.adapter
            ))
        })
}

/// The resume-token adapter of a recorded `session_type` session's spec, read from the tools
/// `tddy_data_dir` declares: a tmux re-attach (`#tmux-agent`) after a daemon restart has no
/// coding agents at hand, only the session's record.
// TODO(agent-tools-cascade): confirm the re-attach should read the spec from `<tddyhome>/tools`
// rather than from the daemon's installed `HostCodingAgents` (the launch's source).
pub(crate) fn recorded_resume_token_adapter(
    tddy_data_dir: &Path,
    session_type: &str,
) -> Result<Option<Arc<dyn ResumeTokenAdapter>>, Status> {
    let spec_id = cli_session_spec_id(session_type)?;
    let registry = load_agent_registry(&tddy_data_dir.join("tools"))
        .map_err(|e| Status::internal(format!("load the coding agent tools: {e}")))?;
    let def = registry
        .get(spec_id)
        .ok_or_else(|| Status::internal(format!("the registry holds no {spec_id} tool")))?;
    let spec = CodingAgentSpec::of(def).map_err(|e| Status::internal(e.to_string()))?;
    resume_token_adapter(&spec)
}

impl LaunchSessions {
    /// Refuses a sandboxed start of `session_type` when the spec of its agent places that agent off
    /// the jail. The spec is read from the tools this daemon's data directory declares, so the
    /// refusal does not depend on which coding agents the host has installed.
    pub(crate) fn require_sandbox_placement(&self, session_type: &str) -> Result<(), Status> {
        let spec_id = cli_session_spec_id(session_type)?;
        let registry = load_agent_registry(&self.tddy_data_dir.join("tools"))
            .map_err(|e| Status::internal(format!("load the coding agent tools: {e}")))?;
        let def = registry
            .get(spec_id)
            .ok_or_else(|| Status::internal(format!("the registry holds no {spec_id} tool")))?;
        let spec = CodingAgentSpec::of(def).map_err(|e| Status::internal(e.to_string()))?;
        require_spec_placement(&spec, session_type, Placement::Jail)
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;
    use tddy_actions::tool::RunSection;
    use tddy_coding_agent_specs::{CodingAgentSection, WARP_SPEC_YAML};
    use tddy_rpc::Code;

    use super::*;

    #[test]
    fn a_start_binds_the_model_and_prompt_it_was_given() {
        // When
        let params = harness_template_params(&HarnessLaunch {
            model: "anthropic/claude-sonnet-4-5",
            prompt: Some("  fix the build  "),
            ..HarnessLaunch::default()
        });

        // Then
        assert_eq!(
            params,
            TemplateParams::new()
                .with("model", "anthropic/claude-sonnet-4-5")
                .with("prompt", "fix the build")
        );
    }

    #[test]
    fn a_start_with_an_empty_model_and_a_blank_prompt_binds_nothing() {
        // When
        let params = harness_template_params(&HarnessLaunch {
            model: "",
            prompt: Some("   "),
            ..HarnessLaunch::default()
        });

        // Then
        assert_eq!(params, TemplateParams::new());
    }

    #[test]
    fn a_resume_sets_resume_and_binds_neither_model_nor_prompt() {
        // When
        let params = harness_template_params(&HarnessLaunch {
            model: "muse-spark-1.3",
            prompt: Some("go on"),
            resume: true,
            resume_token: None,
        });

        // Then
        assert_eq!(params, TemplateParams::new().with_flag("resume", true));
    }

    #[test]
    fn a_resume_with_a_recorded_token_binds_it_and_a_blank_one_is_left_unbound() {
        // When
        let recorded = harness_template_params(&HarnessLaunch {
            resume: true,
            resume_token: Some(" 01JB7Q2N5W "),
            ..HarnessLaunch::default()
        });
        let blank = harness_template_params(&HarnessLaunch {
            resume: true,
            resume_token: Some("  "),
            ..HarnessLaunch::default()
        });

        // Then
        assert_eq!(
            (recorded, blank),
            (
                TemplateParams::new()
                    .with_flag("resume", true)
                    .with("resume_token", "01JB7Q2N5W"),
                TemplateParams::new().with_flag("resume", true),
            )
        );
    }

    /// `warp.yaml`, read straight from its YAML.
    fn the_builtin_warp_spec() -> CodingAgentSpec {
        let document: serde_yaml::Value = serde_yaml::from_str(WARP_SPEC_YAML).unwrap();
        let run: RunSection = serde_yaml::from_value(document["run"].clone()).unwrap();
        let agent: CodingAgentSection =
            serde_yaml::from_value(document["extensions"]["coding_agent"].clone()).unwrap();
        CodingAgentSpec {
            id: WARP_AGENT_ID.to_string(),
            run,
            agent,
        }
    }

    #[test]
    fn a_jailed_warp_is_refused_failed_precondition_naming_its_placements() {
        // When
        let refused = require_spec_placement(&the_builtin_warp_spec(), "warp-cli", Placement::Jail)
            .map_err(|status| (status.code, status.message));

        // Then
        assert_eq!(
            refused,
            Err((
                Code::FailedPrecondition,
                "the sandbox runner does not host warp-cli: warp cannot run in the jail: its \
                 spec's placements are [host]"
                    .to_string()
            ))
        );
    }

    #[test]
    fn a_host_warp_is_admitted() {
        // When
        let admitted =
            require_spec_placement(&the_builtin_warp_spec(), "warp-cli", Placement::Host)
                .map_err(|status| status.message);

        // Then
        assert_eq!(admitted, Ok(()));
    }
}
