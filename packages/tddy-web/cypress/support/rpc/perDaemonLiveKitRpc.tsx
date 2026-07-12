/**
 * Cypress adapter for cross-host tests that need *different* daemons to answer with *different*
 * in-memory backends.
 *
 * `mountWithRecordingLiveKitRpc` (`./recordingLiveKitRpc.tsx`) routes both HTTP and every LiveKit
 * client to the *same* backend, which is right for single-host tests. The cross-host sessions
 * drawer, however, fans `ListSessions` out to every daemon and routes interaction to a session's
 * owning daemon — proving that routing requires each `daemon-{instanceId}` client to reach a
 * distinct backend, so host A and host B can return distinct session lists and record their own
 * calls.
 *
 * LiveKit clients are keyed by their target identity (`daemon-{instanceId}` — build the key with
 * `daemonRpcIdentity(instanceId)`); a client whose identity has no entry falls back to the HTTP
 * backend. HTTP (AuthService, TokenService) always routes to `httpBackend`.
 */

import React from "react";
import type { Transport } from "@connectrpc/connect";
import type { InMemoryRpcBackend } from "tddy-connectrpc-testkit";
import { RpcTransportProvider } from "../../../src/rpc/transportProvider";

export function mountWithPerDaemonLiveKitRpc(
  component: React.ReactElement,
  backendsByIdentity: Record<string, InMemoryRpcBackend>,
  options: { httpBackend: InMemoryRpcBackend },
): { targets: string[] } {
  const httpTransport = options.httpBackend.transport();
  const transportsByIdentity = new Map<string, Transport>(
    Object.entries(backendsByIdentity).map(([identity, backend]) => [identity, backend.transport()]),
  );
  const targets: string[] = [];

  cy.mount(
    <RpcTransportProvider
      httpTransport={httpTransport}
      liveKitFactory={(_room, targetIdentity) => {
        targets.push(targetIdentity);
        return transportsByIdentity.get(targetIdentity) ?? httpTransport;
      }}
    >
      {component}
    </RpcTransportProvider>,
  );

  return { targets };
}
