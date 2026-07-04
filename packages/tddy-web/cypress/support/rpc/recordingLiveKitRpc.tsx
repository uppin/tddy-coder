/**
 * Cypress adapter for asserting *which participant identity* a LiveKit RPC client was built for
 * — `mountWithRpc` (`./inMemory.tsx`) ignores `targetIdentity` entirely, which is right for tests
 * that only care that an RPC happened, but not for daemon-routing tests that must prove the client
 * targeted `daemon-<instanceId>` for a specific selected daemon.
 *
 * Both HTTP and LiveKit RPC route to the same in-memory `backend`; every LiveKit client build is
 * additionally recorded into the returned `targets` array, in call order.
 */

import React from "react";
import type { InMemoryRpcBackend } from "tddy-connectrpc-testkit";
import { RpcTransportProvider } from "../../../src/rpc/transportProvider";

export function mountWithRecordingLiveKitRpc(
  component: React.ReactElement,
  backend: InMemoryRpcBackend,
): { targets: string[] } {
  const transport = backend.transport();
  const targets: string[] = [];

  cy.mount(
    <RpcTransportProvider
      httpTransport={transport}
      liveKitFactory={(_room, targetIdentity) => {
        targets.push(targetIdentity);
        return transport;
      }}
    >
      {component}
    </RpcTransportProvider>,
  );

  return { targets };
}
