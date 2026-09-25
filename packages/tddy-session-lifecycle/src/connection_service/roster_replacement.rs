/// Build the (agent name, replaced-tools) pairs a session's roster withdraws — one per attached
/// agent, each with its own `replaces`, normalized.
///
/// From the roster's snapshot of `replaces` rather than from the def each entry resolved from:
/// editing a YAML def or a registry assistant under a running session must not change what its main
/// agent may call (PRD § An entry).
///
/// The single source of what a session's roster withdraws: every spawn path — a fresh sandboxed
/// `claude-cli` session, a fresh sandboxed `cursor-cli` one, and a relaunch of either — computes the
/// withdrawal by calling this on the roster it is starting from, so there is one answer to derive
/// an appendix, an allowlist or a disallowlist from.
pub fn roster_replacement_pairs(
    agents: &[tddy_core::SessionAgentRecord],
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
