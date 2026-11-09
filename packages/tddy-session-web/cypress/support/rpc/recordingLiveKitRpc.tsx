/**
 * Mount into the drawer world and record which daemon participant each RPC was addressed to.
 *
 * Every host answers from the same `backend`; `targets` lists the `daemon-<instanceId>` identities a
 * host connection was built for and `rpcCalls` every call in order — which is how a spec proves a
 * specific RPC was routed to a specific daemon.
 */
import type { ReactElement } from "react";
import type { InMemoryRpcBackend } from "tddy-connectrpc-testkit";
import { mountRouted, type RecordedRpcCall, type RoutedMount } from "./drawerWorld";

export type { RecordedRpcCall };

export function mountWithRecordingLiveKitRpc(component: ReactElement, backend: InMemoryRpcBackend): RoutedMount {
  return mountRouted(component, { httpBackend: backend });
}
