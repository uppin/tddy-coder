/**
 * Cypress adapter for the in-memory ConnectRPC testkit.
 *
 * Provides `cy.mountWithRpc(jsx, backend)` — mounts a component inside an
 * `RpcTransportProvider` that routes **all** RPC (HTTP and LiveKit) to an
 * in-memory `InMemoryRpcBackend` instead of a real server.
 *
 * The existing `cy.intercept`-based layer (`cypress/support/rpc/`) remains
 * available for wire-level HTTP tests; `mountWithRpc` is the preferred path
 * when the test cares about behaviour, not wire format.
 *
 * @example
 * ```ts
 * import { anInMemoryRpcBackend } from "tddy-connectrpc-testkit";
 * import { AuthService } from "../../../src/gen/auth_pb";
 *
 * const backend = anInMemoryRpcBackend()
 *   .onUnary(AuthService.method.getAuthStatus, () => ({
 *     isAuthenticated: false,
 *     user: null,
 *     sessionToken: "",
 *   }));
 *
 * cy.mountWithRpc(<ConnectionScreen />, backend);
 * // → component runs, AuthService.getAuthStatus resolves from the in-memory stub
 * ```
 */

import React from "react";
import type { InMemoryRpcBackend } from "tddy-connectrpc-testkit";
import { RpcTransportProvider } from "../../../src/rpc/transportProvider";

/**
 * Mount `component` inside an `RpcTransportProvider` backed by `backend`.
 *
 * Both HTTP and LiveKit transports are overridden — all RPC calls flow through
 * the in-memory backend regardless of protocol.
 */
export function mountWithRpc(
  component: React.ReactElement,
  backend: InMemoryRpcBackend,
): Cypress.Chainable {
  const transport = backend.transport();
  // LiveKit factory ignores room/identity and returns the same in-memory transport.
  const liveKitFactory = () => transport;

  return cy.mount(
    <RpcTransportProvider httpTransport={transport} liveKitFactory={liveKitFactory}>
      {component}
    </RpcTransportProvider>,
  );
}
