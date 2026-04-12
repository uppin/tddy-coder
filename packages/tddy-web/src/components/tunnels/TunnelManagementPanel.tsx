import { useCallback, useEffect, useState } from "react";
import { create } from "@bufbuild/protobuf";
import { createClient } from "@connectrpc/connect";
import { createConnectTransport } from "@connectrpc/connect-web";
import type { TunnelAdvertisement } from "../../gen/tunnel_management_pb";
import {
  ListTunnelAdvertisementsRequestSchema,
  OpenBrowserForTunnelRequestSchema,
  TunnelManagementService,
} from "../../gen/tunnel_management_pb";

export type TunnelManagementPanelProps = {
  /**
   * Connect transport base URL (e.g. `${window.location.origin}/rpc`).
   * Override in tests to match Cypress intercept host.
   */
  rpcBaseUrl: string;
};

/**
 * Tunnel status from daemon RPC; **Open browser** sends `OpenBrowserForTunnel` using the listed row
 * (`sessionCorrelationId` + `authorizeUrl` from `ListTunnelAdvertisements`).
 */
export function TunnelManagementPanel({ rpcBaseUrl }: TunnelManagementPanelProps) {
  const [advertisements, setAdvertisements] = useState<TunnelAdvertisement[]>([]);

  const refreshAdvertisements = useCallback(async () => {
    const transport = createConnectTransport({
      baseUrl: rpcBaseUrl,
      useBinaryFormat: true,
    });
    const client = createClient(TunnelManagementService, transport);
    const res = await client.listTunnelAdvertisements(create(ListTunnelAdvertisementsRequestSchema, {}));
    setAdvertisements(res.advertisements);
  }, [rpcBaseUrl]);

  useEffect(() => {
    void refreshAdvertisements();
  }, [refreshAdvertisements]);

  const primary = advertisements[0];
  const canOpenBrowser = Boolean(primary?.sessionCorrelationId && primary.authorizeUrl);

  const onOpenBrowser = () => {
    if (!primary?.sessionCorrelationId || !primary.authorizeUrl) {
      return;
    }
    const transport = createConnectTransport({
      baseUrl: rpcBaseUrl,
      useBinaryFormat: true,
    });
    const client = createClient(TunnelManagementService, transport);
    void client.openBrowserForTunnel(
      create(OpenBrowserForTunnelRequestSchema, {
        sessionCorrelationId: primary.sessionCorrelationId,
        url: primary.authorizeUrl,
      }),
    );
  };

  return (
    <div>
      <ul data-testid="tunnel-ad-list">
        {advertisements.map((a) => (
          <li key={a.sessionCorrelationId}>{a.sessionCorrelationId}</li>
        ))}
      </ul>
      <button type="button" data-testid="tunnel-open-browser" disabled={!canOpenBrowser} onClick={onOpenBrowser}>
        Open browser
      </button>
    </div>
  );
}
