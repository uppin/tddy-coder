import { useCallback, useState } from "react";
import type { Client } from "@connectrpc/connect";
import type { SessionEntry, SessionService } from "../../gen/session_pb";
import { requestSessionsRefresh } from "../../lib/sessionsRefreshBridge";
import { owningHostForSession } from "../../utils/crossHostSessions";
import type { UseSessionAttachmentResult } from "./useSessionAttachment";

export interface UseSessionsDrawerDeleteSelectionArgs {
  sortedSessions: SessionEntry[];
  selectedInstanceId: string | null;
  clientForHost: (instanceId: string) => Client<typeof SessionService> | null;
  client: Client<typeof SessionService> | null;
  deleteSession: UseSessionAttachmentResult["deleteSession"];
  sessionToken: string;
}

/**
 * The sessions drawer's bulk-delete selection: the ticked set, selection mode, and the toggle,
 * select-all and bulk-delete actions over them.
 */
export function useSessionsDrawerDeleteSelection({
  sortedSessions,
  selectedInstanceId,
  clientForHost,
  client,
  deleteSession,
  sessionToken,
}: UseSessionsDrawerDeleteSelectionArgs) {
  // Sessions ticked for bulk delete — a Set preserves insertion (selection) order, which the bulk
  // delete replays so sessions are removed in the order they were selected.
  const [selectedForDelete, setSelectedForDelete] = useState<Set<string>>(() => new Set());
  // Bulk-selection mode: off by default so the drawer reads as a plain list. The bottom minibar
  // toggles it on, which is what reveals the per-row checkboxes and the Select-all / Delete actions.
  const [selectionMode, setSelectionMode] = useState(false);

  const toggleSelectForDelete = useCallback((sessionId: string) => {
    setSelectedForDelete((prev) => {
      const next = new Set(prev);
      if (next.has(sessionId)) next.delete(sessionId);
      else next.add(sessionId);
      return next;
    });
  }, []);

  const enterSelectionMode = useCallback(() => setSelectionMode(true), []);

  // Leaving selection mode always clears the tick set — a stale selection must not survive into the
  // next time the operator opens the bar.
  const exitSelectionMode = useCallback(() => {
    setSelectionMode(false);
    setSelectedForDelete(new Set());
  }, []);

  // Delete every selected session in selection (insertion) order, routing each delete to the
  // session's owning daemon. Sequential so the daemon processes them in the same order the operator
  // ticked the rows, and so a single failure doesn't abandon the remaining deletes silently.
  // Select-all toggles against the full visible list: if every session is already ticked, clear;
  // otherwise tick them all (fresh Set so insertion order matches the current list order).
  const toggleSelectAll = useCallback(() => {
    setSelectedForDelete((prev) => {
      const allIds = sortedSessions.map((s) => s.sessionId);
      const allSelected = allIds.length > 0 && allIds.every((id) => prev.has(id));
      return allSelected ? new Set() : new Set(allIds);
    });
  }, [sortedSessions]);

  const handleBulkDelete = useCallback(async () => {
    const ids = [...selectedForDelete];
    for (const id of ids) {
      const session = sortedSessions.find((s) => s.sessionId === id);
      const owner = session
        ? owningHostForSession(session, selectedInstanceId ?? "")
        : (selectedInstanceId ?? "");
      const targetClient = clientForHost(owner) ?? client;
      if (!targetClient) continue;
      try {
        await deleteSession(id, sessionToken, targetClient);
      } catch (err) {
        console.debug("[SessionsDrawerScreen] bulk deleteSession error", err);
      }
    }
    setSelectedForDelete(new Set());
    setSelectionMode(false);
    requestSessionsRefresh();
  }, [selectedForDelete, sortedSessions, selectedInstanceId, clientForHost, client, deleteSession, sessionToken]);

  return {
    selectedForDelete,
    selectionMode,
    toggleSelectForDelete,
    enterSelectionMode,
    exitSelectionMode,
    toggleSelectAll,
    handleBulkDelete,
  };
}
