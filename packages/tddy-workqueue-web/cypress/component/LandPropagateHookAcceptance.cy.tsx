/**
 * Acceptance (`#workqueue` 14/14): an item's Propagate control, wired through `useLandPropagate` —
 * a refused propagation shows why, and the control is offered again once the call answered.
 *
 * PRD: docs/ft/coder/1-WIP/PRD-2026-11-22-land-propagate.md
 */

import React from "react";
import type { Client } from "@connectrpc/connect";
import type { WorkQueueService } from "tddy-client-web/gen/work_queue_pb";
import { LandPropagateContext, PropagateControl, useLandPropagate } from "../../src/landPropagate";

const byTestId = (id: string) => cy.get(`[data-testid="${id}"]`, { timeout: 5000 });

const ADDRESS = { projectId: "proj-1", queueId: "owner", daemonInstanceId: "" };

/** A client whose `PropagateWorkItem` refuses: the local branch is diverged. */
function aClientRefusingPropagation(): Client<typeof WorkQueueService> {
  return {
    propagateWorkItem: cy.stub().as("propagate").resolves({
      propagated: false,
      refusalKind: "local_branch",
      refusalState: "Diverged",
      refusalActions: ["pull", "push"],
      refusalDetail: "local branch is Diverged; pull, then push first",
    }),
  } as unknown as Client<typeof WorkQueueService>;
}

function OneItem({ client }: { client: Client<typeof WorkQueueService> }) {
  const landPropagate = useLandPropagate(client, "token", ADDRESS, () => {});
  return (
    <LandPropagateContext.Provider value={landPropagate.context}>
      <PropagateControl itemId="n2" />
    </LandPropagateContext.Provider>
  );
}

it("a refused propagation names why and offers Propagate again", () => {
  // Given
  cy.mount(<OneItem client={aClientRefusingPropagation()} />);

  // When
  byTestId("work-item-propagate-n2").click();

  // Then
  cy.get("@propagate").should("have.been.calledOnce");
  byTestId("work-item-propagation-refusal-n2").should("contain.text", "Diverged").and("contain.text", "pull");
  byTestId("work-item-propagate-n2").should("not.be.disabled");
});
