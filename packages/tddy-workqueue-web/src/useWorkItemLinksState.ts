import { useEffect, useState } from "react";
import type { Client } from "@connectrpc/connect";
import type { SessionEntry } from "tddy-client-web/gen/session_pb";
import type { WorkQueueService, WorkQueueAddress } from "tddy-client-web/gen/work_queue_pb";
import type { ItemChecks, LinkableSession, WorkItemLinks } from "./workItemLinks";

type Address = Pick<WorkQueueAddress, "projectId" | "queueId" | "daemonInstanceId">;

export interface UseWorkItemLinksStateArgs {
  workQueueClient?: Client<typeof WorkQueueService>;
  sessionToken: string;
  address: Address;
  sessions: SessionEntry[];
  /** Applies the queue a link call answers with — every link call answers the queue after the change. */
  onWorkQueueJson: (workQueueJson: string) => void;
}

function failureReason(err: unknown): string {
  return err instanceof Error ? err.message : String(err);
}

function withoutItem(errors: Record<string, string>, itemId: string): Record<string, string> {
  if (!(itemId in errors)) return errors;
  const next = { ...errors };
  delete next[itemId];
  return next;
}

/**
 * The queue view's [`WorkItemLinks`](./workItemLinks) (`#workqueue` 6/14): the four link calls with
 * their per-item refusals, and each PR'ed item's live check counts from `StreamWorkQueueStatus` — the
 * daemon pushes a snapshot, then one per GitHub inbox change; the browser never polls.
 * `null` without a client, so the rows offer no link controls.
 */
export function useWorkItemLinksState({
  workQueueClient,
  sessionToken,
  address,
  sessions,
  onWorkQueueJson,
}: UseWorkItemLinksStateArgs): WorkItemLinks | null {
  const [errorByItemId, setErrorByItemId] = useState<Record<string, string>>({});
  const [checksByItemId, setChecksByItemId] = useState<Readonly<Record<string, ItemChecks>>>({});

  useEffect(() => {
    if (!workQueueClient?.streamWorkQueueStatus) return;
    const abort = new AbortController();
    (async () => {
      try {
        for await (const view of workQueueClient.streamWorkQueueStatus(
          { sessionToken, queue: address },
          { signal: abort.signal },
        )) {
          const next: Record<string, ItemChecks> = {};
          for (const item of view.items) {
            if (item.hasChecks) {
              next[item.itemId] = {
                running: item.checksRunning,
                failed: item.checksFailed,
                succeeded: item.checksSucceeded,
                runs: item.checks,
              };
            }
          }
          setChecksByItemId(next);
        }
      } catch {
        // Keep the counts shown; a dropped stream must not blank them.
      }
    })();
    return () => abort.abort();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [workQueueClient, sessionToken, address.projectId, address.queueId, address.daemonInstanceId]);

  if (!workQueueClient) return null;

  const runLinkCall = async (itemId: string, call: () => Promise<{ workQueueJson: string }>) => {
    setErrorByItemId((prev) => withoutItem(prev, itemId));
    try {
      const res = await call();
      onWorkQueueJson(res.workQueueJson);
    } catch (err) {
      setErrorByItemId((errors) => ({ ...errors, [itemId]: failureReason(err) }));
    }
  };

  return {
    sessions: sessions.map(
      (entry): LinkableSession => ({
        sessionId: entry.sessionId,
        label: entry.displayName || entry.sessionId,
        daemonInstanceId: entry.daemonInstanceId,
      }),
    ),
    checksByItemId,
    errorByItemId,
    onAssignPr: (itemId, pullNumber) =>
      void runLinkCall(itemId, () =>
        workQueueClient.assignWorkItemPr({
          sessionToken,
          queue: address,
          itemId,
          pullNumber: BigInt(pullNumber),
        }),
      ),
    onUnassignPr: (itemId) =>
      void runLinkCall(itemId, () => workQueueClient.unassignWorkItemPr({ sessionToken, queue: address, itemId })),
    onLinkSession: (itemId, session) =>
      void runLinkCall(itemId, () =>
        workQueueClient.linkWorkItemSession({
          sessionToken,
          queue: address,
          itemId,
          sessionId: session.sessionId,
          daemonInstanceId: session.daemonInstanceId,
        }),
      ),
    onUnlinkSession: (itemId, sessionId) =>
      void runLinkCall(itemId, () =>
        workQueueClient.unlinkWorkItemSession({ sessionToken, queue: address, itemId, sessionId }),
      ),
    onSetLabels: (itemId, labels) =>
      void runLinkCall(itemId, () =>
        workQueueClient.setWorkItemLabels({ sessionToken, queue: address, itemId, labels }),
      ),
    onClearBadge: (itemId, key) =>
      void runLinkCall(itemId, () =>
        workQueueClient.clearWorkItemBadge({ sessionToken, queue: address, itemId, key }),
      ),
  };
}
