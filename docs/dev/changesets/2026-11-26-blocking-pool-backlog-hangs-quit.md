# 2026-11-26 — A QueryBranch backlog cannot starve ListSessions or hang Desktop quit
**Type:** Fix

On 2026-11-26 Tddy Desktop's session list went empty and never recovered, and Cmd+Q then left the
app not responding, using CPU, for about 7 minutes. The work-queue view's `QueryBranch` polling put
its git reads straight on the blocking pool; a call that timed out after 300 s left its job running,
so the pool filled with abandoned jobs and `ListSessions` timed out behind them (~7,900 timeouts).
Quit ran the daemon shutdown on the UI thread, and `kill_all` waited for its children on the same
saturated pool.

- **tddy-daemon-rpc**: `QueryBranch`'s blocking legs run in bounded, coalescing lanes
  (`work_queue/blocking_lane.rs`) — see
  [architecture.md](../../../packages/tddy-daemon-rpc/docs/architecture.md#querybranch-and-the-blocking-pool).
  `query_branch_rpc` was split into one method per leg (157 → 59 lines).
- **tddy-cli-sessions**: `kill_all` waits on the async timer, not the blocking pool.
- **tddy-desktop**: a quit waits at most 10 s for the daemon's children —
  [quit.md](../../../packages/tddy-desktop/docs/quit.md).

PR #1059.
