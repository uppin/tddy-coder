import { useCallback, useEffect, useRef, useState } from "react";
import { Code, ConnectError, type Client } from "@connectrpc/connect";
import type { WorkQueueService, WorkQueueAddress } from "tddy-client-web/gen/work_queue_pb";
import { revisionOf, supersedes } from "./queueRevision";

type Address = Pick<WorkQueueAddress, "projectId" | "queueId" | "daemonInstanceId">;

/** Whether the shown queue is still being kept current. */
export type WorkQueueLiveness =
  /** The watch is open and has not delivered yet. */
  | { state: "connecting" }
  /** The watch has delivered and is open. */
  | { state: "live" }
  /** The watch dropped; it is reopened after a delay, and its first snapshot resyncs the queue. */
  | { state: "reconnecting"; reason: string }
  /** The watch ended for a reason another try would not change (the queue is gone, access is refused). */
  | { state: "ended"; reason: string };

export interface WorkQueueWatch {
  /** The newest copy of the queue seen; empty until the first arrives. */
  workQueueJson: string;
  /** The session that owns the queue, from the watch's summary. */
  createdBySessionId: string;
  /** Whether any copy of the queue has arrived — until then an empty `workQueueJson` is no queue at all. */
  loaded: boolean;
  liveness: WorkQueueLiveness;
  /** Offer a copy of the queue the view got some other way (a mutation's answer); kept only if newest. */
  apply: (workQueueJson: string) => void;
}

/** Refusals a reconnect would only repeat. */
const TERMINAL_CODES: ReadonlySet<Code> = new Set([
  Code.NotFound,
  Code.Unauthenticated,
  Code.PermissionDenied,
  Code.InvalidArgument,
  Code.FailedPrecondition,
  Code.Unimplemented,
]);

const FIRST_RETRY_MS = 1000;
const LONGEST_RETRY_MS = 30_000;

/**
 * The work queue at `address`, live: one `WatchWorkQueue` on `watchClient` — the host that holds the
 * queue — whose snapshots, like `apply`'s copies, replace the queue only when they are newer than the
 * one held. A dropped watch is reopened after 1 s, 2 s, 4 s … (at most 30 s), and the delay starts
 * over once a snapshot arrives; a terminal refusal ends it. A `null` client — the holding host is not
 * reachable from here — is an ended watch that says so.
 */
export function useWorkQueueWatch(
  watchClient: Client<typeof WorkQueueService> | null,
  sessionToken: string,
  address: Address,
): WorkQueueWatch {
  const [workQueueJson, setWorkQueueJson] = useState("");
  const [createdBySessionId, setCreatedBySessionId] = useState("");
  const [loaded, setLoaded] = useState(false);
  const [liveness, setLiveness] = useState<WorkQueueLiveness>({ state: "connecting" });
  // Read and written synchronously by every source, so two copies arriving together are ordered.
  const held = useRef<number | null>(null);

  const accept = useCallback((json: string): boolean => {
    const revision = revisionOf(json);
    if (!supersedes(held.current, revision)) return false;
    held.current = revision === 0 ? (held.current ?? 0) : revision;
    setWorkQueueJson(json);
    setLoaded(true);
    return true;
  }, []);

  useEffect(() => {
    held.current = null;
    setWorkQueueJson("");
    setCreatedBySessionId("");
    setLoaded(false);
    if (!watchClient) {
      setLiveness({
        state: "ended",
        reason: `the host holding this work queue (${address.daemonInstanceId || "this host"}) is not reachable`,
      });
      return;
    }
    setLiveness({ state: "connecting" });
    const abort = new AbortController();
    let attempt = 0;
    let retry: ReturnType<typeof setTimeout> | undefined;

    const reconnect = (reason: string) => {
      setLiveness({ state: "reconnecting", reason });
      const delay = Math.min(FIRST_RETRY_MS * 2 ** attempt, LONGEST_RETRY_MS);
      attempt += 1;
      retry = setTimeout(() => {
        if (!abort.signal.aborted) void watch();
      }, delay);
    };

    const watch = async () => {
      try {
        for await (const snapshot of watchClient.watchWorkQueue(
          { sessionToken, queue: address },
          { signal: abort.signal },
        )) {
          attempt = 0;
          if (accept(snapshot.workQueueJson)) {
            setCreatedBySessionId(snapshot.summary?.createdBySessionId ?? "");
          }
          setLiveness({ state: "live" });
        }
        if (abort.signal.aborted) return;
        reconnect("the watch ended");
      } catch (err) {
        if (abort.signal.aborted) return;
        const error = ConnectError.from(err);
        if (TERMINAL_CODES.has(error.code)) {
          setLiveness({ state: "ended", reason: error.rawMessage });
          return;
        }
        reconnect(error.rawMessage);
      }
    };

    void watch();
    return () => {
      abort.abort();
      if (retry !== undefined) clearTimeout(retry);
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [watchClient, sessionToken, address.projectId, address.queueId, address.daemonInstanceId, accept]);

  const apply = useCallback((json: string) => void accept(json), [accept]);

  return { workQueueJson, createdBySessionId, loaded, liveness, apply };
}
