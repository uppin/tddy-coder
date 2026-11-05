//! What a CLI agent launch binds when it renders its tool definition
//! (`tddy-coding-agent-specs/specs/*.yaml`) — on the host through
//! [`crate::spec_launch::launch_host_pty`], in a jail through
//! [`crate::launch_ports::LaunchSessions::launch_jailed`] — and the spec-driven refusal of a
//! placement the spec excludes.
//!
//! Feature docs: `docs/ft/daemon/{claude,cursor,opencode,muse,warp}-cli-session.md`.
//!
//! Nothing here builds an argv: these are the template parameters the specs name. A resume is
//! `resume` for the agents that continue their worktree's session (`--continue`), `resume_token`
//! for Warp, whose token the `warp-resume-token` adapter read off the last run, and the
//! conversation id (`session_id`, `chat_id`) for an agent whose `coding_agent.conversation` names
//! one.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use tddy_actions::tool::TemplateParams;
use tddy_coding_agent_specs::{
    routed_agent, CodingAgentRegistry, CodingAgentSpec, Placement, CLI_SESSION_TYPE_SUFFIX,
};
use tddy_core::adapters::{LaunchAdapters, ResumeTokenAdapter};
use tddy_core::provider_route::RoutedAgent;
use tddy_daemon_kernel::config::DaemonConfig;
use tddy_rpc::Status;

use crate::cursor_from_spec::NodeLauncher;
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
    /// The daemon's session id, for an agent whose `coding_agent.conversation` is
    /// `daemon_session_id` (`#agent-tools 14/19`): bound as `session_id` (`--session-id`, and
    /// `--resume` with it on a resume).
    pub session_id: Option<&'a str>,
    /// The minted conversation id, for an agent whose conversation is `minted`: bound as `chat_id`.
    pub chat_id: Option<&'a str>,
    /// The permission mode the start chose (`StartSessionRequest.permission_mode`); `None` or blank
    /// leaves the spec's own default (`auto` in claude.yaml).
    pub permission_mode: Option<&'a str>,
    /// `StartSessionRequest.dangerously_skip_permissions`: bound as the
    /// `dangerously_skip_permissions` flag when set.
    pub dangerously_skip_permissions: bool,
    /// The bundled runtime behind a launcher script (`cursor_from_spec::node_launcher_of`): bound as
    /// `node_launcher` and `node_entry`, which a spec's argv runs instead of `{binary}`.
    pub node_launcher: Option<&'a NodeLauncher>,
}

/// The parameters every CLI agent spec's `run.argv` is rendered with: `model` and `prompt` each
/// bound when non-empty (trimmed) on a start; `resume` set on a resume, with `resume_token` bound
/// when a non-blank one was recorded. A resume binds no prompt, and the model only for a
/// conversation by id (`session_id`, `chat_id`) — the agent continues the session it had. The
/// conversation id, the permission mode, `dangerously_skip_permissions` and a node launcher are
/// bound on both, each when given.
pub fn harness_template_params(launch: &HarnessLaunch<'_>) -> TemplateParams {
    let by_id = launch.session_id.is_some() || launch.chat_id.is_some();
    let mut params = TemplateParams::new();
    if let Some(launcher) = launch.node_launcher {
        params = params
            .with("node_launcher", &launcher.node.to_string_lossy())
            .with("node_entry", &launcher.entry.to_string_lossy());
    }
    let params = params
        .with_opt("session_id", non_blank(launch.session_id))
        .with_opt("chat_id", non_blank(launch.chat_id))
        .with_opt("permission_mode", non_blank(launch.permission_mode))
        .with_flag(
            "dangerously_skip_permissions",
            launch.dangerously_skip_permissions,
        );
    if launch.resume {
        // A resume by an id re-binds the recorded model (the bespoke resume passed `--model`); a
        // resume by the worktree or a token binds neither model nor prompt.
        let model = if by_id {
            non_blank(Some(launch.model))
        } else {
            None
        };
        return params
            .with_flag("resume", true)
            .with_opt("resume_token", non_blank(launch.resume_token))
            .with_opt("model", model);
    }
    params
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
/// spec that launches it and what its provider route is built from. It replaces the
/// `CliHarness` descriptor on the spec-driven paths; every field has a home in the spec or the route.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CliSessionType {
    /// The session type's name (`opencode-cli`), interned: the worktree and branch policy take a
    /// static string, and an agent dropped into `<tddyhome>/tools` has no static name of its own.
    pub name: &'static str,
    /// The id of the spec that launches the session type (`opencode`), interned.
    pub spec_id: &'static str,
    /// What the session's provider route is built from: its spec's id, product and route support
    /// (`routed_agent`). An agent whose spec declares no route is refused when a route is asked of
    /// it, naming its product.
    pub route: RoutedAgent,
}

/// `name` as a `'static` string, one allocation per distinct name for the life of the process: the
/// names are agent ids and session types, a handful per host.
fn interned(name: &str) -> &'static str {
    static NAMES: std::sync::Mutex<std::collections::BTreeSet<&'static str>> =
        std::sync::Mutex::new(std::collections::BTreeSet::new());
    let mut names = NAMES.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(existing) = names.get(name) {
        return existing;
    }
    let leaked: &'static str = Box::leak(name.to_string().into_boxed_str());
    names.insert(leaked);
    leaked
}

/// The agent id of the one CLI agent whose session has its own bespoke launch path (the hook,
/// tmux and recipe handling of Claude Code's TUI).
// TODO(cli-dispatch-collapse): #agent-tools n17 `cli-dispatch-collapse` — claude and cursor still
// launch through their own paths, selected here by the spec's id; unifying them with the generic
// launch removes this
// (docs/dev/todo/2026-11-04-claude-and-cursor-launch-by-agent-id.md).
const CLAUDE_AGENT_ID: &str = "claude";
/// The agent id of the CLI agent with the Cursor launch path; see [`CLAUDE_AGENT_ID`].
const CURSOR_AGENT_ID: &str = "cursor";

/// Which launch path a CLI session type takes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CliLauncher {
    /// The Claude Code TUI's path.
    Claude,
    /// The Cursor agent's path.
    Cursor,
    /// The spec-driven launch every other CLI agent shares.
    Generic,
}

/// The launch path of `session_type`, from the coding agents this host has: `None` for a session
/// type that is no agent's (a tool or workspace session), a refusal for an agent session type no
/// installed spec launches, or when the host has no coding agents at all.
pub(crate) fn cli_launcher(
    agents: Option<&HostCodingAgents>,
    session_type: &str,
) -> Result<Option<CliLauncher>, Status> {
    if !session_type.ends_with(CLI_SESSION_TYPE_SUFFIX) {
        return Ok(None);
    }
    let agents = agents.ok_or_else(|| {
        Status::failed_precondition(format!(
            "this host has no coding agents installed: a {session_type} session is launched from \
             its tool definition"
        ))
    })?;
    let spec = agents
        .agents()
        .for_session_type(session_type)
        .ok_or_else(|| {
            Status::failed_precondition(format!("no coding agent spec launches {session_type}"))
        })?;
    Ok(Some(launcher_of(&spec.id)))
}

/// [`cli_launcher`] for a recorded session whose host has no [`HostCodingAgents`] at hand — a tmux
/// re-attach after a daemon restart — read from the tools `tddy_data_dir` declares. `None` for a
/// session type no installed spec launches.
pub(crate) fn recorded_cli_launcher(
    tddy_data_dir: &Path,
    session_type: &str,
) -> Result<Option<CliLauncher>, Status> {
    let registry = CodingAgentRegistry::load(&tddy_data_dir.join("tools"))
        .map_err(|e| Status::internal(format!("load the coding agent tools: {e}")))?;
    Ok(registry
        .for_session_type(session_type)
        .map(|spec| launcher_of(&spec.id)))
}

/// The launch path of the agent `agent_id`.
fn launcher_of(agent_id: &str) -> CliLauncher {
    match agent_id {
        CLAUDE_AGENT_ID => CliLauncher::Claude,
        CURSOR_AGENT_ID => CliLauncher::Cursor,
        _ => CliLauncher::Generic,
    }
}

/// The CLI session type `session_type` names, refused when no installed spec launches it: there is
/// no other launch for a session type, so a missing spec is an error, not a fall back to a
/// hand-built one.
pub(crate) fn cli_session_type(
    agents: &HostCodingAgents,
    session_type: &str,
) -> Result<CliSessionType, Status> {
    let spec = cli_session_spec(agents.agents(), session_type)?;
    Ok(CliSessionType {
        name: interned(session_type),
        spec_id: interned(&spec.id),
        route: routed_agent(spec),
    })
}

/// The coding agent spec a CLI session type launches from.
fn cli_session_spec<'a>(
    registry: &'a CodingAgentRegistry,
    session_type: &str,
) -> Result<&'a CodingAgentSpec, Status> {
    registry.for_session_type(session_type).ok_or_else(|| {
        Status::failed_precondition(format!("no coding agent spec launches {session_type}"))
    })
}

/// The binary `spec_id` runs for a `session_type` session: `cli_harnesses.<session type>.binary_path`
/// after the binary's own environment variable, as the daemon has always read them, when that names a
/// path. `None` leaves the binary to the spec's own resolution on the host.
// TODO(agent-config-removal): #agent-tools n19 `agent-config-removal` — `cli_harnesses` is refused at load; only the
// binary's environment variable is left to read, and the DaemonConfig section goes with the rest of
// the removal
// (docs/dev/todo/2026-11-04-removed-agent-config-readers-remain.md).
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
    let registry = CodingAgentRegistry::load(&tddy_data_dir.join("tools"))
        .map_err(|e| Status::internal(format!("load the coding agent tools: {e}")))?;
    resume_token_adapter(cli_session_spec(&registry, session_type)?)
}

impl LaunchSessions {
    /// Refuses a sandboxed start of `session_type` when the spec of its agent places that agent off
    /// the jail. The spec is read from the tools this daemon's data directory declares, so the
    /// refusal does not depend on which coding agents the host has installed.
    pub(crate) fn require_sandbox_placement(&self, session_type: &str) -> Result<(), Status> {
        let registry = CodingAgentRegistry::load(&self.tddy_data_dir.join("tools"))
            .map_err(|e| Status::internal(format!("load the coding agent tools: {e}")))?;
        require_spec_placement(
            cli_session_spec(&registry, session_type)?,
            session_type,
            Placement::Jail,
        )
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
    fn a_cursor_start_binds_the_chat_model_and_prompt_it_was_given() {
        // When
        let params = harness_template_params(&HarnessLaunch {
            model: "composer-2.5",
            prompt: Some("  fix it  "),
            chat_id: Some("chat-1"),
            ..HarnessLaunch::default()
        });

        // Then
        assert_eq!(
            params,
            TemplateParams::new()
                .with("chat_id", "chat-1")
                .with("model", "composer-2.5")
                .with("prompt", "fix it")
        );
    }

    #[test]
    fn a_node_launcher_binds_its_node_and_entry() {
        // Given
        let launcher = NodeLauncher {
            node: PathBuf::from("/opt/cursor-agent/versions/2026.10.1/node"),
            entry: PathBuf::from("/opt/cursor-agent/versions/2026.10.1/index.js"),
        };

        // When
        let params = harness_template_params(&HarnessLaunch {
            chat_id: Some("chat-1"),
            node_launcher: Some(&launcher),
            ..HarnessLaunch::default()
        });

        // Then
        assert_eq!(
            params,
            TemplateParams::new()
                .with("chat_id", "chat-1")
                .with("node_launcher", "/opt/cursor-agent/versions/2026.10.1/node")
                .with(
                    "node_entry",
                    "/opt/cursor-agent/versions/2026.10.1/index.js"
                )
        );
    }

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
            ..HarnessLaunch::default()
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

    #[test]
    fn a_start_binds_the_session_id_chat_id_permission_mode_and_skip_flag_it_was_given() {
        // When
        let params = harness_template_params(&HarnessLaunch {
            model: "opus",
            session_id: Some("0190-claude"),
            chat_id: Some("chat-0042"),
            permission_mode: Some(" plan "),
            dangerously_skip_permissions: true,
            ..HarnessLaunch::default()
        });

        // Then
        assert_eq!(
            params,
            TemplateParams::new()
                .with("model", "opus")
                .with("session_id", "0190-claude")
                .with("chat_id", "chat-0042")
                .with("permission_mode", "plan")
                .with_flag("dangerously_skip_permissions", true)
        );
    }

    #[test]
    fn a_resume_by_an_id_binds_the_recorded_model_and_the_id_and_a_worktree_resume_binds_neither() {
        // When
        let by_session_id = harness_template_params(&HarnessLaunch {
            model: "opus",
            resume: true,
            session_id: Some("0190-claude"),
            ..HarnessLaunch::default()
        });
        let by_chat = harness_template_params(&HarnessLaunch {
            model: "composer-2.5",
            resume: true,
            chat_id: Some("chat-0042"),
            ..HarnessLaunch::default()
        });
        let by_worktree = harness_template_params(&HarnessLaunch {
            model: "anthropic/claude-sonnet-4-5",
            resume: true,
            ..HarnessLaunch::default()
        });

        // Then
        assert_eq!(
            (by_session_id, by_chat, by_worktree),
            (
                TemplateParams::new()
                    .with_flag("resume", true)
                    .with("model", "opus")
                    .with("session_id", "0190-claude"),
                TemplateParams::new()
                    .with_flag("resume", true)
                    .with("model", "composer-2.5")
                    .with("chat_id", "chat-0042"),
                TemplateParams::new().with_flag("resume", true),
            )
        );
    }

    /// The built-in `id` resolved to `/usr/local/bin/<command>`.
    fn a_host_agent(id: &str) -> HostAgent {
        let spec = tddy_coding_agent_specs::embedded_spec(id).expect("the spec is built in");
        HostAgent {
            tool: tddy_actions::tool::ResolvedTool {
                id: id.to_string(),
                command: id.to_string(),
                binary: PathBuf::from(format!("/usr/local/bin/{id}")),
                reads: Vec::new(),
                network: Default::default(),
            },
            spec,
            deps: Vec::new(),
        }
    }

    /// `agent`'s rendered argv for `launch`, with no slots.
    fn argv_of(agent: &HostAgent, launch: &HarnessLaunch<'_>) -> Vec<String> {
        crate::spec_launch::plan_host_pty(
            agent,
            &harness_template_params(launch),
            &tddy_coding_agent_specs::slots::RenderedSlots::default(),
        )
        .expect("the spec renders")
        .argv
    }

    #[test]
    fn the_claude_spec_renders_through_the_generic_parameters_the_argv_the_bespoke_launch_built() {
        // Given
        let claude = a_host_agent("claude");

        // When — a start with no permission mode chosen, and its resume
        let start = argv_of(
            &claude,
            &HarnessLaunch {
                model: "opus",
                prompt: Some("fix the build"),
                session_id: Some("0190-claude"),
                ..HarnessLaunch::default()
            },
        );
        let resume = argv_of(
            &claude,
            &HarnessLaunch {
                model: "opus",
                resume: true,
                session_id: Some("0190-claude"),
                ..HarnessLaunch::default()
            },
        );

        // Then — byte-identical to `claude_template_params`' render: the default permission mode
        // is `auto`
        assert_eq!(
            (start, resume),
            (
                [
                    "/usr/local/bin/claude",
                    "--model",
                    "opus",
                    "--session-id",
                    "0190-claude",
                    "--permission-mode",
                    "auto",
                    "fix the build",
                ]
                .map(str::to_string)
                .to_vec(),
                [
                    "/usr/local/bin/claude",
                    "--model",
                    "opus",
                    "--resume",
                    "0190-claude",
                    "--permission-mode",
                    "auto",
                ]
                .map(str::to_string)
                .to_vec(),
            )
        );
    }

    #[test]
    fn the_cursor_spec_renders_through_the_generic_parameters_the_argv_the_bespoke_launch_built() {
        // When
        let argv = argv_of(
            &a_host_agent("cursor"),
            &HarnessLaunch {
                model: "composer-2.5",
                prompt: Some("fix the build"),
                chat_id: Some("chat-0042"),
                ..HarnessLaunch::default()
            },
        );

        // Then
        assert_eq!(
            argv,
            [
                "/usr/local/bin/cursor",
                "--resume",
                "chat-0042",
                "--model",
                "composer-2.5",
                "fix the build",
            ]
            .map(str::to_string)
            .to_vec()
        );
    }

    #[test]
    fn opencode_muse_and_warp_render_the_same_argv_whatever_conversation_parameters_are_bound() {
        // Given — a start and a resume, each with and without the claude/cursor parameters
        let plain_start = HarnessLaunch {
            model: "a-model",
            prompt: Some("go"),
            ..HarnessLaunch::default()
        };
        let plain_resume = HarnessLaunch {
            resume: true,
            resume_token: Some("tok-123"),
            ..HarnessLaunch::default()
        };
        let with_ids = |launch: HarnessLaunch<'static>| HarnessLaunch {
            session_id: Some("0190-agent"),
            chat_id: Some("chat-0042"),
            permission_mode: Some("plan"),
            dangerously_skip_permissions: true,
            ..launch
        };

        // When
        let renders: Vec<(bool, bool)> = ["opencode", "muse", "warp"]
            .into_iter()
            .map(|id| {
                let agent = a_host_agent(id);
                (
                    argv_of(&agent, &plain_start) == argv_of(&agent, &with_ids(plain_start)),
                    argv_of(&agent, &plain_resume) == argv_of(&agent, &with_ids(plain_resume)),
                )
            })
            .collect();

        // Then
        assert_eq!(renders, vec![(true, true); 3]);
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
