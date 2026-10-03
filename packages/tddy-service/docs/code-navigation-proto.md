# `code_navigation.proto`

The web-facing contract for code navigation in a session's worktree. `package code_navigation`,
service `CodeNavigationService`; implemented by `tddy-daemon`
([how a request is served](../../tddy-daemon/docs/code-navigation-service.md)).

| RPC | Request → Response |
|---|---|
| `Definition` | `DefinitionRequest` → `locations: [CodeLocation]` |
| `References` | `ReferencesRequest` → `locations: [CodeLocation]` |
| `Hover` | `HoverRequest` → `optional markdown` |

Every request names its worktree the way `worktree.WorktreeService` does — `session_token`,
`project_id`, `worktree_path` — plus the `rel_path` of the file (relative to the worktree) and a
`position`. Authorisation is the worktree service's.

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

## Failure modes

A daemon configured without an `index_daemon:` section answers every method with
`FAILED_PRECONDITION` naming that section; there is no fallback to another language server. An index
daemon that cannot be started or dialled is `UNAVAILABLE`.

## Generation

`build.rs` runs the RPC-server pass for this proto, like `session_files.proto`, with no tonic adapter.
The TypeScript bindings are generated into `packages/tddy-web/src/gen/code_navigation_pb.ts` (and the
copy under `tddy-rust-typescript-tests/gen/`).
