// Served by the agent's own `tddy-tools --mcp` child, the same one that carries the exec tools to
// the codebase daemon.
use tddy_sandbox_recipes::PERMISSION_PROMPT_TOOL;

use super::NATIVE_FILESYSTEM_TOOLS;

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
    // Every tool call a split session makes crosses LiveKit to the codebase daemon, and every way
    // that can fail is reported by `tddy-tools` itself. Claude Code captures an MCP server's stderr,
    // so without a log file those reports exist only inside a process nobody can attach to: a split
    // session whose dispatch is failing would leave no evidence on either daemon. The sandbox path
    // solves this the same way, pointing the same variable at its egress dir.
    let mcp_env = BTreeMap::from([
        (
            "TDDY_TOOLS_LOG_FILE".to_string(),
            split_mcp_log_path(session_dir)
                .to_string_lossy()
                .into_owned(),
        ),
        ("RUST_LOG".to_string(), MCP_RUST_LOG.to_string()),
    ]);
    let mcp_config = tddy_sandbox_recipes::write_claude_mcp_config(
        session_dir,
        Path::new(tddy_tools_path),
        &mcp_env,
    )
    .map_err(|e| Status::internal(format!("failed to write MCP config: {e}")))?;

    let withdrawn = withdrawn_tools(withdrawals);
    let withdrawn_refs: Vec<&str> = withdrawn.iter().map(String::as_str).collect();

    let mut args = Vec::new();
    // Every exec tool the roster leaves alone is pre-approved: they are all reachable, they simply
    // execute on the codebase daemon. The subagent tools are pre-approved whether or not this
    // session has agents *yet* — unlike the jail, which reads a seed fixed at spawn, a split
    // session's roster is live and an operator may attach an agent at minute forty, while Claude's
    // own lists are fixed for the life of the process they were passed to. Nothing is granted by
    // pre-approving them on a session with no agents: `tddy-tools` advertises them only while the
    // roster has someone to address, so the flag names a tool the model is never offered.
    let subagent_tools_are_pre_approved = true;
    for tool in tddy_sandbox_recipes::build_claude_allowlist(
        subagent_tools_are_pre_approved,
        &withdrawn_refs,
    ) {
        args.push("--allowedTools".to_string());
        args.push(tool);
    }
    // Dropping a tool from `--allowedTools` only un-pre-approves it. A split session's *native*
    // routes are already hard-disabled below, but a withdrawn tool's proxied `mcp__tddy-tools__`
    // form is the route this agent actually had, and it stays callable through the permission
    // prompt until `--disallowedTools` names it (`PermissionServer::decide` allows every
    // `mcp__tddy-tools__*` call it is asked about).
    let mut disallowed: Vec<String> = NATIVE_FILESYSTEM_TOOLS
        .iter()
        .map(|tool| (*tool).to_string())
        .collect();
    for tool in tddy_sandbox_recipes::build_claude_disallowlist(&withdrawn_refs) {
        if !disallowed.contains(&tool) {
            disallowed.push(tool);
        }
    }
    for tool in disallowed {
        args.push("--disallowedTools".to_string());
        args.push(tool);
    }
    args.push("--permission-prompt-tool".to_string());
    args.push(PERMISSION_PROMPT_TOOL.to_string());
    args.push("--mcp-config".to_string());
    args.push(mcp_config.to_string_lossy().into_owned());
    // `--mcp-config` alone *adds* to the user-scoped MCP configuration, so any filesystem or shell
    // MCP server the operator has configured would load beside `tddy-tools` — reachable under an
    // `mcp__*` name the disallowlist above does not cover, on this host rather than the codebase
    // host. The restriction the split placement rests on has to be impossible to route around, not
    // merely the default, so this config is the only one loaded.
    args.push("--strict-mcp-config".to_string());
    Ok(args)
}
