import { mount } from "cypress/react";
import { createElement, type ReactElement } from "react";
import { MediaPortProvider } from "tddy-client-web";
import type { InMemoryRpcBackend } from "tddy-connectrpc-testkit";
import { aMediaDouble, type MountWithClientsOverrides, withClients } from "tddy-client-web/testkit";
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
/** A media port with no camera video anywhere; a spec that cares mounts its own provider inside (`cameraRoster`). */
const noMedia = aMediaDouble();

Cypress.Commands.add("mount", (jsx, options = {}) =>
  mount(createElement(MediaPortProvider, { port: noMedia }, jsx), { strict: false, ...options }),
);
Cypress.Commands.add("mountWithClients", (jsx: ReactElement, backend: InMemoryRpcBackend, overrides?: MountWithClientsOverrides) =>
  mount(withClients(jsx, backend, overrides), { strict: false }),
);
