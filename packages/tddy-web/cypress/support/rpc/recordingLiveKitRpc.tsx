/**
 * Cypress adapter for asserting *which participant identity* a LiveKit RPC was built for
 * — `mountWithRpc` (`./inMemory.tsx`) ignores `targetIdentity` entirely, which is right for tests
 * that only care that an RPC happened, but not for daemon-routing tests that must prove the client
 * targeted `daemon-<instanceId>` for a specific selected daemon.
 *
 * Both HTTP and LiveKit RPC route to the same in-memory `backend`; every LiveKit client build is
 * additionally recorded into the returned `targets` array (in build order), and every individual
 * RPC call is recorded into `rpcCalls` as `{ targetIdentity, method, stream? }` (in call order) —
 * the latter lets a test prove a *specific* RPC (e.g. `signalSession`) targeted the daemon
 * participant, independent of which other clients were built for other RPCs in the same flow.
 */

import React from "react";
import type { InMemoryRpcBackend } from "tddy-connectrpc-testkit";
import type { Transport } from "@connectrpc/connect";
import { RpcTransportProvider } from "../../../src/rpc/transportProvider";

/** A recorded LiveKit RPC: which participant identity it targeted and the service method name. */
export interface RecordedRpcCall {
  /** The LiveKit participant identity the calling client was built for. */
  targetIdentity: string;
  /** The service method name as the proto-declared name (e.g. `SignalSession`, `ConnectSession`, `StreamTerminalOutput`). */
  method: string;
  /** True for streaming RPCs, false for unary. */
  stream: boolean;
}

export function mountWithRecordingLiveKitRpc(
  component: React.ReactElement,
  backend: InMemoryRpcBackend,
): { targets: string[]; rpcCalls: RecordedRpcCall[] } {
  const transport = backend.transport();
  const targets: string[] = [];
  const rpcCalls: RecordedRpcCall[] = [];

  cy.mount(
    <RpcTransportProvider
      httpTransport={transport}
      liveKitFactory={(_room, targetIdentity) => {
        targets.push(targetIdentity);
        // Wrap the backend transport so each RPC records the participant identity its client was
        // built for. The backend's own handlers can't see the target (one shared transport), so
        // recording at the transport layer is the only way to tie a specific RPC to its route.
        const wrapped: Transport = {
          unary: (method, signal, timeoutMs, header, input, contextValues) => {
            rpcCalls.push({ targetIdentity, method: method.name, stream: false });
            return transport.unary(method, signal, timeoutMs, header, input, contextValues);
          },
          stream: (method, signal, timeoutMs, header, input, contextValues) => {
            rpcCalls.push({ targetIdentity, method: method.name, stream: true });
            return transport.stream(method, signal, timeoutMs, header, input, contextValues);
          },
        };
        return wrapped;
      }}
    >
      {component}
    </RpcTransportProvider>,
  );

  return { targets, rpcCalls };
}
