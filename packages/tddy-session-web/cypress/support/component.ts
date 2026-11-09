import { mount } from "cypress/react";
import { createElement, type ReactElement } from "react";
import { anInMemoryRpcBackend, type InMemoryRpcBackend } from "tddy-connectrpc-testkit";
import { type MountWithClientsOverrides, withClients } from "tddy-client-web/testkit";
import { UploadProgressProvider } from "tddy-terminal-web";
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
 * Stands the subtree where the app shell puts it: the terminal session always renders its upload
 * affordances, which need the `UploadProgressProvider` and read a daemon client through the clients
 * port even when a spec never drives an upload.
 */
function inShell(jsx: ReactElement, backend: InMemoryRpcBackend, overrides?: MountWithClientsOverrides): ReactElement {
  return withClients(createElement(UploadProgressProvider, null, jsx), backend, overrides);
}

/** `strict: false` as in tddy-web, so a dev double-mount does not discard imperative refs mid-test. */
Cypress.Commands.add("mount", (jsx, options = {}) =>
  mount(inShell(jsx as ReactElement, anInMemoryRpcBackend()), { strict: false, ...options }),
);
Cypress.Commands.add("mountWithClients", (jsx: ReactElement, backend: InMemoryRpcBackend, overrides?: MountWithClientsOverrides) =>
  mount(inShell(jsx, backend, overrides), { strict: false }),
);
