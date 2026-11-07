/**
 * `mountWithRpc(jsx, backend)`: mount `component` into the drawer world with every host answering
 * from one in-memory `backend` — the ports counterpart of tddy-web's `RpcTransportProvider` mount.
 */
import type { ReactElement } from "react";
import type { InMemoryRpcBackend } from "tddy-connectrpc-testkit";
import { mountRouted } from "./drawerWorld";

export function mountWithRpc(component: ReactElement, backend: InMemoryRpcBackend): Cypress.Chainable {
  mountRouted(component, { httpBackend: backend });
  return cy.get("body");
}
