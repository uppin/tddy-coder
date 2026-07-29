import React from "react";
import { mount } from "cypress/react";
import { mountWithRpc } from "./rpc/inMemory.tsx";
import { agentActivityRegistry } from "../../src/components/sessions/agentActivityRegistry";
import { UploadProgressProvider } from "../../src/rpc/uploadProgress";

/** The Agent Activity store is an app-lifetime module singleton; component tests share one JS
 *  context across cases, so clear its per-session cache before each test to keep cases that reuse a
 *  `sessionId` isolated. */
beforeEach(() => {
  agentActivityRegistry.reset();
});

/** Default `strict: false` so React 18 dev double-mount does not discard imperative refs / font state mid-test.
 *
 *  Wraps every mount in `UploadProgressProvider` to mirror production: the app shell provides it
 *  app-wide, and `GhosttyTerminalGrpc` always renders `TerminalFileDropZone` (which reads the upload
 *  store). Mounting a terminal subtree without the provider would otherwise throw
 *  "upload-progress hooks must be used within an UploadProgressProvider" — the provider is inert
 *  when no upload hooks run, so wrapping unconditionally is safe. (createElement is used because this
 *  support file is `.ts`, not `.tsx`, so JSX syntax is not available here.) */
Cypress.Commands.add("mount", (jsx, options = {}) => {
  const wrapped = React.createElement(UploadProgressProvider, null, jsx);
  return mount(wrapped, { strict: false, ...options });
});

/**
 * Mount a component with all RPC (HTTP + LiveKit) routed to an in-memory
 * `InMemoryRpcBackend`. Use this instead of `cy.intercept` when the test
 * cares about behaviour rather than wire format.
 */
Cypress.Commands.add("mountWithRpc", mountWithRpc);
