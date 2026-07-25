import { mount } from "cypress/react";
import { mountWithRpc } from "./rpc/inMemory.tsx";
import { agentActivityRegistry } from "../../src/components/sessions/agentActivityRegistry";

/** The Agent Activity store is an app-lifetime module singleton; component tests share one JS
 *  context across cases, so clear its per-session cache before each test to keep cases that reuse a
 *  `sessionId` isolated. */
beforeEach(() => {
  agentActivityRegistry.reset();
});

/** Default `strict: false` so React 18 dev double-mount does not discard imperative refs / font state mid-test. */
Cypress.Commands.add("mount", (jsx, options = {}) => {
  return mount(jsx, { strict: false, ...options });
});

/**
 * Mount a component with all RPC (HTTP + LiveKit) routed to an in-memory
 * `InMemoryRpcBackend`. Use this instead of `cy.intercept` when the test
 * cares about behaviour rather than wire format.
 */
Cypress.Commands.add("mountWithRpc", mountWithRpc);
