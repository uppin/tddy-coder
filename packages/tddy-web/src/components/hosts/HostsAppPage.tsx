/**
 * Data container for the Hosts screen: one `ListKnownHosts` call against the selected daemon.
 *
 * One RPC, deliberately. Registry membership changes rarely, so a stream here would buy nothing and
 * would owe the `tx.closed()` teardown contract in `packages/tddy-codegen/docs/server-streaming.md`.
 * Live per-host telemetry is a separate concern and arrives with `#hosts-screen 2/8`.
 */

import { ConnectionService } from "../../gen/connection_pb";
import { useAuthContext } from "../../hooks/authProvider";
import { useDaemonClient } from "../../rpc/selectedDaemon";
import { AppShell } from "../shell/AppShell";
import { HostsScreen, type HostRow } from "./HostsScreen";

export function HostsAppPage({ onNavigate }: { onNavigate: (path: string) => void }) {
  const { sessionToken } = useAuthContext();
  const client = useDaemonClient(ConnectionService);

  // TODO(host-registry): implement — call listKnownHosts and map the response to HostRow[].
  void sessionToken;
  void client;
  const rows: HostRow[] = [];

  return (
    <AppShell title="Hosts" onNavigate={onNavigate} dataTestId="hosts-app-page">
      <HostsScreen rows={rows} nowUnixMs={Date.now()} />
    </AppShell>
  );
}
