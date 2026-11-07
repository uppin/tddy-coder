import { mount } from "cypress/react";
import type { ReactElement } from "react";
import { anInMemoryRpcBackend, type InMemoryRpcBackend } from "tddy-connectrpc-testkit";
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

/**
 * `strict: false` as in tddy-web, so a dev double-mount does not discard imperative refs mid-test.
 *
 * A bare mount still stands inside the clients port (backed by an inert in-memory backend):
 * the terminal session always renders its upload affordances, which read a daemon client
 * through the port even when the spec never drives an upload.
 */
Cypress.Commands.add("mount", (jsx, options = {}) =>
  mount(withClients(jsx as ReactElement, anInMemoryRpcBackend()), { strict: false, ...options }),
);
Cypress.Commands.add("mountWithClients", (jsx: ReactElement, backend: InMemoryRpcBackend, overrides?: MountWithClientsOverrides) =>
  mount(withClients(jsx, backend, overrides), { strict: false }),
);
