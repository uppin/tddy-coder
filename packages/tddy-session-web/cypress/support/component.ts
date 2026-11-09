import { mount } from "cypress/react";
import { createElement, type ReactElement } from "react";
import { create } from "@bufbuild/protobuf";
import { anInMemoryRpcBackend, registerServerStreamFallback, type InMemoryRpcBackend } from "tddy-connectrpc-testkit";
import { SessionService, StartSessionEventSchema } from "tddy-client-web/gen/session_pb";
import { MediaPortProvider, PresencePortProvider, RoomRpcPortProvider } from "tddy-client-web";
import {
  aMediaDouble,
  aPresenceDouble,
  aRoomRpcDouble,
  type MountWithClientsOverrides,
  withClients,
} from "tddy-client-web/testkit";
import { agentActivityRegistry } from "tddy-chat-web";
import { UploadProgressProvider } from "tddy-terminal-web";
import { StartPassphraseProvider } from "tddy-session-create-web";
import { sessionNotificationRegistry } from "../../src/lib/sessionNotificationRegistry";
import { SessionShellProvider } from "../../src/sessionShell";
import { SessionShellDouble } from "./sessionShellDouble";
// No stylesheet, as in tddy-web: the specs were written unstyled (a closed drawer is still "visible" to
// them). A spec that measures real layout imports `tddy-ui-web/theme.css` itself.

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
  // The LiveKit ports, as doubles: no room, no media, nobody present until a spec says so.
  const livekit = createElement(
    PresencePortProvider,
    { port: aPresenceDouble() },
    createElement(
      RoomRpcPortProvider,
      { port: aRoomRpcDouble(backend) },
      createElement(MediaPortProvider, { port: aMediaDouble() }, jsx),
    ),
  );
  const shelled = createElement(
    SessionShellProvider,
    { shell: SessionShellDouble },
    createElement(StartPassphraseProvider, { prompt: NoPassphraseDialog }, livekit),
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

/** The form always starts a session over `StreamStartSession`, whose last event is the unary answer.
 *  Specs that stub only the unary `startSession` keep working: the stream runs that handler and emits
 *  its answer as the single `result` event. A spec that implements the stream itself wins. */
registerServerStreamFallback({
  unary: SessionService.method.startSession,
  stream: SessionService.method.streamStartSession,
  toEvent: (result) => create(StartSessionEventSchema, { event: { case: "result", value: result } }),
});
