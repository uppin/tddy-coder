import React, { useEffect, useState } from "react";
import { ConnectError } from "@connectrpc/connect";
import {
  AgentCloneState,
  ConnectionService,
  SessionAgentStatus,
  type SessionAgentEntry,
} from "../../gen/connection_pb";
import { safeTestIdPart } from "../../lib/testId";
import { useHttpClient } from "../../rpc/transportProvider";
import { Button } from "../ui/button";
import { AgentPicker } from "./AgentPicker";
import { type AvailableAgent } from "./useAvailableAgents";
import { useSessionAgentRoster } from "./useSessionAgentRoster";

/**
 * The **Agent roster** — the specialized agents attached to a live session, with the flows that
 * change it: an Add that says what the main agent loses, and a Detach that asks before it deletes a
 * checkout on another host.
 *
 * Not to be confused with `SessionAgentsSection`, which lists a session's peer *child sessions*.
 * This pane is about agents attached to one session; the two share only the word "agent".
 *
 * Rows come from `StreamSessionAgents`, so an attach made from another browser tab or by another
 * operator appears without a refresh. Everything is keyed by the qualified `agent_id`
 * (`name@daemon_instance_id`), because two hosts routinely offer a def of the same name and a bare
 * name cannot say which one a row is.
 *
 * Feature: docs/ft/daemon/session-agent-roster.md § Web UI (AC50-AC53).
 */

export interface SessionAgentRosterPaneProps {
  readonly sessionId: string;
  readonly sessionToken: string;
  /** The daemon facilitating the session — it owns the roster. Empty means the serving daemon. */
  readonly daemonInstanceId: string;
  /**
   * Whether that daemon is reachable. Kept a prop rather than probed here: the surfaces that mount
   * this pane already know whether they hold a live daemon connection, and a disconnected host must
   * be shown as disconnected rather than as a roster that never loads.
   */
  readonly daemonConnected: boolean;
}

// ---------------------------------------------------------------------------
// Test ids — mirrored in `cypress/support/testIds.ts` (`agentRoster*`).
// ---------------------------------------------------------------------------

const rowTestId = (agentId: string) => `agent-roster-row-${safeTestIdPart(agentId)}`;
/** The prefix every control of this pane's picker is named after — see {@link AgentPicker}. */
const PICKER_TEST_ID_PREFIX = "agent-roster-picker";

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/**
 * The clone state as a word, for both the badge's text and its `data-clone-state`. `local` is a
 * state of its own and not a synonym for `ready`: there is no checkout behind a local agent, so
 * saying "ready" would imply one exists.
 */
const CLONE_STATE_NAMES: Record<AgentCloneState, string> = {
  [AgentCloneState.UNSPECIFIED]: "unspecified",
  [AgentCloneState.LOCAL]: "local",
  [AgentCloneState.PROVISIONING]: "provisioning",
  [AgentCloneState.READY]: "ready",
  [AgentCloneState.ERROR]: "error",
};

/** A value this build has no name for is shown as itself, not folded into a state it is not. */
function cloneStateName(state: AgentCloneState): string {
  return CLONE_STATE_NAMES[state] ?? String(state);
}

/**
 * What the agent is doing, as the badge's text and its `data-agent-status`.
 *
 * `UNSPECIFIED` is "unknown", never "idle". The daemon sends it when it has nothing to say — a
 * roster restored from `.session.yaml` after a restart, or an entry no signal has reached yet — and
 * an operator told an agent is idle reads that as "free, ready for work", which is a different
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

function agentStatusName(status: SessionAgentStatus): string {
  return AGENT_STATUS_NAMES[status] ?? String(status);
}

function agentStatusToken(status: SessionAgentStatus): string {
  return AGENT_STATUS_TOKENS[status] ?? String(status);
}

/**
 * Whether a status is one an operator would look at twice. Drives the badge's emphasis only —
 * nothing is hidden on the strength of it, because a row whose state this build cannot name is
 * exactly the row worth showing.
 */
function statusIsWorking(status: SessionAgentStatus): boolean {
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

/**
 * Whether detaching `entry` would delete a checkout on another host — the one detach an operator
 * cannot undo by re-attaching, because the clone and its uncommitted contents go with it. True only
 * for an agent owned by a *remote* daemon and only when it is that daemon's last one: the daemon
 * keeps one clone per owning host, so removing any earlier entry leaves the checkout in place.
 */
function detachDeletesACheckout(
  entry: SessionAgentEntry,
  roster: ReadonlyArray<SessionAgentEntry>,
  facilitatingInstanceId: string,
): boolean {
  if (entry.daemonInstanceId === facilitatingInstanceId) return false;
  return roster.filter((a) => a.daemonInstanceId === entry.daemonInstanceId).length === 1;
}

// ---------------------------------------------------------------------------
// Component
// ---------------------------------------------------------------------------

export function SessionAgentRosterPane({
  sessionId,
  sessionToken,
  daemonInstanceId,
  daemonConnected,
}: SessionAgentRosterPaneProps) {
  // Addressed over the shared transport with the facilitating daemon named in the request, the way
  // every other session-scoped call in the inspector is routed (see `ExecuteTool`'s
  // `daemon_instance_id`) — the roster is served by the daemon that owns the session.
  const client = useHttpClient(ConnectionService);
  const { agents, hasSnapshot, error } = useSessionAgentRoster({
    client,
    sessionToken,
    sessionId,
    daemonInstanceId,
    enabled: daemonConnected,
  });
  // Ticked so a "4m ago" becomes "5m ago" without a roster frame. A minute is the resolution the
  // text itself has past the first minute; anything faster would re-render the pane for a string
  // that cannot change.
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    const timer = setInterval(() => setNow(Date.now()), 60_000);
    return () => clearInterval(timer);
  }, []);

  const [pickerOpen, setPickerOpen] = useState(false);
  const [pendingDetach, setPendingDetach] = useState<SessionAgentEntry | null>(null);
  const [detachError, setDetachError] = useState<string | null>(null);

  const attach = async (agent: AvailableAgent): Promise<string | null> => {
    try {
      await client.attachSessionAgent({
        sessionToken,
        sessionId,
        daemonInstanceId,
        agentId: agent.agentId,
      });
    } catch (err) {
      return ConnectError.from(err).rawMessage;
    }
    // The roster itself arrives on the stream, so nothing is written here — the pane shows what the
    // daemon says the roster is, not what this browser asked for.
    return null;
  };

  const detach = async (entry: SessionAgentEntry) => {
    setPendingDetach(null);
    try {
      await client.detachSessionAgent({
        sessionToken,
        sessionId,
        daemonInstanceId,
        agentId: entry.agentId,
      });
      setDetachError(null);
    } catch (err) {
      setDetachError(ConnectError.from(err).rawMessage);
    }
  };

  const requestDetach = (entry: SessionAgentEntry) => {
    if (detachDeletesACheckout(entry, agents, daemonInstanceId)) {
      setPendingDetach(entry);
      return;
    }
    void detach(entry);
  };

  return (
    <div
      data-testid="agent-roster-pane"
      className="flex flex-col gap-2 px-3 py-3 text-xs"
    >
      <div className="flex items-center justify-between gap-2">
        <span className="font-medium text-muted-foreground">Agent roster</span>
        <Button
          data-testid="agent-roster-add-btn"
          variant="outline"
          size="sm"
          className="h-6 px-2 text-xs"
          onClick={() => setPickerOpen(true)}
        >
          Add agent
        </Button>
      </div>

      {/* The four states are mutually exclusive and none of them is a blank panel: an unreadable
          roster and an empty one look identical otherwise, and only one of them means "no agents". */}
      {!daemonConnected ? (
        <p data-testid="agent-roster-disconnected" className="text-muted-foreground">
          {`Not connected to ${daemonInstanceId || "the session's daemon"} — the roster cannot be read.`}
        </p>
      ) : error !== null ? (
        <p data-testid="agent-roster-error" className="text-destructive">
          {error}
        </p>
      ) : !hasSnapshot ? (
        <p data-testid="agent-roster-loading" className="text-muted-foreground">
          Loading agents…
        </p>
      ) : agents.length === 0 ? (
        <p data-testid="agent-roster-empty" className="text-muted-foreground">
          No agents attached — the main agent keeps its full tool set.
        </p>
      ) : (
        <ul className="flex flex-col gap-1">
          {agents.map((entry) => (
            <li
              key={entry.agentId}
              data-testid={rowTestId(entry.agentId)}
              className="flex items-center justify-between gap-2 rounded-md border border-border bg-background px-2 py-1"
            >
              <div className="flex min-w-0 flex-col">
                <span className="truncate font-medium">{entry.label || entry.agentId}</span>
                <span className="truncate text-muted-foreground">
                  {`${entry.agentId}${entry.model ? ` · ${entry.model}` : ""}`}
                </span>
                <span className="flex flex-wrap items-center gap-1 text-muted-foreground">
                  <span data-testid={`${rowTestId(entry.agentId)}-host`}>
                    {entry.daemonInstanceId}
                  </span>
                  <span
                    data-testid={`${rowTestId(entry.agentId)}-clone-state`}
                    data-clone-state={cloneStateName(entry.cloneState)}
                    title={entry.cloneError}
                  >
                    {cloneStateName(entry.cloneState)}
                  </span>
                  {/* Always rendered, including for UNSPECIFIED: a row with no status badge and a
                      row whose daemon has nothing to say look identical otherwise, and only one of
                      them is a build that forgot to send the field. */}
                  <span
                    data-testid={`${rowTestId(entry.agentId)}-status`}
                    data-agent-status={agentStatusToken(entry.status)}
                    className={
                      statusIsWorking(entry.status)
                        ? "font-medium text-foreground"
                        : "text-muted-foreground"
                    }
                  >
                    {agentStatusName(entry.status)}
                  </span>
                </span>
                {/* Only when there is one. An empty line reserved for an agent nothing has been
                    observed of is a row that looks like it lost its history. */}
                {entry.lastActivity !== undefined && (
                  <span
                    data-testid={`${rowTestId(entry.agentId)}-last-activity`}
                    className="truncate text-muted-foreground"
                    title={entry.lastActivity.summary}
                  >
                    {lastActivityText(
                      entry.lastActivity.summary,
                      entry.lastActivity.atUnixMs,
                      now,
                    )}
                  </span>
                )}
                {/* What the main agent lost to this row — the reason an operator would detach it. */}
                <span
                  data-testid={`${rowTestId(entry.agentId)}-replaces`}
                  className="truncate text-muted-foreground"
                >
                  {entry.replaces.length === 0
                    ? "takes no tools from the main agent"
                    : `replaces ${entry.replaces.join(", ")}`}
                </span>
              </div>
              <Button
                data-testid={`${rowTestId(entry.agentId)}-detach-btn`}
                variant="ghost"
                size="sm"
                className="h-6 px-2 text-xs"
                onClick={() => requestDetach(entry)}
              >
                Detach
              </Button>
            </li>
          ))}
        </ul>
      )}

      {detachError !== null && <p className="text-destructive">{detachError}</p>}

      {/* Detaching the last agent of a remote daemon deletes that daemon's checkout, so the host
          losing it is named before anything is sent. */}
      {pendingDetach !== null && (
        <div
          data-testid="agent-roster-detach-confirm"
          className="flex flex-col gap-2 rounded-md border border-border bg-background p-2"
        >
          <span>
            {`Detaching ${pendingDetach.agentId} deletes the checkout it works from on ${pendingDetach.daemonInstanceId}.`}
          </span>
          <div className="flex gap-2">
            <Button
              data-testid="agent-roster-detach-confirm-btn"
              variant="destructive"
              size="sm"
              className="h-6 px-2 text-xs"
              onClick={() => void detach(pendingDetach)}
            >
              Detach and delete
            </Button>
            <Button
              variant="outline"
              size="sm"
              className="h-6 px-2 text-xs"
              onClick={() => setPendingDetach(null)}
            >
              Keep it
            </Button>
          </div>
        </div>
      )}

      {pickerOpen && (
        <AgentPicker
          testIdPrefix={PICKER_TEST_ID_PREFIX}
          errorTestId="agent-roster-attach-error"
          onAttach={attach}
          onClose={() => setPickerOpen(false)}
        />
      )}
    </div>
  );
}
