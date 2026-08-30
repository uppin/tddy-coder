import { SessionAgentStatus } from "../../gen/connection_pb";

/**
 * The vocabulary every agent badge in the Agents tab draws from.
 *
 * Shared rather than private to one row: a managed roster agent reports `SessionAgentEntry.status`
 * and a non-managed subagent session reports the inferred `SessionEntry.agent_status`, and the proto
 * ships one enum for both precisely so one badge renders them alike.
 */

/**
 * What the agent is doing, as the badge's text and its `data-agent-status`.
 *
 * `UNSPECIFIED` is "unknown", never "idle". The daemon sends it when it has nothing to say — a
 * roster restored from `.session.yaml` after a restart, or a session type it does not tail at all —
 * and an operator told an agent is idle reads that as "free, ready for work", which is a different
 * claim from "nobody here knows".
 */
const AGENT_STATUS_NAMES: Record<SessionAgentStatus, string> = {
  [SessionAgentStatus.UNSPECIFIED]: "unknown",
  [SessionAgentStatus.IDLE]: "idle",
  [SessionAgentStatus.RUNNING]: "running",
  [SessionAgentStatus.EXECUTING_TOOL]: "executing tool",
  [SessionAgentStatus.WAITING_FOR_INPUT]: "waiting for input",
  [SessionAgentStatus.CONNECTING]: "connecting",
  [SessionAgentStatus.ERROR]: "error",
};

/** Kebab-case, for `data-agent-status` — a test and a stylesheet want one stable token. */
const AGENT_STATUS_TOKENS: Record<SessionAgentStatus, string> = {
  [SessionAgentStatus.UNSPECIFIED]: "unknown",
  [SessionAgentStatus.IDLE]: "idle",
  [SessionAgentStatus.RUNNING]: "running",
  [SessionAgentStatus.EXECUTING_TOOL]: "executing-tool",
  [SessionAgentStatus.WAITING_FOR_INPUT]: "waiting-for-input",
  [SessionAgentStatus.CONNECTING]: "connecting",
  [SessionAgentStatus.ERROR]: "error",
};

export function agentStatusName(status: SessionAgentStatus): string {
  return AGENT_STATUS_NAMES[status] ?? String(status);
}

export function agentStatusToken(status: SessionAgentStatus): string {
  return AGENT_STATUS_TOKENS[status] ?? String(status);
}

/**
 * Whether a status is one an operator would look at twice. Drives the badge's emphasis only —
 * nothing is hidden on the strength of it, because a row whose state this build cannot name is
 * exactly the row worth showing.
 */
export function statusIsWorking(status: SessionAgentStatus): boolean {
  return (
    status === SessionAgentStatus.RUNNING ||
    status === SessionAgentStatus.EXECUTING_TOOL ||
    status === SessionAgentStatus.WAITING_FOR_INPUT
  );
}

/**
 * `last_activity` as "<summary> · <how long ago>".
 *
 * Relative rather than a clock time: the question an operator asks a roster row is "is this moving?",
 * and "4m ago" answers it without them having to subtract. Rendered from a `now` passed in rather
 * than read here so the caller can tick it — and so a test can pin one.
 *
 * A stamp in the future is shown as "just now" rather than as a negative age: clocks on two hosts
 * disagree by seconds routinely, and "in -3s" reads as a bug in the page.
 */
export function lastActivityText(summary: string, atUnixMs: bigint, nowMs: number): string {
  const ageMs = nowMs - Number(atUnixMs);
  if (ageMs < 0 || ageMs < 5_000) return `${summary} · just now`;
  const seconds = Math.floor(ageMs / 1000);
  if (seconds < 60) return `${summary} · ${seconds}s ago`;
  const minutes = Math.floor(seconds / 60);
  if (minutes < 60) return `${summary} · ${minutes}m ago`;
  const hours = Math.floor(minutes / 60);
  if (hours < 24) return `${summary} · ${hours}h ago`;
  return `${summary} · ${Math.floor(hours / 24)}d ago`;
}
