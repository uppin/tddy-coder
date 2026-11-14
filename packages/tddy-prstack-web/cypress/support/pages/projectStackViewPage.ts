/**
 * Page object for the stack view (`ProjectStackView`, `#orchestrators` 9/11) — one project stack opened
 * by its id. Its rows and controls are the Planned PRs panel's, so those are read through
 * `prStackScreenPage`; this object holds what is the view's own, and the assertions about which stack
 * a request named.
 */

import type { InMemoryRpcBackend } from "tddy-connectrpc-testkit";
import { PrStackService } from "tddy-client-web/gen/pr_stack_pb";
import { byTestId, TEST_IDS } from "../testIds";

/** The address fields every request the view makes must carry. */
export interface ExpectedStackAddress {
  projectId: string;
  stackId: string;
  daemonInstanceId: string;
}

/** The fields of a request that address a stack: the address, and the legacy session id. */
interface AddressedRequest {
  sessionId?: string;
  stack?: { projectId: string; stackId: string; daemonInstanceId: string };
}

function expectAddressed(request: AddressedRequest, address: ExpectedStackAddress) {
  expect(request.stack, "the request names its stack by address").to.include(address);
  expect(request.sessionId ?? "", "and by no orchestrator session").to.equal("");
}

export const projectStackViewPage = {
  /** The view's root. */
  view: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.projectStackView, { timeout: 5000, ...options }),

  /** The stack was read from the daemon by its address — `GetProjectStack` named it. */
  expectStackReadAt(backend: InMemoryRpcBackend, address: ExpectedStackAddress) {
    cy.wrap(backend).should((b) => {
      const calls = b.callsTo(PrStackService.method.getProjectStack);
      expect(calls, "GetProjectStack was asked").to.have.length.at.least(1);
      expect(calls[0].stack, "GetProjectStack names the stack").to.include(address);
    });
  },

  /** Exactly one reorder was sent, moving `nodeId` `direction`, addressed at the stack. */
  expectReorderSentTo(
    backend: InMemoryRpcBackend,
    address: ExpectedStackAddress,
    move: { nodeId: string; direction: "up" | "down" },
  ) {
    cy.wrap(backend).should((b) => {
      const calls = b.callsTo(PrStackService.method.reorderPlannedPr);
      expect(calls).to.have.length(1);
      expectAddressed(calls[0], address);
      expect(calls[0].nodeId).to.equal(move.nodeId);
      expect(calls[0].direction).to.equal(move.direction);
    });
  },

  /** Exactly one planned PR titled `title` was added, addressed at the stack. */
  expectPlannedPrAddedTo(backend: InMemoryRpcBackend, address: ExpectedStackAddress, title: string) {
    cy.wrap(backend).should((b) => {
      const calls = b.callsTo(PrStackService.method.addPlannedPr);
      expect(calls).to.have.length(1);
      expectAddressed(calls[0], address);
      expect(calls[0].title).to.equal(title);
    });
  },
};
