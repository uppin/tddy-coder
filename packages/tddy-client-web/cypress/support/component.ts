import { mount } from "cypress/react";
import type { ReactElement } from "react";
import type { InMemoryRpcBackend } from "tddy-connectrpc-testkit";
import { type MountWithClientsOverrides, withClients } from "../../src/testkit";

declare global {
  // eslint-disable-next-line @typescript-eslint/no-namespace
  namespace Cypress {
    interface Chainable {
      mount: typeof mount;
      mountWithClients(jsx: ReactElement, backend: InMemoryRpcBackend, overrides?: MountWithClientsOverrides): Chainable;
    }
  }
}

Cypress.Commands.add("mount", (jsx, options = {}) => mount(jsx, { strict: false, ...options }));
Cypress.Commands.add("mountWithClients", (jsx: ReactElement, backend: InMemoryRpcBackend, overrides?: MountWithClientsOverrides) =>
  mount(withClients(jsx, backend, overrides), { strict: false }),
);
