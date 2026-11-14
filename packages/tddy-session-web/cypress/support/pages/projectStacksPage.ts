/**
 * Page object for a project's PR stacks in the drawer (`#orchestrators` 9/11): the **Stacks** row
 * under a project's header, the stack view an entry opens, and which stack the daemon double was
 * asked about.
 *
 * All raw selectors and wire inspection live here; test bodies call named methods.
 *
 * PRD: docs/ft/web/1-WIP/PRD-2026-11-10-stack-web-by-id.md
 */

import type { InMemoryRpcBackend } from "tddy-connectrpc-testkit";
import { PrStackService } from "tddy-client-web/gen/pr_stack_pb";
import {
  TEST_IDS,
  byTestId,
  sessionsDrawerProjectGroup,
  sessionsDrawerProjectStack,
  sessionsDrawerProjectStacks,
} from "../testIds";

/** How long a lookup waits for something the screen renders after an RPC answers. */
const RENDER_WAIT = { timeout: 5000 };

/** `#/…` with nothing after the path but an optional query. */
const hashIs = (path: string) => new RegExp(`^#${path}(\\?|$)`);

/** A daemon double recording the unary calls it answered. */
type RecordingBackend = Pick<InMemoryRpcBackend, "callsTo">;

/** The address fields a request about one stack must carry. */
export interface StackAddressFields {
  projectId: string;
  stackId: string;
  daemonInstanceId?: string;
}

export const projectStacksPage = {
  // ---------------------------------------------------------------------------
  // Drawer
  // ---------------------------------------------------------------------------

  /** The Stacks row of `projectId`, inside that project's group. */
  stacksRow: (projectId: string) =>
    byTestId(sessionsDrawerProjectGroup(projectId), RENDER_WAIT).find(
      `[data-testid='${sessionsDrawerProjectStacks(projectId)}']`,
    ),

  /** One stack's entry in a Stacks row. */
  stackEntry: (stackId: string) => byTestId(sessionsDrawerProjectStack(stackId), RENDER_WAIT),

  openStack(stackId: string) {
    projectStacksPage.stackEntry(stackId).click();
  },

  // ---------------------------------------------------------------------------
  // The stack view
  // ---------------------------------------------------------------------------

  stackView: () => byTestId(TEST_IDS.projectStackView, RENDER_WAIT),

  assertAtStackView(projectId: string, stackId: string) {
    cy.location("hash").should("match", hashIs(`/sessions/project/${projectId}/stack/${stackId}`));
  },

  // ---------------------------------------------------------------------------
  // What the daemon was asked
  // ---------------------------------------------------------------------------

  /** The project's stacks were listed once or more, for `projectId`. */
  assertStacksListedFor(backend: RecordingBackend, projectId: string) {
    cy.wrap(backend).should((b) => {
      const calls = b.callsTo(PrStackService.method.listProjectStacks);
      expect(calls, "ListProjectStacks was asked").to.have.length.at.least(1);
      expect(calls.map((c) => c.projectId)).to.include(projectId);
    });
  },

  /** The stack view read its stack by the address the listing gave it. */
  assertStackReadAt(backend: RecordingBackend, address: StackAddressFields) {
    cy.wrap(backend).should((b) => {
      const calls = b.callsTo(PrStackService.method.getProjectStack);
      expect(calls, "GetProjectStack was asked").to.have.length.at.least(1);
      expect(calls[calls.length - 1].stack).to.include(address);
    });
  },

  /** Exactly one planned PR titled `title` was added, naming the stack by its address. */
  assertPlannedPrAddedTo(backend: RecordingBackend, address: StackAddressFields, title: string) {
    cy.wrap(backend).should((b) => {
      const calls = b.callsTo(PrStackService.method.addPlannedPr);
      expect(calls).to.have.length(1);
      expect(calls[0].stack, "the add names its stack by address").to.include(address);
      expect(calls[0].title).to.equal(title);
    });
  },
};
