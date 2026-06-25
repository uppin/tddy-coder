import { mount } from "cypress/react";
import { mountWithRpc } from "./rpc/inMemory.tsx";

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
