# Changeset: dissolve `ConnectionService` — the daemon becomes wiring

**Date**: 2026-09-10
**Status**: 🚧 In Progress
**Type**: Architecture Change
**Stack**: `#unbundle` node **9 of 9** — the last. Base: `feature/unbundle/exec-prstack-services` (node 8)

## Initial Discovery

Full codebase exploration that grounded this plan:
[2026-09-10-unbundle-daemon-becomes-wiring-initial-discovery.md](./2026-09-10-unbundle-daemon-becomes-wiring-initial-discovery.md).

State A below is distilled from that file. Do not duplicate grep traces or item dumps here.

## Responsibility

The last 17 methods leave, and with them the thing this stack existed to remove.

| Family | Methods | New coordinate | Served by |
|---|---:|---|---|
| C — sessions lifecycle | 8 | `session.SessionService` | `tddy-session-lifecycle` *(new)* |
| D — projects and branches | 5 | `project.ProjectService` | `tddy-projects` *(new)* |
| O — demo VM | 3 | `demo_vm.DemoVmService` | `tddy-vm` *(exists)* |
| Q — local peer-trust | 1 | `local_token.LocalTokenService` | `tddy-daemon-auth` *(node 4)* |

**Deleted, not moved**: `connection.proto`, `connection_service.rs`, all 59 files of
`connection_service/`, `ConnectionServiceImpl` (60 fields, 21 `with_*` builders, the `Weak`
self-handle), `connection_tonic_adapter.rs`'s 1,437 hand-written delegating methods, `test_util.rs`,
and the 931-line `packages/tddy-daemon/docs/connection-service.md`.

`tddy-daemon` ends at ≈**4,900 lines**, all of it wiring, config, or its own settings service.

## Boundaries

This PR explicitly does **not**:

- Move `daemon_settings.rs` or `daemon_config_service.rs`. The daemon owns its own settings; that is
  wiring. `daemon_config.DaemonConfigService` is the one service `tddy-daemon` keeps implementing.
- Move `runtime.rs`, `config.rs`, `main.rs`, `lib.rs`, `server.rs`, `startup.rs`,
  `user_sessions_path.rs`, `tddy_user_config.rs` or `relay_idle.rs`. These **are** the endpoint.
- Change any behaviour. Every method answers as it does today, at a new coordinate.
- Move the **peer-credential read** for `MintLocalToken`. `SO_PEERCRED` is visible only to the
  transport, so `local_socket_server.rs` keeps resolving uid → username and passes the resolved
  identity in. The credential check stays where the credential is; the signing moves to the signer.
- Delete `connection.proto` before the other eight nodes have removed their methods from it. That
  ordering is a hard dependency, not a preference — see `## Green wave`.
- Renumber or re-shape `types.proto`. Its four types survive; only the *reason* they are shared
  changes, and that is recorded below.

## Dependencies

What each parent PR delivers that this PR consumes. These surfaces are **theirs to create**;
implementing one here collides with the PR that owns it.

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `n1` host-worktree-services | `move_module_to_crate`; `tddy-daemon-kernel` with the nine-symbol spawn preamble | every module move is a plan the operation executes; `cursor_cli_spawn.rs` reaches the preamble at 7 sites | add to the operation, or re-define a kernel symbol |
| `n3` sandbox-spawn-services | `tddy-daemon-sandbox` | the sandbox-IPC `HostRpcHandler` bridge moves **into** that crate — it is the only genuine user of `self_arc`, and its new home is where the sandbox already lives | create a second sandbox crate, or keep the bridge in the daemon |
| `n4` auth-livekit | `tddy-daemon-auth`, the identity boundary | it gains `local_token.LocalTokenService`; the minting joins the signer that already holds `config.livekit.api_secret` | add a second signer, or move the `SO_PEERCRED` read |
| `n6` session-io-services | `types.proto` with `HostDocumentScope` | `session.proto` imports it for `StartSession`'s staged attachments | redeclare it |
| `n7` session-agent-services | `types.proto`'s `SessionAgentStatus`, `SessionAgentActivity` | `session.proto` imports both for `ListSessions` | redeclare either |
| `n8` exec-prstack-services | `types.proto`'s `BranchSession` | `session.proto` imports it for `ListSessions`' branch views | redeclare it |
| **all of `n1`, `n4`, `n6`, `n7`, `n8`** | **their families removed from `connection.proto`** | this PR **deletes** `connection.proto`, which is impossible while any method is still declared on it | delete it early, or delete another node's methods on its behalf |

## Draft PR contract

What lands in this PR's **second commit**:

- `packages/tddy-service/proto/session.proto` (8 rpcs, 25 own messages), `project.proto` (5, 11),
  `demo_vm.proto` (3, 7) and `local_token.proto` (1, 2). **Measured, not assumed**: families C, D, O
  and Q share **nothing with each other**, and only family C reaches `types.proto` — all four of its
  types. So `session.proto` imports it and the other three import nothing.
- `packages/tddy-session-lifecycle/src/lib.rs` and `packages/tddy-projects/src/lib.rs` declaring each
  crate's entry constructor and trait ports with real signatures, bodies annotated
  `// TODO(daemon-becomes-wiring): implement`.
- `build_demo_vm_entry` in `tddy-vm` and `build_local_token_entry` in `tddy-daemon-auth`.
- **`TaskRegistry`'s new owner**: `tddy-session-lifecycle` exposes it, because it originates in
  `CliSessionManager` and was only ever re-exposed through `ConnectionServiceImpl`.
- The failing tests, including the ones that assert the deletions.

**This is the first push of a PR that goes on to implement the same thing. It must never merge in
that state.**

## Green wave

**Wave:** **4 of 4 — alone.**
**Greenable independently:** **no, and uniquely so.** Every other node in this stack can be greened
alongside at least one sibling. This one cannot be greened until **every** other node is green,
because it deletes `connection.proto` and a proto that still declares methods cannot be deleted. It
also consumes a surface from five different predecessors.
**Concurrent with:** nothing.
**Blocks:** nothing. It is the terminal node of the stack and of the effort.

Real dependency edges, as opposed to the branch line:

    n1 → n2, n3, n4, n5      n2 → n4      n5 → n6, n7, n8
    n1, n3, n4, n6, n7, n8 → n9

    w1  n1
    w2  n2, n3, n5          3 concurrent
    w3  n4, n6, n7, n8      4 concurrent
    w4  n9                  alone

⚠ **Recurring conflicts**: `packages/tddy-daemon/src/runtime.rs` (every node) and
`packages/tddy-coder/src/session_participant/mod.rs` (nodes 6, 7, 8 and this one — it registered
`connection.ConnectionService` and must now register the services it actually serves).

## Correction to node 1

**Node 1's `## Affected Packages` overclaims.** It lists `tddy-worktree-service` as taking "the
git/worktree subsystem", which discovery enumerated as 8 modules **including `project_storage.rs` and
`project_provision.rs`**. Node 8's `## Boundaries` simultaneously declares those two as staying with
family D. Two changesets contradict each other on two files.

Per the decision recorded here, **projects get their own crate**: `project_storage.rs` and
`project_provision.rs` come to `tddy-projects` in this node, and **node 1 must stop claiming them** —
its `tddy-worktree-service` takes 6 git/worktree modules, not 8.

That is a one-line fix to node 1's changeset, and it **could not be made from this branch**: node 1's
branch is checked out in another worktree, and the `pr-stack` model forbids editing a parent's
changeset from a child anyway. It must be applied on node 1's own branch by whoever greens it, and
node 1's `## Boundaries` should gain a line naming the two modules as this node's.

## Affected Packages

- **tddy-session-lifecycle** *(new)* — 9 session modules + 25 family-C handler files; owns `TaskRegistry`
- **tddy-projects** *(new)* — `project_storage.rs`, `project_provision.rs`
- **tddy-vm** — gains `demo_vm.DemoVmService`
- **tddy-daemon-auth** — gains `local_token.LocalTokenService`
- **tddy-daemon-sandbox** — gains the sandbox-IPC `HostRpcHandler` bridge
- **tddy-daemon**: [README.md](../../packages/tddy-daemon/README.md) — ≈21,500 → ≈4,900 lines
  - [connection-service.md](../../packages/tddy-daemon/docs/connection-service.md) — **931 lines, deleted**
- **tddy-service**: `connection.proto` **deleted**; four protos appear
- **tddy-coder**: `session_participant/mod.rs` — registers real services instead of `ConnectionService`
- **tddy-web**: every session, project and demo-VM call site; the 736-line Cypress `connectionServiceBackend.ts` fake **deleted** with the service it faked
- **tddy-desktop**: embeds `runtime::build`. **Outside the CI gate**

## Related Feature Documentation

- [PRD-2026-09-10-daemon-becomes-wiring.md](../../ft/daemon/1-WIP/PRD-2026-09-10-daemon-becomes-wiring.md)

## Summary

Families C, D, O and Q leave for `tddy-session-lifecycle`, `tddy-projects`, `tddy-vm` and
`tddy-daemon-auth`; `connection.proto` and `ConnectionServiceImpl` are deleted; and `tddy-daemon`
becomes the ≈4,900 lines of wiring the brief asked for.

## Background

See the PRD. The argument worth repeating: the eight-node plan called its 17 residual methods "the
answer, not a compromise", and measuring what they left behind refutes that. 21,500 lines is not
wiring, and **`ConnectionServiceImpl` survives intact in that endpoint** — because family C is
exactly what needs its 60 fields and its `self_arc` handle. The stack would have moved 73 of 90
methods and left the defect it was aimed at.

### `types.proto`'s reason changes, and that is worth saying

Node 6 created `types.proto` for `HostDocumentScope` on the grounds that it was reached by
`StartSession` — **a family that stayed**. Nodes 7 and 8 added three more types on the same grounds.
This node moves family C, so all four types are now shared between services that **all moved**.

The file's justification survives — four types, each reached by more than one service, each
established by walking field types — but the *reason* is no longer "a moved service and a staying
one". Leaving node 6's rationale standing unqualified would make the next reader think a staying
family still reaches them, and go looking for one.

## Prerequisites

Open items in [`docs/dev/todo/`](../todo/) this change runs into.

### ℹ ANSWERED — the `self_arc` pre-existing failure

Every changeset in this stack records
`cursor_cli_session_acceptance::cursor_cli_sandbox_start_succeeds_when_sandbox_backend_available`
failing with `ConnectionServiceImpl::self_arc called before set_self_handle`, carried forward as
pre-existing so it could not read as a regression. **This node ends it** — not by fixing the test but
by deleting its subject. `self_arc` exists because a `&self` handler must produce an `Arc<Self>` for
`tddy_sandbox_runner::HostRpcHandler`; with that bridge in `tddy-daemon-sandbox`, nothing needs it.
The baseline's expected failure count goes from 1 to 0, and that must be asserted rather than assumed.

### ⚠ DURING — `2026-07-01-tddy-daemon.md`

Records that the daemon's real session lifecycle should switch onto the stdio transport, and that the
remaining work is *"purely wiring the daemon's own spawn/dial call sites in `connection_service.rs`"*
— deferred because that file's orchestration was large. Node 3 moved `sandbox_session.rs`'s
`dial_and_bridge`; **this node deletes `connection_service.rs` entirely**, so the entry's stated
obstacle is gone and its call sites now live in `tddy-session-lifecycle`. Still not done here — it
changes live transport behaviour for every real session. Re-read and re-scope at wrap.

### ⚠ DURING — `2026-09-05-…-tauri-desktop-single-process-daemon.md`

`tddy-desktop` embeds `runtime::build`, which after this node assembles twelve services it does not
implement. It is **outside the CI gate**, so this node can break it with green checks. Built locally
and stated.

### ⚠ DURING — `2026-08-13-tddy-daemon-generalize-pr-stack-spawn-args-to-all-optional-spawn-flags.md`

In family C's spawn path, which moves here. The move must not flatten the optional-flag shape the
generalisation needs.

## Scope

- [ ] **Protos**: `session.proto`, `project.proto`, `demo_vm.proto`, `local_token.proto`
- [ ] **`tddy-session-lifecycle`**: crate, 9 modules, 25 handler files, owns `TaskRegistry`
- [ ] **`tddy-projects`**: crate, 2 modules (⚠ node 1 must stop claiming them)
- [ ] **`tddy-vm`**, **`tddy-daemon-auth`**: one service each
- [ ] **Sandbox-IPC bridge** moved to `tddy-daemon-sandbox`; `self_arc` and `set_self_handle` deleted
- [ ] **Deletions**: `connection.proto`, `connection_service.rs`, `connection_service/` (59 files),
      the tonic adapter's delegating methods, `test_util.rs`, `connection-service.md`
- [ ] **`local_socket_server.rs`** becomes a multi-service tonic server
- [ ] **Web**: call sites migrated; the 736-line Cypress fake deleted
- [ ] **`tddy-coder`**: registers real services instead of `ConnectionService`
- [ ] **Baseline**: the pre-existing `self_arc` failure is **gone**, asserted — expected failures 1 → 0
- [ ] **Endpoint**: `tddy-daemon` under 6,000 non-blank source lines, asserted
- [ ] **Code Quality**: `cargo clippy -p <each> -- -D warnings` clean, `cargo fmt` clean
- [ ] **Documentation**: doc triage executed at wrap

**Status indicators**: `[ ]` not started · `[~]` in progress · `[x]` complete ✅

## Technical Changes

### State A (after nodes 1–8)

`connection.ConnectionService` declares 17 methods. `ConnectionServiceImpl` has 60 fields, 21
`with_*` builders, a `Weak` self-handle with three `self_arc` call sites, and implements 7 traits from
4 crates. `connection_service/` is 59 files; 25 of them are family C/D/O/Q handlers. `TaskRegistry`
originates in `CliSessionManager` and is re-exposed through the god object to five services.
`local_socket_server.rs` serves exactly one service.

### State B

Twelve services, none implemented by `tddy-daemon`. `tddy-daemon` is `main.rs`, `lib.rs`, `server.rs`,
`startup.rs`, `runtime.rs`, `config.rs`, `daemon_settings.rs`, `daemon_config_service.rs`,
`local_socket_server.rs`, `user_sessions_path.rs`, `tddy_user_config.rs`, `relay_idle.rs` —
≈4,900 lines. No `connection.proto`, no god object, no `self_arc`.

### Delta

#### tddy-service
- **Proto**: four added; **`connection.proto` deleted**; `build.rs` loses its two `connection` passes
  and the descriptor-set entry

#### tddy-daemon
- **Architecture**: 11 modules and 59 handler files leave or are deleted; `lib.rs` drops to the endpoint set
- **Implementation**: `runtime.rs` assembles twelve services through their owners' constructors;
  `local_socket_server.rs` serves all of them

#### tddy-session-lifecycle, tddy-projects
- **Architecture**: new crates; the first owns `TaskRegistry`

#### tddy-daemon-sandbox
- **API**: gains the `HostRpcHandler` bridge, which needed `Arc<Self>` and now holds its own state

## Implementation Milestones

- [ ] M1 — four protos generate; `session.proto` imports all four shared types
- [ ] M2 — `tddy-projects` extracted; node 1's overclaim resolved
- [ ] M3 — `tddy-vm` and `tddy-daemon-auth` serve their services
- [ ] M4 — `tddy-session-lifecycle` extracted; `TaskRegistry` sourced from it
- [ ] M5 — the sandbox-IPC bridge moved; `self_arc` deleted; the pre-existing failure gone
- [ ] M6 — `connection.proto` and the god object deleted; nothing references either
- [ ] M7 — `local_socket_server.rs` multi-service; web and coder migrated; desktop built locally

## Testing Plan

**Primary test level: integration, per package.** The ~85 daemon test files that reached
`connection_service` move with their families; most did so via `test_util::test_service`, which is
deleted, so each moved suite constructs the service it actually drives.

Four assertions are new, because they are about *absence* and nothing that exists can carry them:

- **`connection.proto` does not exist**, and no source names `ConnectionServiceImpl` or
  `connection.ConnectionService`.
- **`self_arc` does not exist**, and the baseline's expected-failure count is **0** rather than 1.
  That number has been carried through nine changesets as pre-existing; this node is where it is
  claimed to have changed, so it is asserted.
- **`tddy-daemon` is under 6,000 non-blank source lines**, which is the brief expressed as a test.
- **`MintLocalToken` is still unavailable on any transport but the local socket**, and still refuses a
  uid that resolves to no known user. Moving the mint must not widen its reach.

## Acceptance Tests

### tddy-session-lifecycle
- [ ] **Integration**: all 8 `session.SessionService` methods answer over Connect-HTTP (`session_service_acceptance.rs`)
- [ ] **Integration**: a claude-cli session starts, is listed, resumed, signalled and deleted (`claude_cli_session_acceptance.rs`)
- [ ] **Integration**: `StreamStartSession` reports the same events at the same points as the old coordinate (`start_session_event_parity_acceptance.rs`)
- [ ] **Unit**: `TaskRegistry` is obtained from this crate, and `tddy-daemon` is absent from its dependency path (`task_registry_owner_unit.rs`)

### tddy-projects
- [ ] **Integration**: all 5 `project.ProjectService` methods answer (`project_service_acceptance.rs`)
- [ ] **Integration**: a project is created, gains a host, and reports its branches (`project_lifecycle_acceptance.rs`)

### tddy-vm / tddy-daemon-auth
- [ ] **Integration**: `demo_vm.DemoVmService` starts, reports and stops a demo VM (`demo_vm_service_acceptance.rs`)
- [ ] **Integration**: `MintLocalToken` mints for a resolved uid and refuses an unresolved one (`local_token_acceptance.rs`)
- [ ] **Integration**: `MintLocalToken` is unavailable over Connect-HTTP and LiveKit (`local_token_transport_acceptance.rs`)

### tddy-daemon-sandbox
- [ ] **Integration**: the sandbox-IPC bridge serves a jailed tool call without any `Arc<Self>` handle (`host_rpc_handler_acceptance.rs`)

### tddy-service
- [ ] **Unit**: `proto/connection.proto` does not exist (`unbundle_service_split.rs`)
- [ ] **Unit**: no source names `ConnectionServiceImpl` or `connection.ConnectionService` (`unbundle_service_split.rs`)

### tddy-daemon
- [ ] **Unit**: `self_arc` and `set_self_handle` do not exist (`unbundle_endpoint.rs`)
- [ ] **Unit**: the crate is under 6,000 non-blank source lines (`unbundle_endpoint.rs`)
- [ ] **Integration**: the local socket serves more than one service (`local_socket_multi_service_acceptance.rs`)

## Decisions & Trade-offs

- **One node for 17 methods, two crates and a god-object dissolution.** Three nodes were on the table
  and one was chosen: the deletion of `connection.proto` is only possible once every family has left,
  so splitting family C from the deletion would produce a node whose whole diff is removals and a
  predecessor that cannot be verified without it. The cost is the largest diff in the stack —
  ~14,000 lines of moves plus the deletions — in the subsystem where a mistake is hardest to spot.
  The mitigations are that the moved suites come with the code, and that four absence-assertions pin
  the endpoint rather than a changeset claiming it.
- **Projects get their own crate rather than joining `tddy-worktree-service`.** They are adjacent —
  a worktree belongs to a project — but a project outlives every worktree cut from it, and
  `ListProjectBranches` reads the project's *main checkout*, not a worktree. A separate crate also
  makes node 1's overclaim resolvable by subtraction rather than by argument.
- **`MintLocalToken` splits across two crates, deliberately.** The `SO_PEERCRED` read stays with the
  transport because only the transport can see it; the signing moves to the crate that already holds
  the signing secret. A single home for both would either put credential-reading in a library that
  cannot see the socket, or put signing in the daemon this stack exists to empty.
- **`TaskRegistry` moves to its origin.** It is created by `CliSessionManager` and was re-exposed
  through `ConnectionServiceImpl` to five services. Dissolving the god object without moving it would
  leave five crates reaching into the daemon for a registry the daemon does not own.
- **`daemon_config.DaemonConfigService` stays.** A daemon that cannot serve its own settings is not
  wiring, it is a shell. This is the one service `tddy-daemon` keeps implementing, and it is about
  the daemon itself rather than about sessions.

## Technical Debt & Production Readiness

- [ ] `tddy-desktop` is outside the CI gate and embeds `runtime::build`; the local build result is
      stated rather than assumed
- [ ] `2026-07-01-tddy-daemon.md`'s stdio-transport switch loses its stated obstacle here but is not
      done; re-scope at wrap
- [ ] `local_socket_server.rs` becomes multi-service, which is new behaviour on a privileged socket —
      the transport-restriction test for `MintLocalToken` is what stops that widening reach

## Baseline

| Gate | Before | After |
|---|---|---|
| `./test -p tddy-daemon` | inherited: **1027 passed / 1 failed** | **expected 0 failed** — the `self_arc` subject is deleted |
| `cargo clippy -p tddy-session-lifecycle -p tddy-projects -p tddy-vm -p tddy-daemon-auth -p tddy-service --all-targets -- -D warnings` | ✅ exit 0 | |
| `cargo check -p tddy-daemon` | ✅ clean with the two new members | |
| `tddy-daemon` non-blank source lines | ≈21,500 | **< 6,000** |

**26 failing tests** define this node: 2 in `tddy-session-lifecycle`, 3 in `tddy-projects`, 1 in
`tddy-vm`, 5 in `tddy-daemon-auth`, 11 in `tddy-service` (the inherited ones plus the four services'
shape, the shared-type imports, and **`connection.proto` still existing**), and 4 in `tddy-daemon`'s
new `unbundle_endpoint.rs`.

Those last four are the brief expressed as tests rather than claimed in prose: the crate is under
6,000 non-blank lines; `connection_service.rs` and its directory are gone; `self_arc` and
`set_self_handle` do not exist; and **every module left is one of the twelve endpoint files** — a
whitelist rather than a line count, because "under 6,000 lines" would still pass if a session module
stayed and something else left instead.

## A mistake made and undone during the red phase

Reaching for `tddy-daemon-auth`'s greened version with
`git checkout origin/feature/unbundle/auth-livekit -- packages/tddy-daemon-auth` pulled **node 4's
implementation onto this branch** — 17 files, 3,629 lines of a predecessor's work staged into node
9's diff. The compiler caught it (`unresolved import tddy_daemon_kernel::config`: node 4's greened
crate expects a kernel symbol that does not exist on this stale red chain), and it was reverted with
`git reset` + `git clean`.

Worth recording because `git checkout <branch> -- <path>` **stages** what it pulls, so
`git clean` alone does not remove it and `git checkout HEAD -- <path>` reverts only the *modified*
files, leaving the *added* ones staged and invisible to a casual `git status` read. This is the
`pr-stack` model's "never implement a symbol another node owns" hazard arriving through a git
convenience rather than through typing.

`build_local_token_entry` was then appended to the version this branch actually inherits.

## Final Checklist

- [ ] `docs/dev/changesets/2026-09-10-unbundle-daemon-becomes-wiring.md` — the release-note file,
      carrying the whole stack's 90 → 0 arithmetic and the god object's deletion
- [ ] **Delete** `packages/tddy-daemon/docs/connection-service.md` (931 lines)
- [ ] Move the session and project docs to the new crates' `docs/`
- [ ] `docs/ft/daemon/rpc-playground.md` and any diagram naming `connection.ConnectionService` as the
      daemon's RPC surface
- [ ] Re-scope `docs/dev/todo/2026-07-01-tddy-daemon.md` now that `connection_service.rs` is gone
- [ ] Close the `self_arc` pre-existing-failure note carried by all nine changesets
- [ ] Doc triage: `grep -rn -e 'ConnectionService' -e 'connection_service' packages docs`
