/**
 * The option/selection algebra behind the new-session form's **Agent** `<select>` — the agent a tool
 * session is started *as*, listed across every common-room host.
 *
 * Kept apart from `CreateSessionPane` and from `useSelectableAgents` because it is the branchy part:
 * how an option is keyed and captioned, and which agent a host change lands on. None of it needs
 * React, a transport or a mount to be exercised.
 *
 * Feature: docs/ft/web/1-WIP/PRD-2026-08-19-session-agent-host-fan-out.md
 */

/** One agent as a host offers it, attributed to the host whose `ListAgents` answered. */
export interface SelectableAgent {
  /** The bare id the offering daemon knows it by — the value sent as `StartSessionRequest.agent`. */
  readonly id: string;
  /** The daemon's caption. Empty for rows the daemon captions by id. */
  readonly label: string;
  readonly daemonInstanceId: string;
}

/**
 * The `<option>` value. Qualified as `{id}@{daemonInstanceId}` while hosts are advertised, because
 * two hosts routinely offer an agent called `claude` and a bare value cannot say which was picked;
 * bare when no host is advertised, where there is one host and nothing to disambiguate.
 */
export function selectableAgentValue(agent: SelectableAgent, hostsAdvertised: boolean): string {
  return hostsAdvertised ? `${agent.id}@${agent.daemonInstanceId}` : agent.id;
}

/**
 * The option's caption: the label (or the id), and the offering host when there is one to name. An
 * `<option>` renders text only — no markup — so the host is joined on with a middle dot rather than
 * carried in an element of its own the way the specialized-agent checkboxes carry theirs.
 */
export function selectableAgentText(agent: SelectableAgent, hostsAdvertised: boolean): string {
  const caption = agent.label || agent.id;
  return hostsAdvertised ? `${caption} · ${agent.daemonInstanceId}` : caption;
}

/**
 * The agent to select once `host` runs the session: the one of the same name **that host** offers,
 * else that host's first, else none — so the agent and the host on screen are never contradictory.
 */
export function agentForHost(
  agents: readonly SelectableAgent[],
  host: string,
  currentAgentId: string,
): SelectableAgent | null {
  // Scoped to `host` before the name is looked at: an agent of the same name on a third host is a
  // different agent, and selecting it would name a host that is not running the session.
  const offered = agents.filter((a) => a.daemonInstanceId === host);
  return offered.find((a) => a.id === currentAgentId) ?? offered[0] ?? null;
}
