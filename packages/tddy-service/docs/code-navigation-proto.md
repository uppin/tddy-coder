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

The three navigation requests name their worktree the way `worktree.WorktreeService` does —
`session_token`, `project_id`, `worktree_path` — plus the `rel_path` of the file (relative to the
worktree) and a `position`. Authorisation is the worktree service's.
`WatchCodeIndex` is keyed by session instead, see [Watching a session's index warm-up](#watching-a-sessions-index-warm-up).

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

## Failure modes

A daemon configured without an `index_daemon:` section answers `Definition`, `References` and `Hover`
with `FAILED_PRECONDITION` naming that section; there is no fallback to another language server.
(`WatchCodeIndex` instead ends empty, as above.) An index
daemon that cannot be started or dialled is `UNAVAILABLE`.

## Generation

`build.rs` runs the RPC-server pass for this proto, like `session_files.proto`, with no tonic adapter.
The TypeScript bindings are generated into `packages/tddy-web/src/gen/code_navigation_pb.ts` (and the
copy under `tddy-rust-typescript-tests/gen/`).
