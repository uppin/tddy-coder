# Initial Discovery — activity-wake sweep

**Date:** 2026-11-17
**Node:** `feature/activity-wake/sweep` (`#activity-wake` 6/6, the tail)
**Question:** what structural debt did the `#activity-wake` stack leave, and what closes it?

## The backlog (collected at 8d0970ecd..8736ce533)

`git diff --name-status 8d0970ecd..origin/feature/activity-wake/session-messages -- 'packages/*/docs/code-issues/' docs/dev/todo/`:

| Record | Δ | Final measurement | Close by |
|---|---|---|---|
| `packages/tddy-session-activity/docs/code-issues/oversized-file-session-watches.md` | A | `session_watches.rs` 857 prod lines (budget 500); `take_stream_batch` 255 lines, `stream` 62 (fn ceiling 60) | split into submodules |
| `packages/tddy-session-activity/docs/code-issues/oversized-file-storage.md` | A | `project_notification_log/storage.rs` 578 | split |
| `packages/tddy-daemon/docs/code-issues/oversized-file-mount-host.md` | A | `runtime/mount_host.rs` 527 | split |
| `docs/dev/todo/2026-11-16-session-watches-are-not-shown-on-the-web.md` | A | feature request, not structural debt | **TODO gate** — the user decides continue/defer; not the sweep's |

Re-measured but not worsened (out of scope, per the sweep rule "added or reopened by this stack"):
`complexity-pty-spawn-spawn-tool-on.md` (75 → 75), `complexity-mount-host-build-session-host.md`
(73 → 73, rows appended). Closed by the stack (good): `complexity-exec-tool-ports-execute-tool.md`
(deleted; `answer_execute_tool` back under budget).

`/analyze-code-issues` re-run at the tail will re-derive the same three oversized files; no extra
findings are expected, and its re-run at the sweep's `[W]` confirms none remain.

## Why the splits were deferred until now

Each file was **stack-shared** while the stack ran: #930/#931/#934/#935 all edited
`session_watches.rs`; #929/#930/#935 all edited `storage.rs`; #929 and master's `#remote-shell`
commits edited `mount_host.rs`. Splitting a stack-shared file mid-stack would have cascaded a
rebase over pinned branches. The stack has drained; nothing downstream shares them.

## The seams (from the changeset's State A; the engine's anchors refine them)

- `session_watches.rs` — store/open+locks, matcher (`watch_matches`, kinds), delivery
  (`take_delivery`/`apply_delivery`/`deliver_with`), stream (`StreamBatch`, lease,
  `take_stream_batch`), messages (`read_messages`, inbox position) → `session_watches/` submodules.
- `storage.rs` — DTOs, persistence (per-project json), replay → `storage/` submodules.
- `mount_host.rs` — builders, wiring → `mount_host/` submodules.

`move_item`/`extract_module` only; any seam the engine refuses to apply is reported, not
hand-moved (the #935 wrap's hand move is the recorded exception this node exists to retire).

## Exploration 1

The green baseline (recorded in the changeset) was taken on 8736ce533 before any move:
tddy-session-activity's 29 suites, tddy-daemon `--lib`, clippy `-D warnings` on both packages,
fmt — all green/clean. The index daemon for the engine was last seen warm on the #935 wrap
session; `./run-index-daemon --status` decides whether to reuse or restart it at green.
