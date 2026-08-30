import React, { useEffect, useMemo, useState } from "react";
import { ConnectError } from "@connectrpc/connect";
import {
  ConnectionService,
  type SessionAgentEntry,
  type SessionEntry,
} from "../../gen/connection_pb";
import { useHttpClient } from "../../rpc/transportProvider";
import { Button } from "../ui/button";
import { AgentPicker } from "./AgentPicker";
import { subagentSessionNodes } from "./agentTree";
import { rosterHalfOf, type RosterHalf } from "./sessionRosterHalf";
import { SessionAgentTree, type RosterDetachRequest } from "./SessionAgentTree";
import { type AvailableAgent } from "./useAvailableAgents";
import { useSessionAgentRoster } from "./useSessionAgentRoster";

/**
 * The **Agents tab** — a tree rooted at the session's own main agent, holding the specialized
 * agents attached to it and the subagent sessions it spawned, with the flows that change the
 * roster: an Add that says what the main agent loses, and a Detach that asks before it deletes a
 * checkout on another host.
 *
 * The root's roster comes from `StreamSessionAgents`, so an attach made from another browser tab or
 * by another operator appears without a refresh. Everything managed is keyed by the qualified
 * `agent_id` (`name@daemon_instance_id`), because two hosts routinely offer a def of the same name
 * and a bare name cannot say which one a row is. The subagent sessions come from the drawer's own
 * `ListSessions` list, folded by {@link subagentSessionNodes}.
 *
 * Feature: docs/ft/daemon/session-agent-roster.md § Web UI (AC50-AC53),
 * docs/ft/daemon/session-agent-roster.md § The Agents tab;
 * module notes in packages/tddy-web/docs/session-agent-tree.md.
 */

export interface SessionAgentRosterPaneProps {
  /**
   * The session the tab is about. The whole entry rather than an id: the root row is this session's
   * own agent, its subagents are matched by `orchestrator_session_id`, and the roster address is
   * derived here by `rosterHalfOf` — a pane handed both the entry and a separately-derived address
   * could be given a mismatched pair.
   */
  readonly session: SessionEntry;
  /** The drawer's full session list, from which this session's subagents are folded out. */
  readonly sessions: ReadonlyArray<SessionEntry>;
  readonly sessionToken: string;
  /**
   * Whether the session's daemon is reachable. Kept a prop rather than probed here: the surfaces
   * that mount this pane already know whether they hold a live daemon connection, and a
   * disconnected host must be shown as disconnected rather than as a roster that never loads.
   */
  readonly daemonConnected: boolean;
  /** Reports the session a subagent row's Switch names, so the host can focus it. */
  readonly onSwitchSubagent: (sessionId: string) => void;
}

/** The prefix every control of this pane's picker is named after — see {@link AgentPicker}. */
const PICKER_TEST_ID_PREFIX = "agent-roster-picker";

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

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
  session,
  sessions,
  sessionToken,
  daemonConnected,
  onSwitchSubagent,
}: SessionAgentRosterPaneProps) {
  // Addressed over the shared transport with the facilitating daemon named in the request, the way
  // every other session-scoped call in the inspector is routed (see `ExecuteTool`'s
  // `daemon_instance_id`) — the roster is served by the daemon that owns the session.
  const client = useHttpClient(ConnectionService);
  const { sessionId, daemonInstanceId } = rosterHalfOf(session);
  const { agents, hasSnapshot, error } = useSessionAgentRoster({
    client,
    sessionToken,
    sessionId,
    daemonInstanceId,
    enabled: daemonConnected,
  });
  const subagents = useMemo(
    () => subagentSessionNodes(sessions, session.sessionId),
    [sessions, session.sessionId],
  );
  // Ticked so a "4m ago" becomes "5m ago" without a roster frame. A minute is the resolution the
  // text itself has past the first minute; anything faster would re-render the pane for a string
  // that cannot change.
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    const timer = setInterval(() => setNow(Date.now()), 60_000);
    return () => clearInterval(timer);
  }, []);

  const [pickerOpen, setPickerOpen] = useState(false);
  const [pendingDetach, setPendingDetach] = useState<RosterDetachRequest | null>(null);
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

  const detach = async (entry: SessionAgentEntry, half: RosterHalf) => {
    setPendingDetach(null);
    try {
      await client.detachSessionAgent({
        sessionToken,
        sessionId: half.sessionId,
        daemonInstanceId: half.daemonInstanceId,
        agentId: entry.agentId,
      });
      setDetachError(null);
    } catch (err) {
      setDetachError(ConnectError.from(err).rawMessage);
    }
  };

  // A row anywhere in the tree asks here rather than detaching itself, so the confirmation is one
  // dialog. It carries the roster it belongs to and the half holding it: "the last agent of a remote
  // daemon" is a fact about that roster, not about the root's.
  const requestDetach = (request: RosterDetachRequest) => {
    if (detachDeletesACheckout(request.entry, request.roster, request.half.daemonInstanceId)) {
      setPendingDetach(request);
      return;
    }
    void detach(request.entry, request.half);
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
      ) : (
        <>
          <SessionAgentTree
            session={session}
            agents={agents}
            subagents={subagents}
            client={client}
            sessionToken={sessionToken}
            daemonConnected={daemonConnected}
            now={now}
            onSwitchSubagent={onSwitchSubagent}
            onDetach={requestDetach}
          />
          {/* Said only when the main agent has nobody working for it at all. A spawned subagent is
              not "no agents attached", so the tree speaks for itself in that case. */}
          {agents.length === 0 && subagents.length === 0 && (
            <p data-testid="agent-roster-empty" className="text-muted-foreground">
              No agents attached — the main agent keeps its full tool set.
            </p>
          )}
        </>
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
            {`Detaching ${pendingDetach.entry.agentId} deletes the checkout it works from on ${pendingDetach.entry.daemonInstanceId}.`}
          </span>
          <div className="flex gap-2">
            <Button
              data-testid="agent-roster-detach-confirm-btn"
              variant="destructive"
              size="sm"
              className="h-6 px-2 text-xs"
              onClick={() => void detach(pendingDetach.entry, pendingDetach.half)}
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
