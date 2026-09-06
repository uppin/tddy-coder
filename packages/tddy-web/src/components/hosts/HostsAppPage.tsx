/**
 * Data container for the Hosts screen: one `ListKnownHosts` call against the selected daemon.
 *
 * One RPC, deliberately. Registry membership changes rarely, so a stream here would buy nothing and
 * would owe the `tx.closed()` teardown contract in `packages/tddy-codegen/docs/server-streaming.md`.
 * Live per-host telemetry is a separate concern and arrives with `#hosts-screen 2/8`.
 */

import { useEffect, useState } from "react";
import { ConnectionService, type KnownHostEntry } from "../../gen/connection_pb";
import { useAuthContext } from "../../hooks/authProvider";
import { useDaemonClient } from "../../rpc/selectedDaemon";
import { AppShell } from "../shell/AppShell";
import { HostsScreen, type HostRow } from "./HostsScreen";

function rowFromRpc(host: KnownHostEntry): HostRow {
  return {
    instanceId: host.instanceId,
    label: host.label,
    online: host.online,
    lastSeenUnixMs: host.lastSeenUnixMs,
    reposBasePath: host.reposBasePath,
    isLocal: host.isLocal,
  };
}

export function HostsAppPage({ onNavigate }: { onNavigate: (path: string) => void }) {
  const { sessionToken } = useAuthContext();
  const client = useDaemonClient(ConnectionService);

  const [rows, setRows] = useState<HostRow[]>([]);
  const [error, setError] = useState<string | null>(null);

  // One read per visit. Both dependencies are stable for as long as the screen is looking at the
  // same host with the same session — `useDaemonClient` memoises the client per host and service —
  // so this fires once on mount. It fires again only when the selected host changes or the session
  // token is renewed, and each of those genuinely invalidates the answer: the registry belongs to
  // the daemon that was asked. A ref guard would suppress exactly those legitimate re-reads.
  useEffect(() => {
    if (!client) return;
    // Because it does re-fire on a host change, a reply from the daemon we just navigated away from
    // can still be in flight. Dropping it keeps the rows belonging to the host now selected.
    let current = true;
    client
      .listKnownHosts({ sessionToken: sessionToken ?? "" })
      .then((res) => {
        if (!current) return;
        setRows(res.hosts.map(rowFromRpc));
        setError(null);
      })
      .catch((e: unknown) => {
        if (!current) return;
        setError(e instanceof Error ? e.message : "Failed to list known hosts");
      });
    return () => {
      current = false;
    };
  }, [client, sessionToken]);

  return (
    <AppShell title="Hosts" onNavigate={onNavigate} dataTestId="hosts-app-page">
      {error ? (
        <p className="mb-3 text-sm text-destructive" data-testid="hosts-error">
          {error}
        </p>
      ) : null}
      <HostsScreen rows={rows} nowUnixMs={Date.now()} />
    </AppShell>
  );
}
