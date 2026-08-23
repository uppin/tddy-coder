import React, { useState } from "react";
import { ConnectError } from "@connectrpc/connect";
import {
  AgentCloneState,
  ConnectionService,
  type SessionAgentEntry,
} from "../../gen/connection_pb";
import { safeTestIdPart } from "../../lib/testId";
import { useSelectedDaemon } from "../../rpc/selectedDaemon";
import { useHttpClient } from "../../rpc/transportProvider";
import { Button } from "../ui/button";
import { useAvailableAgents, type AvailableAgent } from "./useAvailableAgents";
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
const pickerOptionTestId = (agentId: string) =>
  `agent-roster-picker-option-${safeTestIdPart(agentId)}`;

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
  // Which host the pane's own client reaches: the origin, i.e. the daemon that served this bundle.
  // Not the same idea as `selectedInstanceId`, which an operator can point at a peer host without
  // changing where this HTTP transport lands.
  const { servingInstanceId } = useSelectedDaemon();
  const { agents, hasSnapshot, error } = useSessionAgentRoster({
    client,
    sessionToken,
    sessionId,
    daemonInstanceId,
    enabled: daemonConnected,
  });
  // The catalog is a fan-out, and its home is the host behind the client it is handed — not the host
  // that owns the session. `ListSubagents` carries no routing field and a daemon never forwards it,
  // so the fan-out reads its home host through `client` and addresses every *other* common-room
  // daemon over LiveKit RPC. Naming the facilitating daemon here would invert both halves of that
  // for a split session — the codebase host asked over an HTTP route that does not reach it, the
  // connected host addressed as a peer — which is why such a session showed an empty catalog.
  const available = useAvailableAgents(client, servingInstanceId ?? "");

  const [pickerOpen, setPickerOpen] = useState(false);
  const [pickedAgent, setPickedAgent] = useState<AvailableAgent | null>(null);
  const [attachError, setAttachError] = useState<string | null>(null);
  const [pendingDetach, setPendingDetach] = useState<SessionAgentEntry | null>(null);
  const [detachError, setDetachError] = useState<string | null>(null);

  const closePicker = () => {
    setPickerOpen(false);
    setPickedAgent(null);
    setAttachError(null);
  };

  const attach = async (agent: AvailableAgent) => {
    try {
      await client.attachSessionAgent({
        sessionToken,
        sessionId,
        daemonInstanceId,
        agentId: agent.agentId,
      });
    } catch (err) {
      setAttachError(ConnectError.from(err).rawMessage);
      return;
    }
    // The roster itself arrives on the stream, so nothing is written here — the pane shows what the
    // daemon says the roster is, not what this browser asked for.
    closePicker();
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
                </span>
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
        <div
          data-testid="agent-roster-picker"
          className="flex flex-col gap-2 rounded-md border border-border bg-background p-2"
        >
          {/* A host that could not answer costs its own row and nothing else — the other hosts'
              agents stay on offer. */}
          {available.failures.map((failure) => (
            <p
              key={failure.daemonInstanceId}
              data-testid={`agent-roster-picker-host-error-${safeTestIdPart(failure.daemonInstanceId)}`}
              className="text-destructive"
            >
              {`${failure.daemonInstanceId}: ${failure.message}`}
            </p>
          ))}

          {available.agents.length === 0 && available.failures.length === 0 ? (
            <p className="text-muted-foreground">No agents on offer.</p>
          ) : (
            <ul className="flex flex-col gap-1">
              {available.agents.map((agent) => (
                <li key={agent.agentId}>
                  <button
                    type="button"
                    data-testid={pickerOptionTestId(agent.agentId)}
                    aria-pressed={pickedAgent?.agentId === agent.agentId}
                    onClick={() => {
                      setPickedAgent(agent);
                      setAttachError(null);
                    }}
                    className={`flex w-full items-center justify-between gap-2 rounded-md border px-2 py-1 text-left ${
                      pickedAgent?.agentId === agent.agentId
                        ? "border-foreground"
                        : "border-transparent hover:border-border"
                    }`}
                  >
                    <span className="truncate">{agent.label || agent.name}</span>
                    <span
                      data-testid={`${pickerOptionTestId(agent.agentId)}-host`}
                      className="text-muted-foreground"
                    >
                      {agent.daemonInstanceId}
                    </span>
                  </button>
                </li>
              ))}
            </ul>
          )}

          {/* The cost of the attach, stated before it is made: every tool named here stops being
              callable by the main agent for as long as this agent stays attached. */}
          {pickedAgent !== null && (
            <>
              <p
                data-testid="agent-roster-picker-withdrawal-warning"
                className="text-muted-foreground"
              >
                {pickedAgent.replaces.length === 0
                  ? `${pickedAgent.agentId} takes no tools away from the main agent.`
                  : `The main agent loses ${pickedAgent.replaces.join(", ")} while ${pickedAgent.agentId} is attached.`}
              </p>
              {attachError !== null && <p className="text-destructive">{attachError}</p>}
              <div className="flex gap-2">
                <Button
                  data-testid="agent-roster-picker-confirm-btn"
                  size="sm"
                  className="h-6 px-2 text-xs"
                  onClick={() => void attach(pickedAgent)}
                >
                  Attach
                </Button>
                <Button
                  variant="outline"
                  size="sm"
                  className="h-6 px-2 text-xs"
                  onClick={closePicker}
                >
                  Cancel
                </Button>
              </div>
            </>
          )}
        </div>
      )}
    </div>
  );
}
