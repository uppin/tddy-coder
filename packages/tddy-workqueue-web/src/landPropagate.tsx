/**
 * Propagate and Land wave, wired to the daemon (`#workqueue` 14/14): `PropagateWorkItem` and
 * `LandWorkQueue`. A refused propagation is an answer, shown with the state and the actions that fix
 * it; a client without the calls offers neither control.
 *
 * Test ids: `work-item-propagate-<item id>`, `work-item-propagation-error-<item id>` (a refusal that is
 * not about the local branch), plus those of {@link LandWaves} and {@link PropagationRefusal}.
 */
import React, { createContext, useContext, useState } from "react";
import type { Client } from "@connectrpc/connect";
import type { WorkQueueAddress, WorkQueueService } from "tddy-client-web/gen/work_queue_pb";
import { Button } from "tddy-ui-web";
import { LandWaves, PropagationRefusal, type LandResultView } from "./LandAndPropagate";

export interface PropagationRefused {
  kind: string;
  state: string;
  actions: string[];
  detail: string;
}

export interface LandPropagate {
  propagate: (itemId: string) => void;
  refusals: Readonly<Record<string, PropagationRefused>>;
  propagating: ReadonlySet<string>;
}

export const LandPropagateContext = createContext<LandPropagate | null>(null);

const failureOf = (err: unknown) => (err instanceof Error ? err.message : String(err));

/** The queue's land and propagate state and the calls that change it; `null` without the calls. */
export function useLandPropagate(
  workQueueClient: Client<typeof WorkQueueService> | undefined,
  sessionToken: string | undefined,
  address: Pick<WorkQueueAddress, "projectId" | "queueId" | "daemonInstanceId">,
  onChanged: () => void,
) {
  const [refusals, setRefusals] = useState<Record<string, PropagationRefused>>({});
  const [propagating, setPropagating] = useState<ReadonlySet<string>>(new Set());
  const [results, setResults] = useState<LandResultView[]>([]);
  const canPropagate = Boolean(workQueueClient?.propagateWorkItem);
  const canLand = Boolean(workQueueClient?.landWorkQueue);

  const propagate = async (itemId: string) => {
    if (!workQueueClient?.propagateWorkItem) return;
    setRefusals(({ [itemId]: _gone, ...rest }) => rest);
    setPropagating((prev) => new Set(prev).add(itemId));
    try {
      const res = await workQueueClient.propagateWorkItem({ sessionToken, queue: address, itemId, force: false });
      if (!res.propagated) {
        setRefusals((prev) => ({
          ...prev,
          [itemId]: {
            kind: res.refusalKind,
            state: res.refusalState,
            actions: res.refusalActions,
            detail: res.refusalDetail,
          },
        }));
      }
      onChanged();
    } catch (err) {
      setRefusals((prev) => ({
        ...prev,
        [itemId]: { kind: "failed", state: "", actions: [], detail: failureOf(err) },
      }));
    } finally {
      setPropagating((prev) => {
        const next = new Set(prev);
        next.delete(itemId);
        return next;
      });
    }
  };

  const land = async (fromWave: number, toWave: number) => {
    if (!workQueueClient?.landWorkQueue) return;
    try {
      const res = await workQueueClient.landWorkQueue({ sessionToken, queue: address, fromWave, toWave });
      setResults(
        res.steps.map((step) => ({
          itemId: step.itemId,
          outcome: step.outcome as LandResultView["outcome"],
          detail: step.detail,
        })),
      );
      onChanged();
    } catch (err) {
      setResults([{ itemId: "queue", outcome: "stopped", detail: failureOf(err) }]);
    }
  };

  return {
    canPropagate,
    canLand,
    results,
    land,
    context: { propagate, refusals, propagating } as LandPropagate,
  };
}

/** An item's Propagate control and what its last propagation was refused for. */
export function PropagateControl({ itemId }: { itemId: string }) {
  const landPropagate = useContext(LandPropagateContext);
  if (!landPropagate) return null;
  const refusal = landPropagate.refusals[itemId];
  return (
    <>
      <Button
        data-testid={`work-item-propagate-${itemId}`}
        size="sm"
        variant="outline"
        disabled={landPropagate.propagating.has(itemId)}
        onClick={() => landPropagate.propagate(itemId)}
      >
        Propagate
      </Button>
      {refusal && refusal.kind === "local_branch" && (
        <PropagationRefusal itemId={itemId} state={refusal.state} actions={refusal.actions} />
      )}
      {refusal && refusal.kind !== "local_branch" && (
        <p data-testid={`work-item-propagation-error-${itemId}`} role="alert" className="text-xs text-destructive">
          {refusal.detail}
        </p>
      )}
    </>
  );
}

export { LandWaves };
