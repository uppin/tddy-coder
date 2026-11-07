import React from "react";
import { mount } from "cypress/react";
import type { ReactElement, ReactNode } from "react";
import { create } from "@bufbuild/protobuf";
import { anInMemoryRpcBackend, registerServerStreamFallback, type InMemoryRpcBackend } from "tddy-connectrpc-testkit";
import { SessionService, StartSessionEventSchema } from "tddy-client-web/gen/session_pb";
import { aClientsPortDouble, type MountWithClientsOverrides, withClients } from "tddy-client-web/testkit";
import { StartPassphraseProvider } from "../../src";
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

/** The host application's passphrase dialog is not this package's: a form under test asks for none. */
function NoPassphraseDialog(): null {
  return null;
}

/** The ports a create-session form needs, plus the slot its host application fills. */
function aCreateSessionHost(jsx: ReactNode, backend: InMemoryRpcBackend, overrides?: MountWithClientsOverrides): ReactElement {
  return React.createElement(StartPassphraseProvider, { prompt: NoPassphraseDialog }, withClients(jsx as ReactElement, backend, overrides));
}

/**
 * `strict: false` as in tddy-web, so a dev double-mount does not discard imperative refs mid-test.
 *
 * A bare mount stands for a page whose host directory lists no daemon: the form's clients are handed
 * in as props (over `cy.intercept` or a backend of the spec's own), and nothing else answers.
 */
Cypress.Commands.add("mount", (jsx, options = {}) => {
  const nothing = anInMemoryRpcBackend();
  return mount(aCreateSessionHost(jsx, nothing, { clients: aClientsPortDouble(nothing, { daemons: [] }) }), {
    strict: false,
    ...options,
  });
});
Cypress.Commands.add("mountWithClients", (jsx: ReactElement, backend: InMemoryRpcBackend, overrides?: MountWithClientsOverrides) =>
  mount(aCreateSessionHost(jsx, backend, overrides), { strict: false }),
);

/** The form always starts a session over `StreamStartSession`, whose last event is the unary answer.
 *  Specs that stub only the unary `startSession` keep working: the stream runs that handler and emits
 *  its answer as the single `result` event. A spec that implements the stream itself wins. */
registerServerStreamFallback({
  unary: SessionService.method.startSession,
  stream: SessionService.method.streamStartSession,
  toEvent: (result) => create(StartSessionEventSchema, { event: { case: "result", value: result } }),
});
