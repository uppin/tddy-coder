import { mount } from "cypress/react";
import type { ReactElement } from "react";
import type { InMemoryRpcBackend } from "tddy-connectrpc-testkit";
import { type MountWithClientsOverrides, withClients } from "tddy-client-web/testkit";
import "tddy-ui-web/theme.css";

declare global {
  // eslint-disable-next-line @typescript-eslint/no-namespace
  namespace Cypress {
    interface Chainable {
      mount: typeof mount;
      mountWithClients(jsx: ReactElement, backend: InMemoryRpcBackend, overrides?: MountWithClientsOverrides): Chainable;
    }
  }
}

/** `strict: false` as in tddy-web, so a dev double-mount does not discard imperative refs mid-test. */
Cypress.Commands.add("mount", (jsx, options = {}) => mount(jsx, { strict: false, ...options }));
Cypress.Commands.add("mountWithClients", (jsx: ReactElement, backend: InMemoryRpcBackend, overrides?: MountWithClientsOverrides) =>
  mount(withClients(jsx, backend, overrides), { strict: false }),
);
