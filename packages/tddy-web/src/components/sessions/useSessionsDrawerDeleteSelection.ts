import { useCallback, useState } from "react";
import type { Client } from "@connectrpc/connect";
import type { SessionEntry, SessionService } from "../../gen/session_pb";
import { requestSessionsRefresh } from "../../lib/sessionsRefreshBridge";
import { hostNotConnected, rpcFailureText } from "../../rpc/rpcFailure";
import { owningHostForSession } from "../../utils/crossHostSessions";
import { sessionDrawerLabel } from "../../utils/sessionDrawerLabel";

export interface UseSessionsDrawerDeleteSelectionArgs {
  sortedSessions: SessionEntry[];
  selectedInstanceId: string | null;
  clientForHost: (instanceId: string) => Client<typeof SessionService> | null;
  client: Client<typeof SessionService> | null;
  sessionToken: string;
  /** A daemon instance id's display label — names the host a row could not be deleted on. */
  hostLabelForInstance: (instanceId: string) => string;
  /** Told of each session the bulk delete removed, so the screen can drop an attachment to it. */
  onSessionDeleted?: (sessionId: string) => void;
}

/** One row a bulk delete left behind, and why. */
export interface BulkDeleteFailure {
  readonly sessionId: string;
  /** The row's drawer label, so the failure names the session the operator ticked. */
  readonly label: string;
  readonly reason: string;
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
  sessionToken,
  hostLabelForInstance,
  onSessionDeleted,
}: UseSessionsDrawerDeleteSelectionArgs) {
  // Sessions ticked for bulk delete — a Set preserves insertion (selection) order, which the bulk
  // delete replays so sessions are removed in the order they were selected.
  const [selectedForDelete, setSelectedForDelete] = useState<Set<string>>(() => new Set());
  // Bulk-selection mode: off by default so the drawer reads as a plain list. The bottom minibar
  // toggles it on, which is what reveals the per-row checkboxes and the Select-all / Delete actions.
  const [selectionMode, setSelectionMode] = useState(false);
  // The rows the last bulk delete could not remove, each with its own reason. Held here — not in
  // the selected session's attachment, where they used to land — because they are about the rows
  // that were ticked, not about whichever session the pane happens to show.
  const [bulkDeleteFailures, setBulkDeleteFailures] = useState<readonly BulkDeleteFailure[]>([]);

  const toggleSelectForDelete = useCallback((sessionId: string) => {
    setSelectedForDelete((prev) => {
      const next = new Set(prev);
      if (next.has(sessionId)) next.delete(sessionId);
      else next.add(sessionId);
      return next;
    });
  }, []);

  const enterSelectionMode = useCallback(() => {
    setSelectionMode(true);
    setBulkDeleteFailures([]);
  }, []);

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
    const failures: BulkDeleteFailure[] = [];
    setBulkDeleteFailures([]);
    for (const id of ids) {
      const session = sortedSessions.find((s) => s.sessionId === id);
      const owner = session
        ? owningHostForSession(session, selectedInstanceId ?? "")
        : (selectedInstanceId ?? "");
      const label = session ? sessionDrawerLabel(session) : id;
      try {
        // A row whose host no wire reaches is a failure of that row, said with the host named —
        // never skipped, which used to leave it in the list with nothing to say why.
        const targetClient = clientForHost(owner) ?? client;
        if (!targetClient) throw hostNotConnected(hostLabelForInstance(owner));
        await targetClient.deleteSession({ sessionToken, sessionId: id });
        onSessionDeleted?.(id);
      } catch (err) {
        failures.push({ sessionId: id, label, reason: rpcFailureText(err) });
      }
    }
    setBulkDeleteFailures(failures);
    setSelectedForDelete(new Set());
    setSelectionMode(false);
    requestSessionsRefresh();
  }, [
    selectedForDelete,
    sortedSessions,
    selectedInstanceId,
    clientForHost,
    client,
    sessionToken,
    hostLabelForInstance,
    onSessionDeleted,
  ]);

  return {
    selectedForDelete,
    selectionMode,
    toggleSelectForDelete,
    enterSelectionMode,
    exitSelectionMode,
    toggleSelectAll,
    handleBulkDelete,
    bulkDeleteFailures,
  };
}
