import React, { useEffect, useState } from "react";
import type { Client } from "@connectrpc/connect";
import type { WorkQueueService, WorkQueueAddress, WorkQueueGraphResponse } from "tddy-client-web/gen/work_queue_pb";
import { WorkQueueGraph, type GraphLineEndpoint } from "./WorkQueueGraph";

export interface WorkQueueGraphPanelProps {
  workQueueClient?: Client<typeof WorkQueueService>;
  sessionToken?: string;
  address: Pick<WorkQueueAddress, "projectId" | "queueId" | "daemonInstanceId">;
  items: { itemId: string; title: string }[];
  /** Changes whenever the queue is edited, so the graph is read again. */
  refreshKey: string;
}

function endpointOf(kind: string, endpoint: string): GraphLineEndpoint {
  if (kind === "item") return { kind: "item", itemId: endpoint };
  return kind === "trunk" ? { kind: "trunk", branch: endpoint } : { kind: "external", branch: endpoint };
}

/**
 * A queue's lineage (`#workqueue` 9/14), read with `GetWorkQueueGraph` and drawn by [`WorkQueueGraph`].
 * Nothing is drawn until the daemon answers, and a client without the call (or a failed read) shows
 * nothing rather than a guess.
 */
export function WorkQueueGraphPanel({ workQueueClient, sessionToken, address, items, refreshKey }: WorkQueueGraphPanelProps) {
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

  if (!graph || (graph.lineEdges.length === 0 && graph.dependencies.length === 0)) return null;
  return (
    <div data-testid="work-queue-graph-panel" className="flex-shrink-0 p-3 border-t">
      {graph.unavailableReason && (
        <p data-testid="work-queue-graph-unavailable" className="text-xs text-muted-foreground mb-1">
          Pull request bases could not be read ({graph.unavailableReason}); showing planned bases.
        </p>
      )}
      <WorkQueueGraph
        items={items}
        lineEdges={graph.lineEdges.map((edge) => ({ itemId: edge.itemId, from: endpointOf(edge.kind, edge.endpoint) }))}
        dependencyEdges={graph.dependencies.map((edge) => ({ from: edge.from, to: edge.to }))}
      />
    </div>
  );
}
