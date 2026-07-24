# Host Stats Footer

The **Host Stats Footer** is a persistent, screen-level bottom strip on the sessions
drawer screen. It consolidates connection-level and host-level telemetry into one place:
the live **byte-traffic** readout (relocated here from the top header) plus two new
host-level indicators — **available disk space** and **per-core CPU usage** — reported for
the currently selected daemon.

> **Relocation note:** the byte-traffic strip previously lived in the screen's top header
> row (see [session-drawer.md § Session Traffic Strip](./session-drawer.md#session-traffic-strip)).
> This feature moves that readout into the new bottom footer and adds the two host-level
> indicators beside it.

## Motivation

Operators watching a daemon want an at-a-glance sense of the host's headroom: is the disk
that holds their projects filling up, and how busy are the machine's cores? Today the web
shows only per-connection byte traffic, and it sits in the top header where the eye does
not naturally rest. A single bottom footer — the same strip that already hosts the mobile
keyboard button — is a more natural home for ambient telemetry and leaves room to add
host-level signals.

## Placement

- A screen-level footer rendered at the **bottom** of `SessionsDrawerScreen`, mirroring the
  existing top header row. It is `flex-shrink-0` and always visible on both desktop and
  mobile (unlike the mobile-only in-terminal keyboard strip).
- The byte-traffic readout moves out of the top header into this footer. The top header
  retains only the daemon selector.
- The mobile keyboard button remains in its existing per-terminal strip; it is not moved by
  this feature.

## Displayed values

The footer shows, left to right:

| Group | Field | Description |
|-------|-------|-------------|
| Traffic | ↑ rate / ↓ rate | Live out/in throughput (B/s, kB/s, MB/s) over the last ~2 s |
| Traffic | ↑ total / ↓ total | Cumulative session bytes sent / received |
| Traffic | Ping | Round-trip time to the LiveKit gateway in ms, or `—` |
| Disk | Available disk | Free space on the filesystem holding the daemon's default project directory (e.g. `42.1 GB free`) |
| CPU | Per-core usage | One mini bar per logical core; bar height encodes that core's utilization percentage |

The traffic sub-readout is unchanged in behavior — it is the existing `SessionTrafficStrip`
relocated into the footer.

## Host stats source

Disk and CPU figures describe the **currently selected daemon's** host (the daemon chosen in
the daemon selector). Switching the selected daemon re-fetches and re-renders the figures for
the newly selected host.

### Available disk space

- Reports **available** and **total** bytes for the filesystem that contains the daemon's
  **default project directory** — the configured repos base (`base_path` override, else
  `$HOME/<repos_base_path>` with the documented default `repos`).
- Displayed as human-readable free space (decimal SI units, matching the traffic formatter).
- **Refreshed every 60 seconds**, pushed by the daemon over the host-stats stream. Disk headroom
  changes slowly; a one-minute cadence keeps the figure current without needless traffic.

### Per-core CPU usage

- Reports the utilization **percentage of each logical core** (core 0 first), as a value in
  the range 0–100.
- Rendered as a compact row of per-core mini bars — one bar per core, height proportional to
  utilization — so the display scales to machines with many cores. Each bar exposes its core
  index and percentage for hover/inspection.
- **Refreshed every 5 seconds**, pushed by the daemon over the host-stats stream. CPU load is
  volatile; a five-second cadence gives a live feel without saturating the RPC path.

## RPC surface

A single server-streaming method on `ConnectionService` (daemon-level RPC, addressed to the
selected daemon over the shared common-room LiveKit connection — no `daemon_instance_id` payload
is needed because the transport already targets the daemon):

- `StreamHostStats(StreamHostStatsRequest) returns (stream HostStatsEvent)` — the **daemon owns the
  cadence**. On subscribe it emits one event immediately, then refreshes CPU every 5 s and disk
  every 60 s, pushing a `HostStatsEvent` carrying the latest CPU **and** disk snapshot on each tick.
- `HostStatsEvent` always carries both `cpu` (`HostCpuStats { per_core_percent: repeated float }`,
  0–100, core 0 first) and `disk` (`HostDiskStats { available_bytes, total_bytes, project_dir }` —
  the last for tooltip/debug).

`StreamHostStatsRequest` takes a `session_token`; an invalid token is rejected with an
unauthenticated error, like every other `ConnectionService` method. The web subscribes **once** via
`useHostStats` and applies each event — there is no client-side polling.

<a id="upload-progress-drag-to-upload"></a>
## Upload progress (drag-to-upload)

The footer is also the home for **file-upload progress** from the terminal's drag-to-upload
feature (see [web-terminal.md § File drop upload](./web-terminal.md#file-drop-upload)). When the
user drops files on the terminal (or picks them via the mobile Keyboard-strip Attach button), a
single **aggregate determinate bar** (`data-testid="upload-progress-indicator"`) appears inside
the footer showing `"{n} files · {pct}%"`, where the percent is total bytes uploaded across all
files in that drop. The indicator:

- **Appears** when a drop starts and **auto-hides** shortly after the drop completes.
- Exposes `data-upload-percent` (0–100) and `data-upload-file-count` for assertions.
- Surfaces a per-file failure as a transient error (`data-testid="upload-progress-error"`, e.g.
  "⚠ upload of report.iso failed — skipped"); the failed file's path is not typed into the
  terminal, and the remaining files continue.

Progress is published from the terminal's upload orchestration into a screen-level
`UploadProgressProvider`, so the drop handler (deep in the terminal subtree) and this footer (a
sibling subtree of `SessionsDrawerScreen`) share one progress snapshot — mirroring how the
traffic readout subscribes to its meter.

## Acceptance criteria

1. The byte-traffic readout renders inside the bottom footer of `SessionsDrawerScreen` and is
   **no longer present** in the top header row.
2. The footer shows the daemon's available disk space as human-readable free space, sourced
   from the `StreamHostStats` feed.
3. The footer shows one CPU mini bar per logical core, each bar's height encoding that core's
   utilization percentage, sourced from the same `StreamHostStats` feed.
4. Disk figures refresh on a 60-second cadence and CPU figures on a 5-second cadence, both driven
   by the daemon (the web opens a single subscription, not two polls).
5. The disk figure describes the filesystem that contains the daemon's default project
   directory.
6. Switching the selected daemon re-subscribes and re-renders disk and CPU figures for the newly
   selected host.
7. `StreamHostStats` rejects an invalid session token.
