/**
 * Page object for the stack view (`WorkQueueView`, `#orchestrators` 9/11) — one project stack opened
 * by its id. Its rows and controls are the Planned PRs panel's, so those are read through
 * `prStackScreenPage`; this object holds what is the view's own, and the assertions about which stack
 * a request named.
 */

import type { InMemoryRpcBackend } from "tddy-connectrpc-testkit";
import { WorkQueueService } from "tddy-client-web/gen/work_queue_pb";
import { byTestId, TEST_IDS } from "../testIds";

/** The address fields every request the view makes must carry. */
export interface ExpectedStackAddress {
  projectId: string;
  queueId: string;
  daemonInstanceId: string;
}

/** The fields of a request that address a stack: the address, and the legacy session id. */
interface AddressedRequest {
  sessionId?: string;
  stack?: { projectId: string; queueId: string; daemonInstanceId: string };
}

function expectAddressed(request: AddressedRequest, address: ExpectedStackAddress) {
  expect(request.stack, "the request names its stack by address").to.include(address);
  expect(request.sessionId ?? "", "and by no orchestrator session").to.equal("");
}

export const workQueueViewPage = {
  /** The view's root. */
  view: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.workQueueView, { timeout: 5000, ...options }),

  /** Why the queue could not be loaded, shown in place of the list. */
  loadError: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.workQueueLoadError, { timeout: 5000, ...options }),

  /** The notice that the shown queue is no longer live. */
  liveNotice: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.workQueueLiveNotice, { timeout: 5000, ...options }),

  /** The stack was read from the daemon by its address — `GetWorkQueue` named it. */
  expectStackReadAt(backend: InMemoryRpcBackend, address: ExpectedStackAddress) {
    cy.wrap(backend).should((b) => {
      const calls = b.callsTo(WorkQueueService.method.getWorkQueue);
      expect(calls, "GetWorkQueue was asked").to.have.length.at.least(1);
      expect(calls[0].stack, "GetWorkQueue names the stack").to.include(address);
    });
  },

  /** Exactly one reorder was sent, moving `itemId` `direction`, addressed at the stack. */
  expectReorderSentTo(
    backend: InMemoryRpcBackend,
    address: ExpectedStackAddress,
    move: { itemId: string; direction: "up" | "down" },
  ) {
    cy.wrap(backend).should((b) => {
      const calls = b.callsTo(WorkQueueService.method.reorderWorkItem);
      expect(calls).to.have.length(1);
      expectAddressed(calls[0], address);
      expect(calls[0].itemId).to.equal(move.itemId);
      expect(calls[0].direction).to.equal(move.direction);
    });
  },

  /** Exactly one planned PR titled `title` was added, addressed at the stack. */
  expectPlannedPrAddedTo(backend: InMemoryRpcBackend, address: ExpectedStackAddress, title: string) {
    cy.wrap(backend).should((b) => {
      const calls = b.callsTo(WorkQueueService.method.addWorkItem);
      expect(calls).to.have.length(1);
      expectAddressed(calls[0], address);
      expect(calls[0].title).to.equal(title);
    });
  },
};
