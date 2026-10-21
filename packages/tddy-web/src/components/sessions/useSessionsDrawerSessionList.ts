import { useMemo, type MutableRefObject } from "react";
import type { Client } from "@connectrpc/connect";
import type { Room } from "livekit-client";
import { SessionService } from "../../gen/session_pb";
import { useHostConnection } from "../../rpc/connections/registry";
import { useCapabilityAvailability } from "../../hooks/useCapabilityAvailability";
import { useRoomParticipants } from "../../hooks/useRoomParticipants";
import { useSessionManager } from "./sessionManager";
import { withoutJailedCodebaseHalves } from "./jailedCodebaseHalf";

export interface UseSessionsDrawerSessionListArgs {
  client: Client<typeof SessionService> | null;
  sessionToken: string;
  room: Room | null;
  selectedInstanceId: string | null;
  selectedSessionId: string | null;
  listGenerationRef: MutableRefObject<number>;
}

/**
 * The sessions drawer's merged session list and what it derives from it: the selected session, the
 * list-snapshot generation, and the drawer's own grouping of the list.
 */
export function useSessionsDrawerSessionList({
  client,
  sessionToken,
  room,
  selectedInstanceId,
  selectedSessionId,
  listGenerationRef,
}: UseSessionsDrawerSessionListArgs) {
  // The merged session list, refresh, and change events all live in one place: `SessionManager`. It
  // unions the selected host's sessions (from ListSessions, refreshed via the window-bound
  // `sessionsRefreshBridge`) with the live cross-host sessions observed as common-room coder
  // participants — LiveKit presence is the keep-alive that makes a non-selected host's session visible.
  const participants = useRoomParticipants(room);
  // Whether this list can see the other hosts at all. The cross-host half of it is presence — a
  // session on a non-selected host is observed as a common-room coder participant — so on a wire
  // that carries none the union silently collapses to `ListSessions` for the selected host, which
  // reads as "this host has no other sessions". The drawer says so instead (PRD AC 5).
  //
  // Read through the same rule the LiveKit screen uses, so a room that is merely mid-join does not
  // produce a claim about the connection that the next second withdraws.
  const selectedHost = useHostConnection(selectedInstanceId);
  const crossHostSessionsVisible =
    useCapabilityAvailability(selectedHost, "presence") !== "unavailable";
  const { sessions: sortedSessions, addOptimisticSession, sessionMetadataBySessionId } = useSessionManager(
    client,
    sessionToken,
    participants,
    selectedInstanceId ?? "",
  );

  const selectedSession = useMemo(
    () => sortedSessions.find((s) => s.sessionId === selectedSessionId) ?? null,
    [sortedSessions, selectedSessionId],
  );

  // Count each session-list snapshot as it arrives. Done during render (not in an effect) so a claim
  // taken from an event handler in the same commit records the snapshot the operator was actually
  // looking at, rather than one the effect queue has not stamped yet.
  const listGeneration = useMemo(() => {
    listGenerationRef.current += 1;
    return listGenerationRef.current;
  }, [sortedSessions]);

  // A pr-stack orchestrator is itself idle while its children do the work; when a child is live,
  // the orchestrator must stay reachable in the drawer (grouped with its children) rather than
  // collapsing into the disconnected "Remaining" partition mid-flight. Reflect that by treating an
  // orchestrator as active whenever a session it owns is active. This only affects drawer grouping;
  // the main pane keeps the raw list so branch→session resolution reads each child's true activity.
  const drawerSessions = useMemo(() => {
    // A sandboxed-codebase placement is served by two sessions on one daemon; only the agent half
    // is a session the operator started. Dropped here rather than in `sortedSessions` for the same
    // reason the orchestrator rule lives here: the main pane keeps the raw list, so branch→session
    // resolution and `codebase_session_id` lookups still see both halves.
    const listed = withoutJailedCodebaseHalves(sortedSessions);
    const activeOrchestratorIds = new Set(
      listed
        .filter((s) => s.isActive && s.orchestratorSessionId.length > 0)
        .map((s) => s.orchestratorSessionId),
    );
    if (activeOrchestratorIds.size === 0) return listed;
    return listed.map((s) =>
      !s.isActive && activeOrchestratorIds.has(s.sessionId) ? { ...s, isActive: true } : s,
    );
  }, [sortedSessions]);

  return {
    crossHostSessionsVisible,
    sortedSessions,
    addOptimisticSession,
    sessionMetadataBySessionId,
    selectedSession,
    listGeneration,
    drawerSessions,
  };
}
