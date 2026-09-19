# Changeset: carve-rpc-handlers

**Date**: 2026-09-19
**Status**: 🚧 Planned
**Type**: Refactor (handler decomposition, no wire change)
**Stack**: `#carve` 11/11 — added on top of `pr-stack-crate`

## Affected Packages

- **`tddy-session-lifecycle`**: `DaemonSessionHost` loses four handler implementations and roughly a
  third of its surface; `connection_service/` loses ~4,000 lines.
- **`tddy-pr-stack`** (created by 10/11): gains `PrStackHandler`'s implementation.
- **`tddy-tool-engine`**: gains `ExecToolHandler`'s.
- **`tddy-discovery`**: gains `CatalogHandler`'s.
- **`tddy-projects`**: gains `ProjectHandler` / `ProjectService`'s.
- **`tddy-daemon-kernel`**: gains two small ports the handlers share.
- **`tddy-daemon`**: `runtime.rs` constructs four handlers instead of one.

## The finding this node rests on

`connection_service` reads as one enormous RPC service. It is not. The **wire protocol is already
decomposed**: `session.proto` (8 rpcs), `pr_stack.proto` (8), `project.proto` (5), `catalog.proto`
(4), `exec_tools.proto` (4) are separate services, and `runtime.rs` already builds
`SessionServiceImpl<H>` and `PrStackServiceImpl<H>` **generic over a handler**.

What is not decomposed is the Rust type that satisfies all of them. **`DaemonSessionHost` has 31
fields and 29 `impl` blocks, and implements 10 traits across 5 unrelated RPC families.**

So this node needs **no proto change, no wire change and no client change**. Each handler trait gets
its own type carrying only the fields it uses, and `runtime.rs` hands a different one to each
`*ServiceImpl`.

## Responsibility

Move four handler implementations to the crates that already own their domain:

| Handler | Destination | Fields it touches | Lines |
|---|---|---|---|
| `PrStackHandler` | `tddy-pr-stack` | **5** of 31 | 1,516 |
| `ExecToolHandler` | `tddy-tool-engine` | **13** of 31 | 1,311 |
| `CatalogHandler` | `tddy-discovery` | **7** of 31 | ~600 |
| `ProjectHandler` + `ProjectService` | `tddy-projects` | **6** of 31 | ~550 |

Each becomes its own struct in its destination crate, holding only what it needs. `runtime.rs`
constructs the four and passes each to the `*ServiceImpl` that already exists for it.

Extract the two behaviours all four share — `record_rpc_activity` (the idle-tracker bump) and
`resolve_os_user` (session token → OS user), both `pub(crate)` on `DaemonSessionHost` — as ports in
`tddy-daemon-kernel`, injected. Same pattern `8/11` uses for `PresenterObserverSpawner`.

Move each handler's test suites with it, using `move_test_binary_to_crate` from `#carve` 4/11.

## Boundaries

- Does **not** change any `.proto`, add an RPC family, or alter a method's wire signature. No client
  changes anywhere — `tddy-web`, `tddy-coder` and `tddy-sandbox-app` are untouched.
- Does **not** let a family leave the local socket.
  `tddy-daemon/tests/local_socket_reachability_acceptance.rs` must stay green **unmodified**; it is
  the guard that a decomposition keeps the wire surface whole.
- Does **not** touch `SessionHandler` / `SessionService`. That is the genuine connection service and
  it stays where it is.
- Does **not** carve the coordinate handlers, `daemon_rpc_handler` or `family_proto_bridge`. They are
  the dispatch layer *for* these services, not a separate responsibility, and they shrink on their
  own as the handlers leave.
- Does **not** carve the rooms/activity/attachments or hosts/users/worktrees groups. Neither is a
  distinct RPC family, so neither has a ready-made trait seam — inventing one is a different node.
- Does **not** fix the complexity of what it moves. See `## Prerequisites`.

## Dependencies

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `10/11` pr-stack-crate | the `tddy-pr-stack` crate, holding the PR-stack **data model** | `PrStackHandler`'s implementation lands **in that crate**, on top of the model. The crate must exist first — this is the hard ordering edge | re-create the data model, or move it |
| `8/11` telegram | removes the six Telegram modules and replaces `DaemonSessionHost`'s `telegram` field with a kernel port | the field is gone before this node partitions the remaining 31, and the **port pattern** this node reuses for `record_rpc_activity` / `resolve_os_user` is the one that node establishes | re-do the telegram extraction, or touch the port it added |
| `4/11` test-homes (merged) | `move_test_binary_to_crate` | each handler's suites travel with it | re-implement the operation |

## Draft PR contract

Published first: the four handler structs in their destination crates, with their fields and
constructors, and the two kernel ports — real signatures, `todo!()` bodies — plus the failing tests
that pin each handler's behaviour. This PR goes on to implement all of it. **It must not merge in
that state.**

## Green wave

**Wave:** after `10/11`
**Greenable independently:** **no** — `tddy-pr-stack` must exist, which is `10/11`'s delivery
**Concurrent with:** nothing; it is the top node
**Blocks:** nothing

## Prerequisites

| Entry | Verdict | What this node does with it |
|---|---|---|
| [`complexity-svc-exec-tool-ports-list-exec-tools.md`](../../packages/tddy-session-lifecycle/docs/code-issues/complexity-svc-exec-tool-ports-list-exec-tools.md) | ⚠ **MOVES** | The file it measures moves to `tddy-tool-engine`. The record is **renamed with a `**Moved:**` line**, not deleted — the finding is elsewhere, not gone |
| [`complexity-svc-exec-tool-ports-list-session-tool-calls.md`](../../packages/tddy-session-lifecycle/docs/code-issues/complexity-svc-exec-tool-ports-list-session-tool-calls.md) | ⚠ **MOVES** | as above |
| [`complexity-svc-exec-tool-ports-stream-execute-tool.md`](../../packages/tddy-session-lifecycle/docs/code-issues/complexity-svc-exec-tool-ports-stream-execute-tool.md) | ⚠ **MOVES** | as above |
| [`complexity-project-coordinate-handlers-add-project-to-host-at-project-coordinate.md`](../../packages/tddy-session-lifecycle/docs/code-issues/complexity-project-coordinate-handlers-add-project-to-host-at-project-coordinate.md) | ⚠ **DURING** | The coordinate handlers stay. **Annotate**; do not claim |
| `cycle-connection-service-telegram.md`, `oversized-file-telegram-session-control.md` | 🔒 **claimed by `8/11`** | An ancestor of this node owns both. Nothing to do here; recorded so the sequencing is explicit |

## Why the seams are real — verified, not assumed

- **Field types are foundation types, not `tddy-session-lifecycle` types**: `DaemonConfig` and
  `SessionUserResolver` (`tddy-daemon-kernel`), `PathBuf`, `ModelRegistryStore`
  (`tddy-model-registry`), `EligibleDaemonSource` (`tddy-host-service`), a LiveKit `Room`. **No
  destination crate needs a new edge to `tddy-session-lifecycle`.**
- **No cycle.** `tddy-tool-engine`'s existing edge back to `tddy-session-lifecycle` is
  **dev-only**, which cargo permits. `tddy-discovery` and `tddy-projects` have no edge back at all.
- **Orphan rule satisfied**: trait and type both become local to the destination crate.
- **`ProjectHandler` is pure delegation** — `project_service` plus five `*_at_project_coordinate`
  methods, no fields at all. It is the cheapest of the four and the one to do first.

## Risks

- The coupling that remains is **behavioural, not structural**: every one of the four calls
  `record_rpc_activity`, and three call `resolve_os_user`. If those ports turn out to need more of
  `DaemonSessionHost` than they appear to, the partition gets harder — that is the premise most
  likely to be wrong, and it should be proven on `ProjectHandler` first.
- `relaunch_sandboxed_runner` (CRAP 650) and `split_context_from_codebase_host` (CRAP 506) are
  **entirely untested** and live next door in the same directory. Neither is inside the four
  handlers, but a partition that disturbs them has no test to catch it.

## TODO

- [ ] Record initial discovery
- [ ] Create/update PRD documentation
- [x] Create changeset — this document
- [ ] Publish the draft-PR contract (handler structs + kernel ports + failing tests)
- [ ] Failing acceptance tests — **USER REVIEW**
- [ ] Implement production code making tests pass (`/green`)
- [ ] Relocate the three `svc_exec_tool_ports` complexity records
- [ ] `/validate-changes`
- [ ] `/pr-wrap`
- [ ] Add a changeset entry under `docs/dev/changesets/` (`/wrap-context-docs`)

## Verification

```bash
./test -p tddy-pr-stack -p tddy-tool-engine -p tddy-discovery -p tddy-projects
./test -p tddy-session-lifecycle
./test -p tddy-daemon -- local_socket_reachability
cargo clippy -p tddy-session-lifecycle -p tddy-tool-engine -p tddy-discovery -p tddy-projects -- -D warnings
```

**The load-bearing check is `local_socket_reachability_acceptance.rs`, unmodified.** Every other gate
can pass while a family has quietly left the local socket, which is a silent capability removal on a
privileged interface.
