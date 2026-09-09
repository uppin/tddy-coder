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

import { formatLastSeen } from "./hostRowFormat";

export interface HostRow {
  /** The daemon instance id — stable, and what every host-addressed RPC is keyed on. */
  instanceId: string;
  label: string;
  /** In the live roster at the moment the list was read. */
  online: boolean;
  /**
   * When the host was last in the live roster. There is deliberately no `firstSeenUnixMs` here: the
   * daemon records one, but this screen has no "known since" column, and an unrendered field is
   * surface later nodes would have to keep mapping for nothing.
   */
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

/**
 * Reachable hosts first, then alphabetically by label.
 *
 * Ordering by liveness is the point: the hosts an operator can act on right now belong at the top,
 * and the rest stay listed rather than disappearing.
 */
function byLivenessThenLabel(a: HostRow, b: HostRow): number {
  if (a.online !== b.online) {
    return a.online ? -1 : 1;
  }
  return a.label.localeCompare(b.label);
}

export function HostsScreen({ rows, nowUnixMs }: HostsScreenProps) {
  const ordered = [...rows].sort(byLivenessThenLabel);

  return (
    <div data-testid="hosts-screen">
      {ordered.length === 0 ? (
        <p data-testid="hosts-empty" className="text-muted-foreground text-sm">
          No hosts recorded yet.
        </p>
      ) : (
        <table data-testid="hosts-table" className="w-full text-sm border-collapse">
          <thead>
            <tr>
              <th className="text-left py-2 pr-4">Host</th>
              <th className="text-left py-2 pr-4">Status</th>
              <th className="text-left py-2 pr-4">Last seen</th>
              <th className="text-left py-2 pr-4">Instance ID</th>
              <th className="text-left py-2">Repos base path</th>
            </tr>
          </thead>
          <tbody>
            {ordered.map((row) => (
              <tr key={row.instanceId} data-testid={`hosts-row-${row.instanceId}`}>
                <td className="py-2 pr-4">
                  {row.label}
                  {/* Every daemon self-labels "<id> (this daemon)" in its own advertisement, so in
                      a list of hosts the label alone cannot say which one is serving this page.
                      `is_local` is the daemon's answer to that, and only it can give it.

                      Its test id sits outside the `hosts-row-` namespace on purpose: that prefix
                      belongs to the row elements alone, so a marker named under it would have to be
                      excluded by hand from any prefix match over rows. */}
                  {row.isLocal ? (
                    <span
                      className="ml-1 text-xs text-muted-foreground"
                      data-testid={`hosts-local-marker-${row.instanceId}`}
                    >
                      (local)
                    </span>
                  ) : null}
                </td>
                <td
                  className={
                    row.online ? "py-2 pr-4" : "py-2 pr-4 text-muted-foreground"
                  }
                  data-testid={`hosts-row-${row.instanceId}-liveness`}
                >
                  {row.online ? "Online" : "Offline"}
                </td>
                <td
                  className="py-2 pr-4 text-muted-foreground"
                  data-testid={`hosts-row-${row.instanceId}-last-seen`}
                >
                  {formatLastSeen(row.lastSeenUnixMs, nowUnixMs)}
                </td>
                <td className="py-2 pr-4 font-mono text-xs">{row.instanceId}</td>
                <td className="py-2 font-mono text-xs">{row.reposBasePath}</td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
    </div>
  );
}
