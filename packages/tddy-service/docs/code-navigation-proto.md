# `code_navigation.proto`

The web-facing contract for code navigation in a session's worktree. `package code_navigation`,
service `CodeNavigationService`; implemented by `tddy-daemon`
([how a request is served](../../tddy-daemon/docs/code-navigation-service.md)).

| RPC | Request → Response |
|---|---|
| `Definition` | `DefinitionRequest` → `locations: [CodeLocation]`, `state: CodeIndexState` |
| `References` | `ReferencesRequest` → `locations: [CodeLocation]`, `state: CodeIndexState` |
| `Hover` | `HoverRequest` → `optional markdown`, `state: CodeIndexState` |
| `WatchCodeIndex` | `WatchCodeIndexRequest` → stream of `CodeIndexProgress` |
| `CodeSession` | stream of `CodeSessionRequest` → stream of `CodeSessionEvent` (bidirectional) |
| `OpenPlan` | `OpenPlanRequest` → `PlanSnapshot` |
| `WatchPlan` | `WatchPlanRequest` → stream of `PlanSnapshot` |
| `RunPlan` | `RunPlanRequest` → stream of `PlanRunEvent` |

The three navigation requests name their worktree the way `worktree.WorktreeService` does —
`session_token`, `project_id`, `worktree_path` — plus the `rel_path` of the file (relative to the
worktree) and a `position`. Authorisation is the worktree service's.
`WatchCodeIndex` is keyed by session instead, see [Watching a session's index](#watching-a-sessions-index).
`CodeSession` names its worktree the same way in its first frame, see [The code session](#the-code-session).
The three plan requests name their worktree the same way and carry the plan file's `rel_path`, see
[Restructure plans](#restructure-plans).

## Coordinates and locations

- `SourcePosition` is a **one-based line and one-based byte column**: the coordinates
  `code_index.CodeIndexService` speaks. A client counting characters in a JavaScript string must count
  UTF-8 bytes instead.
- `CodeLocation` is `{rel_path, range, outside_worktree}`. Inside the worktree `rel_path` is relative to
  its root; outside it (a dependency, the standard library) it is the absolute path and
  `outside_worktree` is true, so a pane that can open only the worktree's files does not offer to.
  The worktree vocabulary (`rel_path`, `outside_worktree`) is this contract's; the index's own
  `CodeLocation` says `file` / `outside_root`.
- `HoverResponse.markdown` is unset when there is nothing to say about the position.
- Each navigation answer carries the `state` of the index when it answered: `LOADING` or `READY`.
  rust-analyzer answers empty while its crate graph loads, so an empty answer means "does not resolve"
  only when `state` is `READY`.

## Index progress

`CodeIndexProgress` is `code_index.IndexProgress` as the web reads it. It is declared here, not imported,
because the web reads only this package's protos:

| Field | Meaning |
|---|---|
| `line` | the index's own one-line account of what it is doing |
| `phase` | the progress phase the language server reported on most recently, e.g. `Indexing` |
| `percentage` | that phase's highest percentage, 0 to 100 |
| `furthest` | the furthest point any phase reached |
| `ready` | kept for hosts that predate `state`; a client reads `state` |
| `error` | why, when `state` is `FAILED` or `NONE` |
| `state` | `CodeIndexState`: `IDLE` (no language server holds the worktree), `LOADING` (the server is loading its crate graph, or busy again after an edit), `READY` (its latest status said quiescent), `FAILED` (the server could not start or exited unasked; `error` says why), `NONE` (the worktree is not a Rust worktree and has no index — `CodeSession` only). `UNSPECIFIED` comes only from a host that predates the field, and is read from `error` / `ready` |
| `phases` | every phase running now, in begin order, as `CodeIndexPhase { title, message, optional percentage }` — the percentage absent when the phase never reported one |

The progress is the worktree root's, folded by the index daemon from the language server's first
notification, so a stream opened mid-load names every phase already running.

## Watching a session's index

`WatchCodeIndex(session_token, session_id)` streams the progress of the session's worktree's index, for
the session header's indexing indicator, so a client needs nothing but the session id. It is passive:
it never starts a load, nor the index daemon. It opens with the root's current state and follows every
change, whoever started the load (a Code pane, the session agent's code tools, a session start), until
the client leaves; a change replaces the last one, so a slow reader sees the newest rather than every
step. It ends with no message when there is nothing to follow — no `index_daemon:` section, no index
daemon running, a session whose worktree does not exist yet, or a worktree without a `Cargo.toml` at its
root — and ends when the index daemon stops. A client shows no indicator for an empty stream, and
watches again later to see a load that starts afterwards.

The caller must own the session: the token resolves to an OS user and the session must exist under
that user's sessions base. A session of another user and one that does not exist answer the same
`NOT_FOUND`, so the answer does not reveal which ids exist; a malformed id is `INVALID_ARGUMENT`. A
session whose `.session.yaml` cannot be read is `FAILED_PRECONDITION`.

## The code session

`CodeSession` is the Code pane's one stream to its worktree's index, open for as long as the pane is.

**Request frames** (`CodeSessionRequest`, `oneof frame`):

| Frame | Meaning |
|---|---|
| `open` (`OpenCodeSession { session_token, project_id, worktree_path }`) | first frame, exactly once; authorised like the navigation calls. Opening warms the worktree's index |
| `query` (`CodeQuery { request_id, kind, rel_path, position }`) | `kind` is `CodeQueryKind`: `HOVER`, `DEFINITION`, `REFERENCES`, or `PROBE` ("is there a definition here?"). `request_id` is the client's; reusing one replaces the earlier query |
| `cancel` (`CancelCodeQuery { request_id }`) | drop a query, held or in flight; it is never answered |

A query before `open`, a second `open`, or an empty frame ends the stream `INVALID_ARGUMENT`.

**Events** (`CodeSessionEvent`, `oneof event`):

| Event | Meaning |
|---|---|
| `progress` (`CodeIndexProgress`) | the first event, then every change, for as long as the stream is open — including a reload after an edit |
| `held` (`CodeQueryHeld { request_id }`) | the query waits for the index to load; its answer follows once it has |
| `answer` (`CodeQueryAnswer { request_id, result }`) | `result` is one of `hover` (`HoverResponse`), `definition` (`DefinitionResponse`), `references` (`ReferencesResponse`), `probe` (`ProbeAnswer`) or `error` (a string) |

A hover, definition or references asked while the index is `LOADING` is held, then answered once it is
`READY`; a held query on an index that then fails is answered `error`. A probe is never held:
`ProbeAnswer { definability, locations }` with `Definability` `DEFINABLE` (and the definition's
locations), `NONE`, or `UNKNOWN` while the index loads. A worktree that is not a Rust worktree sends one
`NONE` progress with why and answers every query `error`. The host keeps the worktree's index — and the
process serving it — in use while the session is open. The stream ends `UNAVAILABLE` when the index stops
following the worktree; a client reopens it.

## Restructure plans

`OpenPlan`, `WatchPlan` and `RunPlan` let a client show and run a restructure plan (a `.jsonl` plan file
in the session's worktree) through the warm index. Their requests — `OpenPlanRequest`,
`WatchPlanRequest`, `RunPlanRequest` — are `{session_token, project_id, worktree_path, rel_path}`, where
`rel_path` is the plan file relative to the worktree.

`PlanSnapshot{rel_path, operations}` lists every operation in plan order as a `PlanOperation`:

| Field | Meaning |
|---|---|
| `id` | the operation's stable id in its plan; rows, run events and staleness are keyed by it. An operation the file gives no id is `op-<n>`, as the plan store names it |
| `index` | zero-based position in the plan |
| `op` | the kind as the plan spells it, e.g. `rename_symbol` |
| `item` | the item or symbol path the anchor names; empty for a range anchor. Several items are joined with `, ` |
| `file` | the anchor's file, relative to the worktree |
| `group` | the transactional group; empty when the operation stands alone |
| `status` | `PlanOperationStatus`: `PENDING`, `IN_FLIGHT`, `APPLIED`, `FAILED` or `ROLLED_BACK` |
| `stale_reason` | why the operation can no longer run as written — `item changed`, `item not found in <file>`, `edited by <plan>#<op>` — in the plan store's words; empty while it still points at its code |

`OpenPlan` loads the plan into the index's plan store for the worktree and answers one snapshot.
`WatchPlan` answers that snapshot first, then a new one whenever a status or a stale reason differs from
the last one sent, until the client goes away. The daemon never produces `ROLLED_BACK` in a snapshot: it
is reported by a run's `failure` event, and a client derives the row's state from it.

`RunPlan` applies the plan and streams `PlanRunEvent`, exactly one of:

| Event | Meaning |
|---|---|
| `operation` (`PlanOperationApplied`) | an operation landed: `op_id`, `index`, `done` of `total`, and the `files` it changed |
| `note` | a consequence an operation could not avoid, reported rather than left in the diff |
| `outcome` (`PlanRunOutcome`) | terminal: the run finished, `applied` of `total` |
| `failure` (`PlanRunFailure`) | terminal: the run stopped on an error — `message`, and when a transactional group did not compile, that `group` and its `rolled_back` operation ids |

A run always applies the whole plan from its first operation. It is refused while another run holds the
worktree (the index daemon's per-root queue), and the refusal reaches the caller with the index's status.
Indexing progress and findings the index emits during an apply are not part of the stream.

## Failure modes

A daemon configured without an `index_daemon:` section answers `Definition`, `References` and `Hover`
with `FAILED_PRECONDITION` naming that section, and refuses `CodeSession` the same way; there is no
fallback to another language server. (`WatchCodeIndex` instead ends empty, as above.) An index
daemon that cannot be started or dialled is `UNAVAILABLE`. The plan calls answer the same way. A plan
file that is not valid, or too large to read, is `FAILED_PRECONDITION`; a `rel_path` that is not a
listed file of the worktree is refused as the worktree service refuses it.

## Generation

`build.rs` runs the RPC-server pass for this proto, like `session_files.proto`, with no tonic adapter.
The TypeScript bindings are generated into `packages/tddy-web/src/gen/code_navigation_pb.ts` (and the
copy under `tddy-rust-typescript-tests/gen/`).
