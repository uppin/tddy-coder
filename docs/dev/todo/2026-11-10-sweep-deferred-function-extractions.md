# 2026-11-10 — Six `#agent-tools` sweep (#876) function extractions deferred: the engine refused the seams

**Category:** Known limitation (engine capability)

**Source:** #876 (`#agent-tools` sweep, Tier C) — the functions the changeset's plan named that the
restructure engine would not extract, deferred with the developer's consent (2026-11-10), not
closed. Standing records: `complexity-*` files under each package's `docs/code-issues/`.

Deferred by the user, 2026-11-10 (#876 sweep): the restructure engine refused the seam (missing imports for builder types / early returns in the range); revisit after the engine fix

## The six functions

| Function | Measurement (2026-11-10) | The engine gap that blocked it |
|---|---|---|
| `Runtime::build_session_host` — `packages/tddy-daemon/src/runtime/mount_host.rs:227` | 74 lines · depth 1 (was 90) | The tail builder chain offers only "extract into variable"; sub-expression extractions name `SplitRoomHosting`, `TerminalExposure` or `RoomTokenMinter`, which other files spell fully-qualified and this file does not import; the launcher expression's `approval_routes` is generated as `&_`. |
| `DaemonSessionHost::new` — `packages/tddy-session-lifecycle/src/connection_service/svc_host_builders.rs:33` | 97 lines · depth 1 (was 112) | The body is a ~78-line `Self { … }` builder literal; every field-expression range offers only "extract into variable", in both `extract_method` variants. |
| `CliSessionManager::spawn_tool_on` — `packages/tddy-cli-sessions/src/cli_session_manager/pty_spawn.rs:75` | 75 lines · depth 3 (was 104) | Every remaining seam (recording hook, task spawn, pending-exit match, handle build) names the exit hook's `Output`, which this file does not import. |
| `for_each_line` — `packages/tddy-supervisor/src/streamed.rs:182` | 23 lines · depth 5 | The per-chunk byte loop (the `split_chunk` seam) was refused three times: its range holds an early `return`, which the engine will not carry into an extracted function. |
| `forward_lines_until_exit` — `packages/tddy-supervisor/src/streamed.rs:112` | 20 lines · depth 5 | Extracting the loop to the function's tail names the generic `S` in the generated signature without declaring it; the single-line send seam holds an early `return`. |
| `UsersMapping::from_yaml` — `packages/tddy-users-file/src/lib.rs:98` | 41 lines · depth about 6 | Every seam that would flatten the row loop holds an early `return Err(…)`; the depth moved here from the supervisor's copy when #876 replaced it with this crate. |

Three gaps, all engine-side: generated signatures name types (and one generic) that are not
imported or declared in the file; an early `return` inside the range refuses the extraction; and a
builder literal offers only "extract into variable". The same family of defects is recorded for
`move_item` in
`docs/dev/todo/2026-11-09-restructure-move-item-defects-found-splitting-coding-subagent-session.md`
— read that one first; the fix there (re-resolve and widen what a move leaves behind) and the fix
here (spell fully-qualified names and declare generics in a generated signature) belong in the
same engine pass.

## What would close it

Engine fixes in `packages/tddy-code-restructuring`: generated signatures spell fully-qualified
trait names or add the import, declare every generic they name, and carry an early `return` out
of an extracted range as a returned value. Then re-anchor each function with
`tddy-tools restructure anchors` against a warm index (`./run-index-daemon`), prove each seam with
`restructure check --deep`, and delete the six `complexity-*` records with their final
measurements in the change-history entry.
