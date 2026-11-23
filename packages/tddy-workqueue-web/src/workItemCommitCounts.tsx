/**
 * An item's commit counts for its queue row (`#workqueue` 11/14): how many commits it is missing
 * (inbox), how many it owes its successors (outbox), and how many of the inbox would conflict —
 * read with `GetWorkQueueCommitCounts`. A client without the call, a failed read, or an item the
 * daemon has no counts for shows nothing rather than a guess.
 *
 * Test id: `work-item-commit-counts-<item id>` (`data-inbox`, `data-outbox`, `data-conflicts`).
 */
import React, { createContext, useContext, useEffect, useState } from "react";
import type { Client } from "@connectrpc/connect";
import type { WorkQueueAddress, WorkQueueService } from "tddy-client-web/gen/work_queue_pb";

export interface ItemCommitCounts {
  inbox: number;
  outbox: number;
  conflicts: number;
}

/** Counts by item id; absent while unread. */
export const WorkItemCommitCountsContext = createContext<ReadonlyMap<string, ItemCommitCounts>>(new Map());

export function useWorkQueueCommitCounts(
  workQueueClient: Client<typeof WorkQueueService> | undefined,
  sessionToken: string | undefined,
  address: Pick<WorkQueueAddress, "projectId" | "queueId" | "daemonInstanceId">,
  refreshKey: string,
): ReadonlyMap<string, ItemCommitCounts> {
  const [counts, setCounts] = useState<ReadonlyMap<string, ItemCommitCounts>>(new Map());
  useEffect(() => {
    if (!workQueueClient?.getWorkQueueCommitCounts) return;
    let current = true;
    workQueueClient
      .getWorkQueueCommitCounts({ sessionToken, queue: address })
      .then((res) => {
        if (!current) return;
        setCounts(
          new Map(
            res.items.map((item) => [
              item.itemId,
              { inbox: item.inboxCount, outbox: item.outboxCount, conflicts: item.conflictCount },
            ]),
          ),
        );
      })
      .catch(() => current && setCounts(new Map()));
    return () => {
      current = false;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [workQueueClient, sessionToken, address.projectId, address.queueId, address.daemonInstanceId, refreshKey]);
  return counts;
}

export function WorkItemCommitCounts({ itemId }: { itemId: string }) {
  const counts = useContext(WorkItemCommitCountsContext).get(itemId);
  if (!counts || (counts.inbox === 0 && counts.outbox === 0)) return null;
  return (
    <span
      data-testid={`work-item-commit-counts-${itemId}`}
      data-inbox={counts.inbox}
      data-outbox={counts.outbox}
      data-conflicts={counts.conflicts}
      title={`${counts.inbox} commits missing, ${counts.outbox} owed to successors, ${counts.conflicts} would conflict`}
      className="flex-shrink-0 text-xs text-muted-foreground"
    >
      ↓{counts.inbox} ↑{counts.outbox}
      {counts.conflicts > 0 ? <strong className="ml-1 text-destructive">⚠{counts.conflicts}</strong> : null}
    </span>
  );
}
