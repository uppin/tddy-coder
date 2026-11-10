/**
 * Cypress adapter for the in-memory ConnectRPC testkit.
 *
 * Provides `mountWithRpc(jsx, backend)` — mounts a component with the clients port
 * (`tddy-client-web/testkit`) backed by an in-memory `InMemoryRpcBackend`, so HTTP,
 * daemon and host calls all flow through the same double.
 */

import React from "react";
import type { InMemoryRpcBackend } from "tddy-connectrpc-testkit";
import { withClients } from "tddy-client-web/testkit";

/** Mount `component` with the clients port backed by `backend`. */
export function mountWithRpc(
  component: React.ReactElement,
  backend: InMemoryRpcBackend,
): Cypress.Chainable {
  return cy.mount(withClients(component, backend));
}
