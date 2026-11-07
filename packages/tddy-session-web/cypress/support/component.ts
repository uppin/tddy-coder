import { mount } from "cypress/react";
import { createElement, type ReactElement } from "react";
import { anInMemoryRpcBackend, type InMemoryRpcBackend } from "tddy-connectrpc-testkit";
import { type MountWithClientsOverrides, withClients } from "tddy-client-web/testkit";
import { agentActivityRegistry } from "tddy-chat-web";
import { UploadProgressProvider } from "tddy-terminal-web";
import { StartPassphraseProvider } from "tddy-session-create-web";
import { sessionNotificationRegistry } from "../../src/lib/sessionNotificationRegistry";
import { SessionShellProvider } from "../../src/sessionShell";
import { SessionShellDouble } from "./sessionShellDouble";
import "tddy-ui-web/theme.css";

/** The Agent Activity and session-notification stores are app-lifetime module singletons; component
 *  tests share one JS context across cases, so clear their per-session state before each test to keep
 *  cases that reuse a `sessionId` isolated. */
beforeEach(() => {
  agentActivityRegistry.reset();
  sessionNotificationRegistry.reset();
});

/** Screens derive their selection from the URL, and component tests share one window — so a hash left
 *  behind by one spec would silently pre-select a session or inspector state in the next. Reset to the
 *  neutral root before each test; specs that need a deep link set their own hash afterwards. */
beforeEach(() => {
  window.location.hash = "/";
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

/** The host application's passphrase dialog is not this package's: a start under test asks for none. */
function NoPassphraseDialog(): null {
  return null;
}

/**
 * Stands the subtree where the app shell puts it: the terminal session always renders its upload
 * affordances, which need the `UploadProgressProvider` and read a daemon client through the clients
 * port even when a spec never drives an upload.
 */
function inShell(jsx: ReactElement, backend: InMemoryRpcBackend, overrides?: MountWithClientsOverrides): ReactElement {
  const shelled = createElement(
    SessionShellProvider,
    { shell: SessionShellDouble },
    createElement(StartPassphraseProvider, { prompt: NoPassphraseDialog }, jsx),
  );
  return withClients(createElement(UploadProgressProvider, null, shelled), backend, overrides);
}

/** `strict: false` as in tddy-web, so a dev double-mount does not discard imperative refs mid-test. */
Cypress.Commands.add("mount", (jsx, options = {}) =>
  mount(inShell(jsx as ReactElement, anInMemoryRpcBackend()), { strict: false, ...options }),
);
Cypress.Commands.add("mountWithClients", (jsx: ReactElement, backend: InMemoryRpcBackend, overrides?: MountWithClientsOverrides) =>
  mount(inShell(jsx, backend, overrides), { strict: false }),
);
