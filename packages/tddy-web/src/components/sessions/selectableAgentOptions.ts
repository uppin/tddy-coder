/**
 * The option/selection algebra behind the new-session form's **Agent** `<select>` — the agent a tool
 * session is started *as*, listed across every common-room host.
 *
 * Kept apart from `CreateSessionPane` and from `useSelectableAgents` because it is the branchy part:
 * how an option is keyed and captioned, and which agent a host change lands on. None of it needs
 * React, a transport or a mount to be exercised.
 *
 * Feature: docs/ft/web/session-agent-catalog-fan-out.md
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
 * The host whose agents a session may be started as, given the host the form will *ask for* and the
 * daemon the browser is connected to.
 *
 * The empty string is not the absence of a host. It is the protocol's other spelling for one:
 * `StartSessionRequest.daemon_instance_id` is optional, and a daemon reads an empty one as "the
 * daemon this request arrived on" — which, for this form, is the daemon the browser is connected to.
 * The peer-spawn flow inherits that spelling from an orchestrator that was itself started without an
 * explicit host.
 *
 * So this is a decode of two spellings of one decided host, not a substitute for one that is missing:
 * there is no failure case to absorb, both arguments are always present, and the host named here is
 * the same host the request reaches either way. `requestedHost` is what stays on the wire.
 */
export function hostRunningSession(requestedHost: string, connectedHost: string): string {
  return requestedHost === "" ? connectedHost : requestedHost;
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
