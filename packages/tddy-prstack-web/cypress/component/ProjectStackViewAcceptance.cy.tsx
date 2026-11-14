/**
 * Acceptance: the stack view (`ProjectStackView`, `#orchestrators` 9/11) shows one of a project's PR
 * stacks, opened by its address with no orchestrator session behind it — today's Planned PRs panel at
 * full width — and every control it offers acts on that stack by its address.
 *
 * Mounted on its own through the package's entry point, over one in-memory backend: the assertions
 * read the typed requests the view sent.
 *
 * PRD: docs/ft/web/1-WIP/PRD-2026-11-10-stack-web-by-id.md
 * Changeset: docs/dev/1-WIP/2026-11-10-stack-web-by-id.md
 */

import React from "react";
import { createClient } from "@connectrpc/connect";
import { create } from "@bufbuild/protobuf";
import { anInMemoryRpcBackend, type InMemoryRpcBackend } from "tddy-connectrpc-testkit";
import { PrStackService } from "tddy-client-web/gen/pr_stack_pb";
import { ProjectStackView } from "../../src";
import { prStackScreenPage } from "../support/pages/prStackScreenPage";
import { projectStackViewPage, type ExpectedStackAddress } from "../support/pages/projectStackViewPage";
import { aPlannedNode, aStackPlanJson, type StackNodeFixture } from "../support/rpc/prStackFixtures";

/** A stack held by another host than the one the view's clients reach. */
const THE_STACK: ExpectedStackAddress = {
  projectId: "proj-auth",
  stackId: "0190b000-0000-7000-8000-0000000000aa",
  daemonInstanceId: "host-b",
};

const TOKEN_STORE = aPlannedNode({ nodeId: "n1", title: "Add token store", displayOrder: 0 });
const MIDDLEWARE = aPlannedNode({ nodeId: "n2", title: "Add auth middleware", displayOrder: 1 });
const LOGOUT = aPlannedNode({ nodeId: "n3", title: "Add logout", displayOrder: 2 });

function aPlan(nodes: StackNodeFixture[]): string {
  return aStackPlanJson(1, nodes);
}

/**
 * The view over a daemon holding `THE_STACK` as n1, n2. A reorder answers with n2 first; an added
 * planned PR answers with n3 appended.
 */
function theStackView(): InMemoryRpcBackend {
  const backend = anInMemoryRpcBackend()
    .onUnary(PrStackService.method.getProjectStack, () =>
      create(PrStackService.method.getProjectStack.output, {
        stackPlanJson: aPlan([TOKEN_STORE, MIDDLEWARE]),
      }),
    )
    .onUnary(PrStackService.method.reorderPlannedPr, () =>
      create(PrStackService.method.reorderPlannedPr.output, {
        stackPlanJson: aPlan([
          { ...MIDDLEWARE, displayOrder: 0 },
          { ...TOKEN_STORE, displayOrder: 1 },
        ]),
      }),
    )
    .onUnary(PrStackService.method.addPlannedPr, () =>
      create(PrStackService.method.addPlannedPr.output, {
        stackPlanJson: aPlan([TOKEN_STORE, MIDDLEWARE, LOGOUT]),
        nodeId: "n3",
      }),
    );
  cy.mountWithClients(
    <ProjectStackView
      address={THE_STACK}
      prStackClient={createClient(PrStackService, backend.transport())}
      sessionToken="fake-token"
    />,
    backend,
  );
  return backend;
}

beforeEach(() => {
  cy.viewport(1280, 800);
});

it("the stack view lists the stack's planned prs and its controls act on that stack", () => {
  // Given
  const backend = theStackView();
  projectStackViewPage.view().should("exist");
  prStackScreenPage.plannedPrRowNodeIds().should("deep.equal", ["n1", "n2"]);
  projectStackViewPage.expectStackReadAt(backend, THE_STACK);

  // When — a row is moved, and a planned PR is added
  prStackScreenPage.expandRow("n2");
  prStackScreenPage.clickMoveUp("n2");
  prStackScreenPage.openAddPlannedPrForm();
  prStackScreenPage.fillAndSubmitAddPlannedPrForm({ title: "Add logout" });

  // Then — both went to the stack by its address, and the list shows what the daemon answered
  projectStackViewPage.expectReorderSentTo(backend, THE_STACK, { nodeId: "n2", direction: "up" });
  projectStackViewPage.expectPlannedPrAddedTo(backend, THE_STACK, "Add logout");
  prStackScreenPage.plannedPrRow("n3").should("contain.text", "Add logout");
});
