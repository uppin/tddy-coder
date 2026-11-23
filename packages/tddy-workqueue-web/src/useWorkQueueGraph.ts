import { useEffect, useState } from "react";
import type { Client } from "@connectrpc/connect";
import type { ConnectorEdge } from "./WorkItemConnectors";
import type {
  WorkQueueService,
  WorkQueueAddress,
  WorkQueueGraphResponse,
} from "tddy-client-web/gen/work_queue_pb";

/**
 * A queue's lineage (`#workqueue` 9/14), read with `GetWorkQueueGraph`. `null` until the daemon
 * answers; a client without the call, or a failed read, stays `null` rather than guessing.
 * `refreshKey` changes whenever the queue is edited, so the graph is read again.
 */
export function useWorkQueueGraph(
  workQueueClient: Client<typeof WorkQueueService> | undefined,
  sessionToken: string,
  address: Pick<WorkQueueAddress, "projectId" | "queueId" | "daemonInstanceId">,
  refreshKey: string,
): WorkQueueGraphResponse | null {
  const [graph, setGraph] = useState<WorkQueueGraphResponse | null>(null);
  useEffect(() => {
    if (!workQueueClient?.getWorkQueueGraph) return;
    let current = true;
    workQueueClient
      .getWorkQueueGraph({ sessionToken, queue: address })
      .then((res) => current && setGraph(res))
      .catch(() => current && setGraph(null));
    return () => {
      current = false;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [workQueueClient, sessionToken, address.projectId, address.queueId, address.daemonInstanceId, refreshKey]);
  return graph;
}

/** The brackets a graph asks for: line edges between two tasks of the queue, and stored dependencies. */
export function connectorEdgesOf(graph: WorkQueueGraphResponse | null): ConnectorEdge[] {
  return [
    ...(graph?.lineEdges ?? [])
      .filter((edge) => edge.kind === "item")
      .map((edge): ConnectorEdge => ({ kind: "line", from: edge.endpoint, to: edge.itemId })),
    ...(graph?.dependencies ?? []).map((edge): ConnectorEdge => ({ kind: "dependency", from: edge.from, to: edge.to })),
  ];
}
