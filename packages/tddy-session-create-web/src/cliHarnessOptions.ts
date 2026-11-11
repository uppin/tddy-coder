/**
 * The session types a daemon declares, as the bundle offers and branches on them. Every session
 * type comes from a daemon's `ListAgents` answer (`session_types`: its tool files'
 * `extensions.session` sections), with the label it is offered under and its **launch kind** — the
 * one thing a form or a tab decides by. The bundle spells no session type of its own except the one
 * no tool file declares: {@link NO_AGENT_SESSION_TYPE}, mirroring the daemon's
 * `tddy_workflow::NO_AGENT_SESSION_TYPE`.
 */

/** The wire value of a session with no agent of its own — a worktree. Declared by no tool file. */
export const NO_AGENT_SESSION_TYPE = "workspace";

/** The terminal kind that opens the host's login shell in a tab. */
export const BASH_KIND = "bash";

/** The terminal kind that attaches a tab to an existing tmux session on the host. */
export const TMUX_KIND = "tmux";

/**
 * The terminal kind that creates a fresh tmux session on the host and attaches a tab to it.
 * Only ever sent (with the name to give it); a live tab's kind is always {@link TMUX_KIND}.
 */
export const TMUX_NEW_KIND = "tmux-new";

/** A coding agent's interactive CLI: the agent the session type names, in a terminal. */
export const AGENT_CLI_LAUNCH = "agent_cli";

/** The tddy-coder workflow tool, run by the start's `agent` from the daemon's `allowed_tools`. */
export const TDDY_CODER_LAUNCH = "tddy_coder";

/** How a session of a type is launched, as the daemon serves it, or the no-agent session's. */
export type SessionLaunch = typeof AGENT_CLI_LAUNCH | typeof TDDY_CODER_LAUNCH | "no_agent";

/** One session type a host declares, attributed to the host whose `ListAgents` answered. */
export interface ServedSessionType {
  readonly sessionType: string;
  readonly label: string;
  /** The launch kind the daemon declared (`agent_cli`, `tddy_coder`); any other word is unknown. */
  readonly launch: string;
  readonly daemonInstanceId: string;
}

/** One session type the Create pane's toggle offers: its value, its label and its launch kind. */
export interface SessionTypeOption {
  id: string;
  label: string;
  launch: string;
}

/**
 * The session types a daemon offers, from the session types it declared (`ListAgents`): those that
 * run the tddy-coder workflow tool first, then the coding agents' CLIs, each in the daemon's order.
 * The bundle hard-codes none — a session type the daemon does not declare is not offered.
 */
export function sessionTypeOptions(
  sessionTypes: readonly ServedSessionType[],
  daemonInstanceId: string,
  connectedInstanceId: string,
): SessionTypeOption[] {
  const wanted = new Set(
    [daemonInstanceId, connectedInstanceId].filter((id) => id !== ""),
  );
  const onHost = (served: ServedSessionType) =>
    wanted.size === 0 || wanted.has(served.daemonInstanceId);
  const options: SessionTypeOption[] = [];
  const seen = new Set<string>();
  for (const served of sessionTypes) {
    if (!onHost(served) || served.sessionType === "" || seen.has(served.sessionType)) {
      continue;
    }
    seen.add(served.sessionType);
    options.push({
      id: served.sessionType,
      label: served.label || served.sessionType,
      launch: served.launch,
    });
  }
  const tddyCoderFirst = (option: SessionTypeOption) =>
    option.launch === TDDY_CODER_LAUNCH ? 0 : 1;
  return options.sort((a, b) => tddyCoderFirst(a) - tddyCoderFirst(b));
}

/** The coding agents' CLI session types among `options`. */
export function cliSessionTypeOptions(
  options: readonly SessionTypeOption[],
): SessionTypeOption[] {
  return options.filter((option) => option.launch === AGENT_CLI_LAUNCH);
}

/**
 * How a session of `sessionType` is launched, read from `options` (the served session types), or
 * `no_agent` for {@link NO_AGENT_SESSION_TYPE}. `undefined` for a type none of them declares — the
 * bundle does not guess.
 */
export function sessionLaunchOf(
  sessionType: string | undefined,
  options: readonly SessionTypeOption[],
): SessionLaunch | undefined {
  if (sessionType === NO_AGENT_SESSION_TYPE) return "no_agent";
  const launch = options.find((option) => option.id === sessionType)?.launch;
  return launch === AGENT_CLI_LAUNCH || launch === TDDY_CODER_LAUNCH ? launch : undefined;
}

/**
 * The session-type options most recently served by a daemon, for readers that hold no catalog of
 * their own (a terminal tab's label, the "+" menu's kinds, the attach rules). Registered by every
 * agent listing; before the first one, none is known.
 */
let knownSessionTypeOptions: SessionTypeOption[] = [];

export function registerSessionTypeOptions(options: readonly SessionTypeOption[]): void {
  if (options.length > 0) {
    knownSessionTypeOptions = [...options];
  }
}

export function knownSessionTypes(): SessionTypeOption[] {
  return knownSessionTypeOptions;
}

/** How a session of `sessionType` is launched, from the registered session-type options. */
export function knownSessionLaunch(sessionType: string | undefined): SessionLaunch | undefined {
  return sessionLaunchOf(sessionType, knownSessionTypeOptions);
}

/** What a terminal tab of `kind` is called, from the registered session-type options. */
export function terminalKindLabel(kind: string): string {
  if (kind === BASH_KIND) return "Bash";
  if (kind === TMUX_KIND || kind === TMUX_NEW_KIND) return "tmux";
  return (
    knownSessionTypeOptions.find((option) => option.id === kind)?.label ?? kind
  );
}
