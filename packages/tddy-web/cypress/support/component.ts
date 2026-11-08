import React from "react";
import { mount } from "cypress/react";
import { mountWithRpc } from "./rpc/inMemory.tsx";
import { agentActivityRegistry } from "tddy-chat-web";
import { sessionNotificationRegistry } from "tddy-session-web";
import { UploadProgressProvider } from "tddy-terminal-web";
import { AppClientsPorts } from "../../src/app/rpc/appClientsPorts";
import { AppPortsScope, type AppPorts } from "../../src/app/ports";
import { hashNavigationPort } from "../../src/routing/useAppLocation";
import { livekitPorts } from "tddy-livekit-web";
import { create } from "@bufbuild/protobuf";
import { registerServerStreamFallback } from "tddy-connectrpc-testkit";
import { SessionService, StartSessionEventSchema } from "tddy-client-web/gen/session_pb";

/** The Agent Activity and session-notification stores are app-lifetime module singletons; component
 *  tests share one JS context across cases, so clear their per-session state before each test to keep
 *  cases that reuse a `sessionId` isolated — a notification left behind by one spec would otherwise
 *  light up the next spec's drawer row. */
beforeEach(() => {
  agentActivityRegistry.reset();
  sessionNotificationRegistry.reset();
});

/** Screens derive their selection from the URL (docs/ft/web/1-WIP/PRD-2026-08-01-url-state-routing.md),
 *  and component tests share one window — so a hash left behind by one spec would silently pre-select
 *  a session, task or inspector tab in the next. Reset to the neutral root before each test; specs
 *  that need a deep link set their own hash afterwards, in their own `beforeEach` or test body. */
beforeEach(() => {
  window.location.hash = "/";
});

/** Default `strict: false` so React 18 dev double-mount does not discard imperative refs / font state mid-test.
 *
 *  Wraps every mount in `UploadProgressProvider` to mirror production: the app shell provides it
 *  app-wide, and `TerminalSession` always renders `TerminalFileDropZone` (which reads the upload
 *  store). Mounting a terminal subtree without the provider would otherwise throw
 *  "upload-progress hooks must be used within an UploadProgressProvider" — the provider is inert
 *  when no upload hooks run, so wrapping unconditionally is safe. (createElement is used because this
 *  support file is `.ts`, not `.tsx`, so JSX syntax is not available here.)
 *
 *  Also mounts `AppClientsPorts`, as the app root does (`RpcTransportProvider`): the hooks from
 *  `tddy-client-web` throw without a clients port, and a bare mount stands for a page whose
 *  directory lists no daemon. A spec that wants hosts or a backend mounts its own providers, which
 *  re-mount the ports from theirs.
 *
 *  `AppClientsPorts` also fills the slots component packages ask the app for: the Hosts screen's
 *  passphrase dialog (create-session) and the screen shell (session screens). */
/** The ports a mount hands the app's providers, supplied explicitly as the app container does: the page's
 *  hash navigation and the real LiveKit ports (their transport factory is what specs override). Built
 *  once — the LiveKit hooks must keep one identity. */
const liveKit = livekitPorts();
const mountedPorts: AppPorts = {
  navigation: hashNavigationPort,
  presence: liveKit.presence,
  media: liveKit.media,
  roomRpc: liveKit.roomRpc,
};

Cypress.Commands.add("mount", (jsx, options = {}) => {
  const wrapped = React.createElement(
    AppPortsScope.Provider,
    { value: mountedPorts },
    React.createElement(AppClientsPorts, null, React.createElement(UploadProgressProvider, null, jsx)),
  );
  return mount(wrapped, { strict: false, ...options });
});

/**
 * Mount a component with all RPC (HTTP + LiveKit) routed to an in-memory
 * `InMemoryRpcBackend`. Use this instead of `cy.intercept` when the test
 * cares about behaviour rather than wire format.
 */
Cypress.Commands.add("mountWithRpc", mountWithRpc);

/** The form always starts a session over `StreamStartSession`, whose last event is the unary answer.
 *  Specs that stub only the unary `startSession` keep working: the stream runs that handler and emits
 *  its answer as the single `result` event. A spec that implements the stream itself wins. */
registerServerStreamFallback({
  unary: SessionService.method.startSession,
  stream: SessionService.method.streamStartSession,
  toEvent: (result) => create(StartSessionEventSchema, { event: { case: "result", value: result } }),
});
