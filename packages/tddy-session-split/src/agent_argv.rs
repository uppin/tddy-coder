// Served by the agent's own `tddy-tools --mcp` child, the same one that carries the exec tools to
// the codebase daemon.
use tddy_coding_agent_specs::claude_tools::{
    build_claude_allowlist, build_claude_disallowlist, PERMISSION_PROMPT_TOOL,
};
use tddy_coding_agent_specs::slots::{self, McpServerCommand, SlotInputs};
use tddy_coding_agent_specs::{embedded_spec, CodingAgentSpec, McpSlot, HOST_AGENT_VARIANT};

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
pub fn split_claude_extra_args(
    session_dir: &Path,
    tddy_tools_path: &str,
    withdrawals: &[(String, Vec<String>)],
) -> Result<Vec<String>, Status> {
    let spec = split_agent_spec()?;
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
        &without_strict_mcp_flag(&spec),
        &SlotInputs {
            workdir: session_dir.to_path_buf(),
            scratch_dir: session_dir.to_path_buf(),
            mcp_server: Some(tddy_tools_server(session_dir, Path::new(tddy_tools_path))),
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

/// The agent a split session runs on this host: `claude.yaml`, as embedded.
fn split_agent_spec() -> Result<CodingAgentSpec, Status> {
    embedded_spec("claude").map_err(|e| Status::internal(format!("the host agent's spec: {e}")))
}

/// `spec` without the MCP slot's strict flag, for a launch whose variant already passes it.
fn without_strict_mcp_flag(spec: &CodingAgentSpec) -> CodingAgentSpec {
    let mut spec = spec.clone();
    if let McpSlot::File { strict_flag, .. } = &mut spec.agent.mcp {
        *strict_flag = None;
    }
    spec
}

/// `tddy-tools --mcp` as `session_dir`'s session registers it.
fn tddy_tools_server(session_dir: &Path, tddy_tools_path: &Path) -> McpServerCommand {
    McpServerCommand {
        name: "tddy-tools".to_string(),
        command: tddy_tools_path.to_path_buf(),
        args: vec!["--mcp".to_string()],
        env: tddy_tools_mcp_env(session_dir),
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

/// Write `session_dir`'s `claude-mcp-config.json`, registering `tddy_tools_path --mcp` as the one
/// MCP server, and return the `claude` flags that load it and nothing else.
///
/// For an agent whose native filesystem and shell tools are withdrawn, `tddy-tools` is the only
/// reach it has, so the config is the only one loaded: `--mcp-config` alone *adds* to the
/// user-scoped MCP configuration, and any filesystem or shell MCP server the operator has
/// configured would load beside `tddy-tools` — reachable under an `mcp__*` name no disallowlist
/// covers. The restriction has to be impossible to route around, not merely the default.
pub fn tddy_tools_only_mcp_args(
    session_dir: &Path,
    tddy_tools_path: &Path,
) -> Result<Vec<String>, Status> {
    let spec = split_agent_spec()?;
    let (file, argv) = slots::mcp::render_mcp(
        &spec.agent.mcp,
        &tddy_tools_server(session_dir, tddy_tools_path),
        session_dir,
        session_dir,
        None,
    )
    .map_err(|e| Status::internal(e.to_string()))?;
    write_registrations(&mut file.into_iter().collect())?;
    Ok(argv)
}
