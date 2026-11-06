import type { SelectableAgent } from "./selectableAgentOptions";

/** The session type that starts a workflow tool. */
export const TOOL_SESSION_TYPE = "tool";

/** The terminal kind that opens the host's login shell in a tab. */
export const BASH_KIND = "bash";

/** The terminal kind that attaches a tab to an existing tmux session on the host. */
export const TMUX_KIND = "tmux";

/**
 * The terminal kind that creates a fresh tmux session on the host and attaches a tab to it.
 * Only ever sent (with the name to give it); a live tab's kind is always {@link TMUX_KIND}.
 */
export const TMUX_NEW_KIND = "tmux-new";

/** One session type the Create pane's toggle offers: its value and its label. */
export interface SessionTypeOption {
  id: string;
  label: string;
}

/**
 * The session types a daemon offers, from the agents it served (`ListAgents`): `tool`, then
 * every agent with a PTY launch on that host, labelled by its spec's product. The bundle
 * hard-codes no agent — an agent the daemon does not serve is not offered.
 */
export function sessionTypeOptions(
  agents: readonly SelectableAgent[],
  daemonInstanceId: string,
  connectedInstanceId: string,
): SessionTypeOption[] {
  const wanted = new Set(
    [daemonInstanceId, connectedInstanceId].filter((id) => id !== ""),
  );
  const onHost = (agent: SelectableAgent) =>
    wanted.size === 0 || wanted.has(agent.daemonInstanceId);
  const options: SessionTypeOption[] = [{ id: TOOL_SESSION_TYPE, label: "Tool" }];
  const seen = new Set<string>([TOOL_SESSION_TYPE]);
  for (const agent of agents) {
    if (!onHost(agent) || agent.sessionType === "" || seen.has(agent.sessionType)) {
      continue;
    }
    seen.add(agent.sessionType);
    options.push({ id: agent.sessionType, label: agent.label || agent.id });
  }
  return options;
}

/** The CLI session types among `options` (every option but `tool`). */
export function cliSessionTypeOptions(
  options: readonly SessionTypeOption[],
): SessionTypeOption[] {
  return options.filter((option) => option.id !== TOOL_SESSION_TYPE);
}

/**
 * The session-type options most recently derived from a daemon's agents, for readers that
 * hold no catalog of their own (a terminal tab's label, the "+" menu's kinds). Registered by
 * every agent listing; before the first one, only `tool` is known.
 */
let knownSessionTypeOptions: SessionTypeOption[] = [
  { id: TOOL_SESSION_TYPE, label: "Tool" },
];

export function registerSessionTypeOptions(options: readonly SessionTypeOption[]): void {
  if (options.length > 1) {
    knownSessionTypeOptions = [...options];
  }
}

export function knownSessionTypes(): SessionTypeOption[] {
  return knownSessionTypeOptions;
}

/** What a terminal tab of `kind` is called, from the registered session-type options. */
export function terminalKindLabel(kind: string): string {
  if (kind === BASH_KIND) return "Bash";
  if (kind === TMUX_KIND || kind === TMUX_NEW_KIND) return "tmux";
  return (
    knownSessionTypeOptions.find((option) => option.id === kind)?.label ?? kind
  );
}
