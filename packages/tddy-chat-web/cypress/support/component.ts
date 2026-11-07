import { mount } from "cypress/react";
import type { ReactElement } from "react";
import type { InMemoryRpcBackend } from "tddy-connectrpc-testkit";
import { type MountWithClientsOverrides, withClients } from "tddy-client-web/testkit";
import { agentActivityRegistry } from "../../src/agentActivityRegistry";

// No stylesheet is loaded on purpose: these specs pin the layout contract of components that carry
// their own inline layout (TRANSCRIPT_ROOT_STYLE, the overlay panel), as tddy-web's component
// support does. A global theme here would restyle the harnesses' own hosts and cover their controls.

/** The Agent Activity store is an app-lifetime module singleton; component tests share one JS context
 *  across cases, so clear its per-session state before each test to keep cases that reuse a
 *  `sessionId` isolated. */
beforeEach(() => {
  agentActivityRegistry.reset();
});

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
