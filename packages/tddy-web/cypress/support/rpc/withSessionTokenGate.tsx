/**
 * Mount a subtree on the production HTTP transport with a signed-in user's access token installed
 * in the transport's auth-token gate — the same seam `AuthProvider` fills from `useAuth()`.
 *
 * Use this when a spec needs to prove what a *request* carries rather than what a screen renders:
 * every RPC whose message owns a `sessionToken` field leaves through
 * `createAuthGateInterceptor`, which rewrites that field per request. A harness that builds its own
 * bare `createConnectTransport` skips the gate entirely and would send an empty credential.
 */

import React, { useEffect, useState, type ReactNode } from "react";
import { RpcTransportProvider, useAuthTokenGate } from "../../../src/rpc/transportProvider";

function InstallSessionToken({ token, children }: { token: string; children: ReactNode }) {
  const gate = useAuthTokenGate();
  const [installed, setInstalled] = useState(false);

  useEffect(() => {
    gate.current = async () => token;
    setInstalled(true);
    return () => {
      gate.current = null;
    };
  }, [gate, token]);

  // Hold the children back one render: a child that fires its RPC from a mount effect would
  // otherwise race the gate and send the request before any resolver is installed.
  return installed ? <>{children}</> : null;
}

/**
 * Wrap `children` in the production transport with `token` as the signed-in session token.
 */
export function withSessionTokenGate(token: string, children: ReactNode) {
  return (
    <RpcTransportProvider>
      <InstallSessionToken token={token}>{children}</InstallSessionToken>
    </RpcTransportProvider>
  );
}
