/**
 * The work item a session is linked to (`#workqueue` 11/14): the queue (address) and item id whose
 * `session_id` is this session, found by reading the project's queues. `null` while unresolved, or
 * when no queue links the session — then the PR view shows no Inbox / Outbox.
 */
import { useEffect, useState } from "react";
import type { Client } from "@connectrpc/connect";
import type { WorkQueueAddress, WorkQueueService } from "tddy-client-web/gen/work_queue_pb";

export interface SessionWorkItem {
  queue: WorkQueueAddress;
  itemId: string;
}

interface QueueJsonNode {
  node_id?: string;
  session_id?: string | null;
  linked_sessions?: { session_id: string }[];
}

/** The node of `workQueueJson` linking `sessionId`, if any. */
export function itemLinkingSession(workQueueJson: string, sessionId: string): string | null {
  try {
    const nodes = (JSON.parse(workQueueJson) as { nodes?: QueueJsonNode[] }).nodes ?? [];
    return nodes.find(
        (node) =>
          node.session_id === sessionId || node.linked_sessions?.some((link) => link.session_id === sessionId),
      )?.node_id ?? null;
  } catch {
    return null;
  }
}

export function useSessionWorkItem(
  workQueueClient: Client<typeof WorkQueueService> | undefined,
  sessionToken: string,
  projectId: string,
  sessionId: string,
): SessionWorkItem | null {
  const [found, setFound] = useState<SessionWorkItem | null>(null);
  useEffect(() => {
    setFound(null);
    if (!workQueueClient?.listProjectWorkQueues || !workQueueClient.getWorkQueue) return;
    let current = true;
    (async () => {
      const listing = await workQueueClient.listProjectWorkQueues({ sessionToken, projectId });
      for (const summary of listing.stacks) {
        if (!summary.address) continue;
        const queue = await workQueueClient.getWorkQueue({ sessionToken, stack: summary.address });
        const itemId = itemLinkingSession(queue.workQueueJson, sessionId);
        if (itemId !== null) {
          if (current) setFound({ queue: summary.address, itemId });
          return;
        }
      }
    })().catch(() => undefined);
    return () => {
      current = false;
    };
  }, [workQueueClient, sessionToken, projectId, sessionId]);
  return found;
}
