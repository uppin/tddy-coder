// Served by the agent's own `tddy-tools --mcp` child, the same one that carries the exec tools to
// the codebase daemon.
use tddy_coding_agent_specs::claude_tools::{
    build_claude_allowlist, build_claude_disallowlist, PERMISSION_PROMPT_TOOL,
};
use tddy_coding_agent_specs::slots::{self, McpServerCommand, SlotInputs};
use tddy_coding_agent_specs::{CodingAgentRegistry, CodingAgentSpec, McpSlot, HOST_AGENT_VARIANT};

use std::collections::BTreeMap;

use tddy_rpc::Status;

use std::path::PathBuf;

use std::path::Path;

/// The `(agent, withdrawn tools)` pairs a session's roster imposes, from the roster as the daemon
/// holding it serves it over the wire.
///
/// A split session's roster lives on the daemon holding its codebase, so the host running the agent
/// only ever sees it as wire entries — never as the `.session.yaml` records
/// [`crate::connection_service::roster_replacement_pairs`] reads on the co-located paths. Same rule
/// and the same normalizer as those: each entry's own snapshot of `replaces`, spelled as the exec
/// catalog spells it, or the allowlist this feeds would filter on a name that is not in it and drop
/// nothing.
pub fn wire_roster_withdrawals(
    agents: &[tddy_service::proto::session_agents_svc::SessionAgentEntry],
) -> Vec<(String, Vec<String>)> {
    agents
        .iter()
        .map(|agent| {
            (
                agent.name.clone(),
                tddy_discovery::subagent::normalize_replaced_tools(&agent.replaces),
            )
        })
        .collect()
}

/// The env pair that hands a host-run agent's `tddy-tools` the roster its spawn already read, so
/// that roster is in force before the MCP server answers its first `tools/list` — or nothing, for a
/// roster with no agent in it.
///
/// A roster **snapshot** rather than `TDDY_SUBAGENTS_JSON`'s defs: a seeded def is what `tddy-tools`
/// runs an agent's turn loop from in-process, and a host-run agent's conversations go through the
/// daemon — its host may not reach the model at all, and a def would export the provider credential
/// into the agent's environment. The entries are what it lacks: who is attached, what each took over.
pub fn roster_seed_env_pairs(
    roster: &tddy_service::proto::session_agents_svc::SessionAgentRoster,
) -> Vec<(String, String)> {
    if roster.agents.is_empty() {
        return Vec::new();
    }
    vec![tddy_discovery::roster::roster_seed_env(roster)]
}

/// Every tool the roster withdraws from the main agent, once each: the union across its agents,
/// which is the rule (PRD § Tool replacement, AC19).
fn withdrawn_tools(withdrawals: &[(String, Vec<String>)]) -> Vec<String> {
    tddy_discovery::subagent::normalize_replaced_tools(
        &withdrawals
            .iter()
            .flat_map(|(_, tools)| tools.clone())
            .collect::<Vec<String>>(),
    )
}

/// The withdrawn tool names as borrowed slices, one `Vec` per agent — the storage
/// [`subagent_replacements`] borrows from, kept separate because
/// [`tddy_sandbox::SubagentReplacement`] holds `&[&str]`.
pub(crate) fn borrowed_withdrawals(withdrawals: &[(String, Vec<String>)]) -> Vec<Vec<&str>> {
    withdrawals
        .iter()
        .map(|(_, tools)| tools.iter().map(String::as_str).collect())
        .collect()
}

/// The per-agent breakdown the appendix renders, over storage from [`borrowed_withdrawals`].
pub(crate) fn subagent_replacements<'a>(
    withdrawals: &'a [(String, Vec<String>)],
    borrowed: &'a [Vec<&'a str>],
) -> Vec<tddy_sandbox::SubagentReplacement<'a>> {
    withdrawals
        .iter()
        .zip(borrowed.iter())
        .map(|((name, _), replaced)| tddy_sandbox::SubagentReplacement { name, replaced })
        .collect()
}

/// Filename of the MCP server's log, under the session directory.
///
/// Same basename the sandbox runner writes into its egress dir (`tddy-sandbox-runner`): a split
/// session's session dir is its equivalent — the per-session place the host can read afterwards.
const MCP_LOG_BASENAME: &str = "tddy-tools.mcp.log";

/// `RUST_LOG` for the agent's `tddy-tools --mcp` child.
///
/// Mirrors the sandbox runner's default, minus its `tddy_discovery=debug` — that one exists for
/// specialized subagents' HTTP activity, which a split session has none of. `tddy_tools=debug` is
/// the part that matters here: it is where a failed LiveKit dispatch to the codebase daemon (room
/// connect refused, peer absent, truncated stream) is reported.
const MCP_RUST_LOG: &str = "info,tddy_tools=debug";

/// Where a split session's MCP server writes its log.
pub fn split_mcp_log_path(session_dir: &Path) -> PathBuf {
    session_dir.join(MCP_LOG_BASENAME)
}

/// Build the `claude` flags that leave the agent no route to this host's filesystem and point its
/// MCP server at `tddy-tools`.
///
/// The MCP config is written under `session_dir` rather than the context directory so the agent's
/// cwd holds only guidance.
///
/// `withdrawals` is what this session's roster takes away from the main agent — `(agent, tools)`
/// pairs, from [`wire_roster_withdrawals`] over the roster its codebase daemon serves. It is the
/// first of the two layers a withdrawal is enforced at (PRD § Enforced at two layers): the second
/// is `tddy-tools`, which stops advertising a withdrawn tool and refuses a call to one. Both are
/// needed. Without this layer the withdrawn tool stays *pre-approved*, so the main agent is invited
/// to reach for it and meets the second layer's refusal mid-turn, every turn.
///
/// `spec` is the split agent as this host's registry holds it ([`split_agent_spec`]), operator
/// overrides included. `tool_server_env` is what the agent's `tddy-tools --mcp` needs to reach its
/// session — the `TDDY_REMOTE_*` transport and the roster seed — registered on that server alone:
/// the agent process itself is given no `TDDY_*` variable.
pub fn split_claude_extra_args(
    spec: &CodingAgentSpec,
    session_dir: &Path,
    tddy_tools_path: &str,
    tool_server_env: &[(String, String)],
    withdrawals: &[(String, Vec<String>)],
) -> Result<Vec<String>, Status> {
    // The variant carries the flags that keep the checkout's own configuration off this host
    // (`--strict-mcp-config`, `--setting-sources user`) and withdraws every native filesystem and
    // shell tool, so the slots below add only what the session's roster changes.
    let mut args = spec
        .variant_args(HOST_AGENT_VARIANT)
        .map_err(|e| Status::internal(e.to_string()))?;
    let natives = spec
        .agent
        .variants
        .get(HOST_AGENT_VARIANT)
        .map(|variant| variant.withdraw_native.as_slice())
        .unwrap_or_default();

    let withdrawn = withdrawn_tools(withdrawals);
    let withdrawn_refs: Vec<&str> = withdrawn.iter().map(String::as_str).collect();
    // Every exec tool the roster leaves alone is pre-approved: they are all reachable, they simply
    // execute on the codebase daemon. The subagent tools are pre-approved whether or not this
    // session has agents *yet* — unlike the jail, which reads a seed fixed at spawn, a split
    // session's roster is live and an operator may attach an agent at minute forty, while Claude's
    // own lists are fixed for the life of the process they were passed to. Nothing is granted by
    // pre-approving them on a session with no agents: `tddy-tools` advertises them only while the
    // roster has someone to address, so the flag names a tool the model is never offered.
    let subagent_tools_are_pre_approved = true;
    // Dropping a tool from `--allowedTools` only un-pre-approves it. A split session's *native*
    // routes are already hard-disabled by the variant, but a withdrawn tool's proxied
    // `mcp__tddy-tools__` form is the route this agent actually had, and it stays callable through
    // the permission prompt until `--disallowedTools` names it (`PermissionServer::decide` allows
    // every `mcp__tddy-tools__*` call it is asked about).
    let denied_tools: Vec<String> = build_claude_disallowlist(&withdrawn_refs)
        .into_iter()
        .filter(|tool| !natives.contains(tool))
        .collect();
    let mut rendered = slots::render(
        &without_strict_mcp_flag(spec),
        &SlotInputs {
            workdir: session_dir.to_path_buf(),
            scratch_dir: session_dir.to_path_buf(),
            mcp_server: Some(tddy_tools_server(
                session_dir,
                Path::new(tddy_tools_path),
                tool_server_env,
            )),
            allowed_tools: build_claude_allowlist(subagent_tools_are_pre_approved, &withdrawn_refs),
            denied_tools,
            permission_prompt_tool: Some(PERMISSION_PROMPT_TOOL.to_string()),
            preamble: None,
            append_system_prompt_file: None,
            hooks: None,
            home: None,
        },
    )
    .map_err(|e| Status::internal(e.to_string()))?;
    write_registrations(&mut rendered.files)?;
    args.extend(rendered.argv_extra);
    Ok(args)
}

/// The coding agent a split session runs (split placement is `claude-cli` only).
pub const SPLIT_AGENT_ID: &str = "claude";

/// The agent a split session runs on this host: [`SPLIT_AGENT_ID`]'s spec as `registry` — the
/// host's own coding-agent registry — holds it, operator overrides included. Refused when the host
/// has no such agent.
pub fn split_agent_spec(registry: &CodingAgentRegistry) -> Result<CodingAgentSpec, Status> {
    registry.get(SPLIT_AGENT_ID).cloned().ok_or_else(|| {
        Status::failed_precondition(format!(
            "this host has no `{SPLIT_AGENT_ID}` coding agent, which a split session runs"
        ))
    })
}

/// `spec` without the MCP slot's strict flag, for a launch whose variant already passes it.
fn without_strict_mcp_flag(spec: &CodingAgentSpec) -> CodingAgentSpec {
    let mut spec = spec.clone();
    if let McpSlot::File { strict_flag, .. } = &mut spec.agent.mcp {
        *strict_flag = None;
    }
    spec
}

/// `tddy-tools --mcp` as `session_dir`'s session registers it, with `tool_server_env` — the
/// session's transport and roster seed — beside its log settings.
fn tddy_tools_server(
    session_dir: &Path,
    tddy_tools_path: &Path,
    tool_server_env: &[(String, String)],
) -> McpServerCommand {
    McpServerCommand {
        name: "tddy-tools".to_string(),
        command: tddy_tools_path.to_path_buf(),
        args: vec!["--mcp".to_string()],
        env: tddy_tools_mcp_env(session_dir)
            .into_iter()
            .chain(tool_server_env.iter().cloned())
            .collect(),
        cwd: None,
    }
}

/// Write each of `files` — the registration the slots rendered — at its path.
fn write_registrations(files: &mut Vec<(PathBuf, Vec<u8>)>) -> Result<(), Status> {
    for (path, contents) in files.drain(..) {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)
                .map_err(|e| Status::internal(format!("failed to write MCP config: {e}")))?;
        }
        tddy_core::atomic_file::write_atomic(&path, contents)
            .map_err(|e| Status::internal(format!("failed to write MCP config: {e}")))?;
    }
    Ok(())
}

/// The `env` block of the `tddy-tools --mcp` server a session under `session_dir` registers: its log
/// file and its log filter.
pub fn tddy_tools_mcp_env(session_dir: &Path) -> BTreeMap<String, String> {
    // Every way a tool call can fail is reported by `tddy-tools` itself, and Claude Code captures
    // an MCP server's stderr, so without a log file those reports exist only inside a process
    // nobody can attach to: a session whose dispatch is failing would leave no evidence on any
    // daemon. The sandbox path solves this the same way, pointing the same variable at its egress
    // dir.
    BTreeMap::from([
        (
            "TDDY_TOOLS_LOG_FILE".to_string(),
            split_mcp_log_path(session_dir)
                .to_string_lossy()
                .into_owned(),
        ),
        ("RUST_LOG".to_string(), MCP_RUST_LOG.to_string()),
    ])
}

#[cfg(test)]
mod host_spec_tests {
    //! The split agent is the host's: its spec comes from this host's coding-agent registry,
    //! operator overrides included, and its `tddy-tools` server is the one place the session's
    //! `TDDY_*` transport is handed over (`#agent-tools 25/28`).

    use std::collections::BTreeMap;
    use std::path::{Path, PathBuf};

    use pretty_assertions::assert_eq;
    use tddy_coding_agent_specs::CodingAgentRegistry;
    use tddy_rpc::Code;

    use super::{split_agent_spec, split_claude_extra_args, SPLIT_AGENT_ID};

    const AN_OPERATOR_FLAG: &str = "--an-operator-flag";
    const TDDY_TOOLS: &str = "/usr/bin/tddy-tools";

    /// A host whose `<tddyhome>/tools` holds one override of the split agent's spec: its
    /// `host_agent` variant passes one flag more than the built-in one.
    struct AHostWithAnOverriddenSplitAgent {
        _home: tempfile::TempDir,
        registry: CodingAgentRegistry,
    }

    impl AHostWithAnOverriddenSplitAgent {
        fn adding_to_the_host_agent_variant(flag: &str) -> Self {
            let builtin_args = "args: [--strict-mcp-config, --setting-sources, user]";
            let overridden = the_builtin_split_agent_yaml().replace(
                builtin_args,
                &format!("args: [--strict-mcp-config, --setting-sources, user, {flag}]"),
            );
            assert_ne!(
                overridden,
                the_builtin_split_agent_yaml(),
                "the override must change the host_agent variant's args"
            );
            let home = tempfile::tempdir().expect("a tddy home");
            let tools = home.path().join("tools");
            std::fs::create_dir_all(&tools).expect("the tools dir");
            std::fs::write(tools.join(format!("{SPLIT_AGENT_ID}.yaml")), overridden)
                .expect("the override is written");
            let registry = CodingAgentRegistry::load(&tools).expect("the override loads");
            Self {
                _home: home,
                registry,
            }
        }
    }

    fn the_builtin_split_agent_yaml() -> &'static str {
        let file_name = format!("/{SPLIT_AGENT_ID}.yaml");
        tddy_coding_agent_specs::builtin_specs()
            .into_iter()
            .find(|(path, _)| path.ends_with(&file_name))
            .map(|(_, yaml)| yaml)
            .expect("the split agent's spec is built in")
    }

    fn a_registry_of_the_builtin_agents() -> CodingAgentRegistry {
        CodingAgentRegistry::builtin().expect("the built-in coding agents load")
    }

    /// The flags a split session spawns with on `registry`'s host, its tool server handed
    /// `tool_server_env`.
    fn split_args_on(
        registry: &CodingAgentRegistry,
        session_dir: &Path,
        tool_server_env: &[(String, String)],
    ) -> Vec<String> {
        let spec = split_agent_spec(registry).expect("the host holds the split agent");
        split_claude_extra_args(&spec, session_dir, TDDY_TOOLS, tool_server_env, &[])
            .expect("the split flags render")
    }

    /// The `env` block the flags' MCP config registers `tddy-tools` with.
    fn tool_server_env_registered_by(args: &[String]) -> BTreeMap<String, String> {
        let config_path: PathBuf = args
            .windows(2)
            .find(|pair| pair[0] == "--mcp-config")
            .map(|pair| PathBuf::from(&pair[1]))
            .expect("the flags name an --mcp-config");
        let config: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(&config_path).expect("the MCP config is written"),
        )
        .expect("the MCP config is JSON");
        serde_json::from_value(config["mcpServers"]["tddy-tools"]["env"].clone())
            .expect("tddy-tools is registered with a string env")
    }

    fn a_split_transport() -> Vec<(String, String)> {
        vec![
            (
                "TDDY_REMOTE_SESSION_ID".to_string(),
                "the-codebase-session".to_string(),
            ),
            (
                "TDDY_REMOTE_LIVEKIT_TOKEN".to_string(),
                "a-scoped-join-jwt".to_string(),
            ),
        ]
    }

    #[test]
    fn a_split_session_renders_the_hosts_overridden_agent_spec() {
        // Given a host whose operator overrode the split agent's host_agent variant
        let host =
            AHostWithAnOverriddenSplitAgent::adding_to_the_host_agent_variant(AN_OPERATOR_FLAG);
        let session_dir = tempfile::tempdir().expect("a session dir");

        // When a split session's flags are rendered on that host
        let args = split_args_on(&host.registry, session_dir.path(), &[]);

        // Then the agent is launched as the operator configured it, not as the binary embeds it
        assert!(
            args.iter().any(|arg| arg == AN_OPERATOR_FLAG),
            "the split agent must carry the host's override {AN_OPERATOR_FLAG}; its flags were {args:?}"
        );
    }

    #[test]
    fn a_host_without_the_split_agent_refuses_a_split_session() {
        // Given a host whose registry holds no coding agent at all
        let registry = CodingAgentRegistry::empty();

        // When the split agent's spec is asked for
        let refused = split_agent_spec(&registry).expect_err("no split agent on this host");

        // Then it is refused as a precondition, naming the agent it looked for
        assert_eq!(
            (refused.code, refused.message.contains(SPLIT_AGENT_ID)),
            (Code::FailedPrecondition, true),
            "the refusal must be FAILED_PRECONDITION naming {SPLIT_AGENT_ID}: {refused:?}"
        );
    }

    #[test]
    fn a_split_agents_tool_server_is_registered_with_the_sessions_transport() {
        // Given the transport a split session's tools reach its codebase with
        let transport = a_split_transport();
        let session_dir = tempfile::tempdir().expect("a session dir");

        // When the split flags are rendered with it
        let args = split_args_on(
            &a_registry_of_the_builtin_agents(),
            session_dir.path(),
            &transport,
        );

        // Then the tddy-tools server carries every variable of it: the agent process carries none
        let registered = tool_server_env_registered_by(&args);
        assert_eq!(
            transport
                .iter()
                .map(|(name, _)| (name.clone(), registered.get(name).cloned()))
                .collect::<Vec<_>>(),
            transport
                .iter()
                .map(|(name, value)| (name.clone(), Some(value.clone())))
                .collect::<Vec<_>>(),
        );
    }

    #[test]
    fn a_split_agents_tool_server_keeps_its_log_settings_beside_the_transport() {
        // Given the transport a split session's tools reach its codebase with
        let session_dir = tempfile::tempdir().expect("a session dir");

        // When the split flags are rendered with it
        let args = split_args_on(
            &a_registry_of_the_builtin_agents(),
            session_dir.path(),
            &a_split_transport(),
        );

        // Then the server still logs where a session's dispatch failures are read from
        let registered = tool_server_env_registered_by(&args);
        assert_eq!(
            (
                registered.get("TDDY_TOOLS_LOG_FILE").cloned(),
                registered.get("RUST_LOG").cloned()
            ),
            (
                Some(
                    super::split_mcp_log_path(session_dir.path())
                        .to_string_lossy()
                        .into_owned()
                ),
                Some(super::MCP_RUST_LOG.to_string())
            )
        );
    }

    #[test]
    fn a_host_with_no_overrides_renders_the_builtin_host_variant() {
        // Given a host with no operator override of the split agent
        let session_dir = tempfile::tempdir().expect("a session dir");

        // When a split session's flags are rendered there
        let args = split_args_on(&a_registry_of_the_builtin_agents(), session_dir.path(), &[]);

        // Then they open with the built-in host_agent variant, and carry no override
        assert_eq!(
            (
                args.iter().take(3).cloned().collect::<Vec<_>>(),
                args.iter().any(|arg| arg == AN_OPERATOR_FLAG)
            ),
            (
                vec![
                    "--strict-mcp-config".to_string(),
                    "--setting-sources".to_string(),
                    "user".to_string()
                ],
                false
            )
        );
    }
}
