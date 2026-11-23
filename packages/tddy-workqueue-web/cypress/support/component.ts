import React from "react";
import { mount } from "cypress/react";
import type { ReactElement, ReactNode } from "react";
import type { InMemoryRpcBackend } from "tddy-connectrpc-testkit";
import { type MountWithClientsOverrides, withClients } from "tddy-client-web/testkit";
import { StartPassphraseProvider } from "tddy-session-create-web";
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

/** The host application's passphrase dialog is not this package's: a start under test asks for none. */
function NoPassphraseDialog(): null {
  return null;
}

/** The three ports, plus the slot the Start-session dialog's host application fills. */
function aPrStackHost(jsx: ReactNode, backend: InMemoryRpcBackend, overrides?: MountWithClientsOverrides): ReactElement {
  return React.createElement(StartPassphraseProvider, { prompt: NoPassphraseDialog }, withClients(jsx as ReactElement, backend, overrides));
}

/** `strict: false` as in tddy-web, so a dev double-mount does not discard imperative refs mid-test. */
Cypress.Commands.add("mount", (jsx, options = {}) => mount(jsx, { strict: false, ...options }));
Cypress.Commands.add("mountWithClients", (jsx: ReactElement, backend: InMemoryRpcBackend, overrides?: MountWithClientsOverrides) =>
  mount(aPrStackHost(jsx, backend, overrides), { strict: false }),
);
