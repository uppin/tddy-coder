# `code_navigation.CodeNavigationService`

The code pane's code session, go-to-definition, references and hover for the web's
[session code pane](../../../docs/ft/web/session-code-pane.md), answered by the warm code-intelligence index. The service owns no index: it authorises a request,
then forwards it. `tddy-daemon` wires it; the handler itself is
[`tddy-daemon-rpc`'s `CodeNavigationServiceImpl`](../../tddy-daemon-rpc/docs/architecture.md#code-navigation).

| RPC | Purpose |
|-----|---------|
| `CodeSession` | The Code pane's bidirectional stream: warms the worktree's index, streams its progress, answers hover / definition / references / probe queries — holding those asked while the index loads — and keeps the index alive while open |
| `Definition` | Where the symbol at a position of a worktree file is defined, with the index's state |
| `References` | Every reference to that symbol, its declaration included, with the index's state |
| `Hover` | The language server's hover markdown for it (unset when there is none), with the index's state |
| `WatchCodeIndex` | The header's passive follow of a session's worktree index: its state, then every change, whoever started the load, until the client leaves. Never starts a load nor the index daemon |
| `OpenPlan` | Load a restructure plan file of the worktree into the index's plan store; its operations with status and staleness |
| `WatchPlan` | The same snapshot, then a new one whenever a status or stale reason changes |
| `RunPlan` | Apply the plan through the index, streaming each operation's outcome |

The wire contract is [`tddy-service/proto/code_navigation.proto`](../../tddy-service/docs/code-navigation-proto.md).
`runtime.rs` builds `CodeNavigationServiceImpl` from the daemon's `WorktreeServiceImpl` and, when an
`index_daemon:` section exists, its `IndexDaemonRegistry` — handed over as the handler's
`IndexChannelSource` port, which `index_daemon/registry.rs` implements — and registers the entry
beside the worktree service it borrows. The daemon holds no RPC method of its own for this service.

When an `index_daemon:` section exists, `runtime.rs` also installs an `IndexWarmupObserver` on the
session host (`DaemonSessionHost::with_worktree_observer`) holding the same registry. The index-daemon
block therefore precedes the session host's construction in `build`, so the observer can hold the
registry. Without the section nothing is installed: no session is warmed at start, `CodeSession` and the
navigation calls answer `FailedPrecondition`, and `WatchCodeIndex` ends empty.

## The path every request takes

1. **Authorise exactly as `WorktreeService` does.** The service holds the daemon's
   `WorktreeServiceImpl` and calls its `resolve_listed_worktree` (session token → GitHub user → mapped
   OS user → the project's main repo on this host → the `worktree_path` must appear in that repo's
   `git worktree list`). It cannot reach a path the file RPCs would refuse to read, and it answers the
   same codes for an unknown token, an unknown project or an unlisted worktree.
2. **Refuse a `rel_path` that leaves the worktree**, with the same shape check the file RPCs apply
   (`validate_rel_path_shape`: no `..`, no absolute path); the refusal is `InvalidArgument`.
3. **Require the index daemon.** Without an `index_daemon:` section the daemon holds no
   `IndexDaemonRegistry`, so the handler has no channel source and every method answers `FailedPrecondition` naming that section. There is
   no fallback to `tddy_lsp_executor` or any other language server: two indexes answering the same
   pane would disagree.
4. **Dial it.** The channel source — in the daemon, `IndexDaemonRegistry::connect` — starts the index daemon on the first request and returns
   a channel to it, refreshing its idle timer. A start or dial failure is `Unavailable`. Nothing is
   started for a request that failed an earlier step. `WatchCodeIndex` dials with
   `IndexDaemonRegistry::connect_if_running` instead, which never starts the process and does not count
   as use.
5. **Forward** a `code_index.CodeIndexService` call with the listed worktree as `workspace_root`, the
   `rel_path` as `file`, and the position unchanged: both services speak one-based lines and one-based
   byte columns.
6. **Map the answer back.** Each index `CodeLocation{file, range, outside_root}` becomes a
   `CodeLocation{rel_path, range, outside_worktree}`, and the answer's `IndexState` becomes its
   `CodeIndexState`. A refusal from the index (a non-`.rs` file, an unreadable file, a language-server
   failure) reaches the caller with the index's status.

**Keeping the index alive.** A `CodeSession` dials once, at open, and holds that channel. So that a pane
in use is not left behind by the registry's idle stop, the session calls
`IndexDaemonRegistry::keep_alive` on every query and on a heartbeat every `keep_alive_every()` — a third
of the configured `idle_timeout_secs`, clamped between `KEEP_ALIVE_AT_LEAST_EVERY` (100 ms) and
`KEEP_ALIVE_AT_MOST_EVERY` (60 s), which also stays well under the index daemon's own unused-server
eviction (600 s). `keep_alive` refreshes the running process's idle timer and starts nothing. The
heartbeat also re-warms the session's loaded or loading root, which is what keeps its language server in
use; the mechanics are in
[tddy-daemon-rpc's `CodeSession`](../../tddy-daemon-rpc/docs/architecture.md#codesession).

`IndexDaemonRegistry::connect`, `connect_if_running`, `keep_alive` and `keep_alive_every` are the
registry's implementation of the `IndexChannelSource` port and have this service as their production
caller. `tddy-index-daemon` is a
dependency of `tddy-daemon-rpc` (for the generated `code_index` client), and only a dev-dependency of
this crate.

## Restructure plans

The plan calls take the path above — the same authorisation, the same refusal of an unlisted worktree
or a `rel_path` that leaves it, the same dial — and then read the plan file from the worktree and
forward to `LoadPlans`, `PlanStatus` and `Apply`. The daemon holds no code of its own for them; how the
rows, statuses and run events are built is in
[tddy-daemon-rpc's restructure plans](../../tddy-daemon-rpc/docs/architecture.md#restructure-plans).

## Warming a session's index

A worktree's index starts loading from three places, all through the same registry channel (the first
of them starts the index daemon):

- **The Code pane opening** — its `CodeSession` calls `code_index.Warm` for the worktree;
- **the session agent's first code-tool call** (`Lsp*`, `restructure_*`), which reaches the index through
  the executor and spawns its language server;
- **a session start** — a started claude-cli, cursor-cli or workspace session announces its worktree to
  the host's `SessionWorktreeObserver`, and the daemon's observer runs `code_index.Warm` for it in the
  background. Starting a session never waits on it, and a failed warm is only logged.

Only a worktree with a `Cargo.toml` at its root is warmed, because the index serves Rust. Nothing records
the progress per session: it is kept per worktree root by the index daemon and followed through
`code_index.WatchRoot`, by `WatchCodeIndex` and `CodeSession` alike, whoever started the load — so a
session the host no longer remembers (after a restart) or whose start announced no worktree (sandboxed,
tool and split starts, PR-stack and grill-me children) shows its load as soon as anything starts one. The
mechanics are in
[tddy-daemon-rpc's code index warm-up](../../tddy-daemon-rpc/docs/architecture.md#code-index-warm-up).

## Who may watch

`WatchCodeIndex` is keyed by session id, not by a worktree path, and applies the worktree service's
`resolve_owned_session_dir`: token → OS user → `<sessions base>/sessions/<id>` must exist. A session
of another user and one that does not exist answer the same `NotFound`. It is the one ownership model
the worktree service's `RestoreSessionWorktree` shares. Only then does it read the session's
`.session.yaml` for its worktree; an unreadable record is `FailedPrecondition`. `CodeSession` is keyed
by project and worktree and authorised with `resolve_listed_worktree`, like the navigation calls.

## Transport

The service is registered among the daemon's RPC entries, so it is reachable over the transports the
web reaches the daemon on. It is generated with an RPC-server pass only (no tonic adapter): nothing reaches it over the
local gRPC socket.

## Testing

The handler's unit tests (path shape, location mapping) live in `tddy-daemon-rpc`;
`tests/code_navigation_acceptance.rs` here runs the production `IndexDaemonRegistry` against a stand-in
program that binds a fake `code_index` server over a Unix socket, so the lazy start, readiness wait
and dial are the production ones.

| Test | Pins |
|---|---|
| a definition request is forwarded for the session worktree | the fake index is asked once with `workspace_root` = the listed worktree and `file` = `rel_path`; the answer maps to `CodeLocation` |
| a worktree not listed for the project is refused | `FailedPrecondition`, the fake is never asked, the registry is never started |
| no `index_daemon:` section | `FailedPrecondition` naming `index_daemon` |
| the first request starts the index daemon | the stand-in is launched with `--grpc-uds <socket>` and the registry reports that socket running |

`tests/support/mod.rs` holds what the code-index suites share: a scripted `code_index` fake (one
root's progress the test moves; `Warm` and `WatchRoot` follow it; navigation answers empty and `LOADING`
until `READY`, then only the call to `foo`; it can fail the load or stop following), the stand-in
index-daemon host (`AnIndexDaemonHost`, including a running registry and a configurable idle timeout),
the project + session fixture, and `AN_EVENT_IS_DUE_WITHIN` (15 s — 5 s timed out on a loaded host).

`tests/code_session_acceptance.rs` drives `CodeSession` through `ServiceEntry::start_bidi_stream`:

| Test | Pins |
|---|---|
| opening a code session warms its worktree and streams progress until ready | `Warm` then `WatchRoot` for the listed worktree; progress events until `READY` |
| a hover / definition / references asked while the index loads is held and answered once it is ready | `held`, then the answer after `READY` |
| a cancelled held hover is never answered | `cancel` drops it |
| a held hover on an index that then fails is answered with the failure | `error: "the code index failed: <reason>"` |
| a probe while the index loads answers unknown without waiting; on a symbol with a definition answers definable with its location; where nothing is defined answers none | the three `Definability` answers |
| a query reusing a held query's id replaces it | one answer, for the later query |
| an index let go while the pane is open is warmed again; a hover asked after it was let go is held until it is ready again | re-warm on `IDLE`; held through `IDLE` |
| a code session whose index stops following its worktree ends unavailable | `WatchRoot` ending ends the session `UNAVAILABLE` |
| a code session on an unlisted worktree / with a token nobody holds is refused; a query before open ends the session | authorisation and `INVALID_ARGUMENT` with its message |
| a code session on a worktree that is not a Rust worktree has no index | one `NONE` progress with why; queries answered `error`; the index daemon not dialled |
| an open code session keeps the index daemon running and lets it go once closed | 1 s idle timeout: not stopped over 2.5 s open, stopped once closed |
| an open code session keeps its loaded root in use by warming it again | the heartbeat re-warms a `READY` root |

`tests/code_index_warmup_acceptance.rs`:

| Test | Pins |
|---|---|
| a session's Rust worktree handed to the warm-up is warmed through the index daemon | `warm_for_session` runs `Warm` for the worktree |
| without an index daemon no warm starts | no warm, and `WatchCodeIndex` ends empty |
| `WatchCodeIndex` follows a load another caller started and never warms | it relays `WatchRoot` and sends no `Warm` |
| `WatchCodeIndex` for a session after a restart follows its worktree | no in-memory record is needed |
| `WatchCodeIndex` reports an index that failed with its reason | `FAILED` with `error` |
| `WatchCodeIndex` while no index daemon runs ends with nothing and starts none | `connect_if_running` |
| `WatchCodeIndex` for a session whose worktree is not created yet / without a `Cargo.toml` / without an `index_daemon:` section ends with nothing | the empty-stream cases |
| `WatchCodeIndex` of a session whose record is unreadable is a failed precondition | `FailedPrecondition` |
| another user's watch is refused | `NotFound`, the same as for a missing session, and no progress |

`tests/plan_dialog_acceptance.rs` (the same stand-in program, with a fake `code_index` server that holds
one plan's store state and answers `LoadPlans`, `ListPlans` and `PlanStatus` from it, and replays a script
for `Apply`):

| Test | Pins |
|---|---|
| `open_plan_loads_the_plan_for_the_session_worktree` | the fake is asked to load the plan under the listed worktree, and the snapshot lists every operation pending with its id, kind, item, file and group |
| `watch_plan_reports_a_stale_operation_with_its_reason` | the stale operation's row carries the store's reason |
| `run_plan_streams_each_operations_outcome` | each applied operation arrives as it lands, and the stream ends with the outcome |
| `run_plan_reports_a_group_that_did_not_compile_as_rolled_back` | an `Apply` ending in the group refusal ends the stream with a failure naming the group and the ids of that group's operations only |
| `a_worktree_not_listed_for_the_project_is_refused` | `FailedPrecondition`, and the index daemon is never started |
