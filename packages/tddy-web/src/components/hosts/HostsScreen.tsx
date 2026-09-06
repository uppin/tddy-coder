/**
 * Hosts screen — every host tddy has a record of, reachable or not.
 *
 * Presentational: it renders the rows it is given. `HostsAppPage` owns fetching them, matching the
 * `VmsScreen` / `VmsAppPage` split.
 *
 * The screen's reason to exist is the **offline** row. A host that has left the common room
 * disappears from every other surface in tddy, which is exactly when an operator wants to look at
 * it, so an offline host is rendered with its last-seen time rather than omitted.
 */

export interface HostRow {
  /** The daemon instance id — stable, and what every host-addressed RPC is keyed on. */
  instanceId: string;
  label: string;
  /** In the live roster at the moment the list was read. */
  online: boolean;
  firstSeenUnixMs: bigint;
  lastSeenUnixMs: bigint;
  reposBasePath: string;
  /** This row is the daemon serving the page. */
  isLocal: boolean;
}

export interface HostsScreenProps {
  rows: HostRow[];
  /** Reference time for relative last-seen phrasing; injected so tests pin it. */
  nowUnixMs: number;
}

export function HostsScreen({ rows, nowUnixMs }: HostsScreenProps) {
  // TODO(host-registry): implement — render one row per known host, online first then by label,
  // with an online/offline indicator and a last-seen time for offline hosts.
  void rows;
  void nowUnixMs;
  return <div data-testid="hosts-screen" />;
}
