# `code_navigation.proto`

The web-facing contract for code navigation in a session's worktree. `package code_navigation`,
service `CodeNavigationService`; implemented by `tddy-daemon`
([how a request is served](../../tddy-daemon/docs/code-navigation-service.md)).

| RPC | Request → Response |
|---|---|
| `Definition` | `DefinitionRequest` → `locations: [CodeLocation]` |
| `References` | `ReferencesRequest` → `locations: [CodeLocation]` |
| `Hover` | `HoverRequest` → `optional markdown` |
| `WatchCodeIndex` | `WatchCodeIndexRequest` → stream of `CodeIndexProgress` |
| `OpenPlan` | `OpenPlanRequest` → `PlanSnapshot` |
| `WatchPlan` | `WatchPlanRequest` → stream of `PlanSnapshot` |
| `RunPlan` | `RunPlanRequest` → stream of `PlanRunEvent` |

The three navigation requests name their worktree the way `worktree.WorktreeService` does —
`session_token`, `project_id`, `worktree_path` — plus the `rel_path` of the file (relative to the
worktree) and a `position`. Authorisation is the worktree service's.
`WatchCodeIndex` is keyed by session instead, see [Watching a session's index warm-up](#watching-a-sessions-index-warm-up).
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

## Watching a session's index warm-up

`WatchCodeIndex(session_token, session_id)` streams the progress of the code-index warm-up the host
started for that session's worktree, for the session header's indexing indicator. It answers from
the session's own record, not from a worktree path, so a client needs nothing but the session id.

`CodeIndexProgress` is `code_index.IndexProgress` as the header reads it, plus the reason a warm
failed. It is declared here, not imported, because the web reads only this package's protos:

| Field | Meaning |
|---|---|
| `line` | the index's own one-line account of what it is doing |
| `phase` | the language server's progress phase, e.g. `Indexing` or `Loading` |
| `percentage` | the phase's progress, 0 to 100 |
| `furthest` | the highest percentage seen in any phase |
| `ready` | true on the last message of a warm that finished: the index is loaded and queryable |
| `error` | set on the last message of a warm that failed: why. The session itself is unaffected |

The stream opens with the latest progress when there is any, follows each change (a change replaces
the last one, so a slow reader sees the newest rather than every step), and ends after the message
carrying `ready` or `error`. A session nothing warmed — no `index_daemon:` section, or a worktree
without a `Cargo.toml` at its root — ends with no message. A client shows no indicator for that, so
a session without a warm index shows none.

The caller must own the session: the token resolves to an OS user and the session must exist under
that user's sessions base. A session of another user and one that does not exist answer the same
`NOT_FOUND`, so the answer does not reveal which ids exist; a malformed id is `INVALID_ARGUMENT`.

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
with `FAILED_PRECONDITION` naming that section; there is no fallback to another language server.
(`WatchCodeIndex` instead ends empty, as above.) An index
daemon that cannot be started or dialled is `UNAVAILABLE`. The plan calls answer the same way. A plan
file that is not valid, or too large to read, is `FAILED_PRECONDITION`; a `rel_path` that is not a
listed file of the worktree is refused as the worktree service refuses it.

## Generation

`build.rs` runs the RPC-server pass for this proto, like `session_files.proto`, with no tonic adapter.
The TypeScript bindings are generated into `packages/tddy-web/src/gen/code_navigation_pb.ts` (and the
copy under `tddy-rust-typescript-tests/gen/`).
