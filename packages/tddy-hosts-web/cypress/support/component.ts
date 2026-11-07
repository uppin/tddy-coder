import React from "react";
import { mount } from "cypress/react";
import type { ReactElement } from "react";
import type { InMemoryRpcBackend } from "tddy-connectrpc-testkit";
import { MediaPortProvider, PresencePortProvider } from "tddy-client-web";
import { aMediaDouble, aPresenceDouble } from "tddy-client-web/testkit";
import { type HostsMountOverrides, withHostsPorts } from "./mountWithHosts";

declare global {
  // eslint-disable-next-line @typescript-eslint/no-namespace
  namespace Cypress {
    interface Chainable {
      mount: typeof mount;
      mountWithClients(jsx: ReactElement, backend: InMemoryRpcBackend, overrides?: HostsMountOverrides): Chainable;
    }
  }
}

// No stylesheet is loaded on purpose, as in tddy-web's component support: these specs pin behaviour
// of components that carry their own layout, and a global theme would turn their fixed-position
// overlays into real, stacked layers that cover each other's controls.

/**
 * `strict: false` as in tddy-web, so a dev double-mount does not discard imperative refs mid-test.
 * A bare mount still gets the presence and media doubles: the overlays render media through the port.
 */
Cypress.Commands.add("mount", (jsx, options = {}) =>
  mount(
    React.createElement(
      PresencePortProvider,
      { port: aPresenceDouble() },
      React.createElement(MediaPortProvider, { port: aMediaDouble() }, jsx),
    ),
    { strict: false, ...options },
  ),
);
Cypress.Commands.add("mountWithClients", (jsx: ReactElement, backend: InMemoryRpcBackend, overrides?: HostsMountOverrides) =>
  mount(withHostsPorts(jsx, backend, overrides), { strict: false }),
);
