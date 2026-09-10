# PRD: Dissolve `ConnectionService` — the daemon becomes wiring

**Date**: 2026-09-10
**PRD Type**: Architecture Change (breaking RPC change — the last 17 methods, and one service deleted)
**Product Area**: daemon
**Stack**: `#unbundle` node 9 of 9 — the last, and the one that finishes the brief

## Affected Features

- [project-concept.md](../project-concept.md) — projects get their own crate and service
- [claude-cli-session.md](../claude-cli-session.md), [cursor-cli-session.md](../cursor-cli-session.md) — the session lifecycle moves
- [session-branch-conflict.md](../session-branch-conflict.md), [remote-managed-worktree.md](../remote-managed-worktree.md) — session start moves
- [session-auth.md](../session-auth.md) — `MintLocalToken` moves, but its peer-credential read does not
- [connection-service.md](../../../packages/tddy-daemon/docs/connection-service.md) — **the 931-line endpoint reference is deleted**, because the service is
- [rpc-playground.md](../rpc-playground.md) — `connection.ConnectionService` disappears from the picker
- [tddy-desktop-tauri.md](../../desktop/tddy-desktop-tauri.md) — the desktop embeds `runtime::build`, which now assembles services it does not own

## Summary

The last 17 methods leave, and with them the thing this whole stack existed to remove:
**`connection.proto` is deleted, `connection_service.rs` and its 59-file directory are deleted, and
`ConnectionServiceImpl` — the 60-field god object with the `Weak` self-handle — is dissolved.**

| Family | Methods | New coordinate | Served by |
|---|---:|---|---|
| C — sessions lifecycle | 8 | `session.SessionService` | `tddy-session-lifecycle` *(new)* |
| D — projects and branches | 5 | `project.ProjectService` | `tddy-projects` *(new)* |
| O — demo VM | 3 | `demo_vm.DemoVmService` | `tddy-vm` *(exists)* |
| Q — local peer-trust | 1 | `local_token.LocalTokenService` | `tddy-daemon-auth` *(node 4)* |

`tddy-daemon` ends at roughly **4,900 lines**: `main.rs`, `lib.rs`, `server.rs`, `startup.rs`,
`runtime.rs`, `config.rs`, its own settings service, the local socket server, and three small
utilities. That is the brief — *"leave them only for high-level wiring"* — reached rather than
approximated.

## Background

### Why this node exists at all

The eight-node plan stopped at 17 residual methods and argued they were "the answer, not a
compromise": a daemon that starts, resumes, signals and deletes sessions, owns projects, runs the
demo VM and mints a local token. Measuring what that actually left behind refutes the argument:

| After nodes 1–8 | Lines |
|---|---|
| Genuine wiring | 3,699 |
| Session lifecycle modules | 6,240 |
| Family C/D/O/Q handlers still in `connection_service/` (25 files) | 6,491 |
| UDS transport + the facade | 4,037 |
| Daemon's own config service + utilities | 1,012 |
| **Total** | **≈21,500** |

21,500 lines is not wiring, and — decisively — **`ConnectionServiceImpl` survives intact**, because
family C is precisely what needs its 60 fields, its 21 `with_*` builders and its `self_arc` handle.
The stack would have moved 73 of 90 methods and left the architectural defect it was aimed at.

### What dissolving the god object actually costs

`ConnectionServiceImpl` is not a service that happens to be large. It is the daemon's single
aggregation point:

- **60 fields** holding every registry in the daemon, injected by **21 `with_*` builders**;
- a **`Weak<ConnectionServiceImpl>` self-handle** (`self_arc`), set once after `Arc::new` in
  `runtime.rs`, because a `&self` handler must produce an `Arc<Self>` for
  `tddy_sandbox_runner::HostRpcHandler`. Three call sites use it, and the one recorded pre-existing
  test failure in this whole stack — `self_arc called before set_self_handle` — is a test that
  constructs it without that step;
- **7 trait implementations from 4 crates**, including two of `session_room`'s ports.

Dissolving it means each service crate owns the state its own family needs, and the sandbox-IPC
bridge — the only thing that genuinely needed `Arc<Self>` — moves to `tddy-daemon-sandbox`, where the
sandbox already lives. The pre-existing failure disappears because the thing it was failing on no
longer exists.

## Proposed Changes

### What changes

**`tddy-session-lifecycle`** *(new)* takes the nine session modules — `cli_session_manager.rs`,
`split_session.rs`, `session_list_enrichment.rs`, `session_deletion.rs`, `cursor_cli_spawn.rs`,
`workspace_session.rs`, `session_admission_service.rs`, `session_reader.rs` and the
`claude_cli_session.rs` shim — plus the 25 family-C handler files from `connection_service/`. It
serves `session.SessionService`.

It also becomes the owner of the `TaskRegistry`, which **originates in `CliSessionManager`** and was
only ever re-exposed through `ConnectionServiceImpl`. Five services take it from there today; after
this node they take it from the crate that creates it.

**`tddy-projects`** *(new)* takes `project_storage.rs` and `project_provision.rs` and serves
`project.ProjectService`. This is also the fix for a contradiction between two existing changesets —
see *Correction to node 1* in the changeset.

**`tddy-vm`** gains `demo_vm.DemoVmService`. It already serves `vm.VmService`, so the demo VM's three
methods land beside the VM machinery that answers them.

**`tddy-daemon-auth`** gains `local_token.LocalTokenService`. The **minting** moves; the
**peer-credential read does not** — `MintLocalToken` is answered from `SO_PEERCRED` on a Unix socket,
which only the transport can see. The daemon's socket server resolves uid → username as it does today
and passes the resolved identity in, so the credential check stays where the credential is and the
signing stays with the signer.

**Deleted outright**: `packages/tddy-service/proto/connection.proto`,
`packages/tddy-daemon/src/connection_service.rs`, all 59 files of
`packages/tddy-daemon/src/connection_service/`, `connection_tonic_adapter.rs`'s 1,437 hand-written
delegating methods, and `test_util.rs`. `packages/tddy-daemon/docs/connection-service.md` — 931 lines
— goes with them.

**`local_socket_server.rs` becomes a multi-service tonic server.** It serves one service today; after
this node the UDS socket carries whichever of the twelve services a local caller needs.

### What stays the same

- Every session, project, demo-VM and local-token behaviour.
- `tddy-daemon`'s own configuration service (`daemon_config.DaemonConfigService`). The daemon owns
  its own settings; that is wiring, not a subsystem.
- `runtime.rs`'s contract — *"assembly: it derives every service from configuration and returns the
  handles. Nothing that listens, dials or runs forever is started there."* It now assembles twelve
  services it does not implement, which is what it was always shaped for.

## Impact Analysis

### Technical

| Area | Impact |
|---|---|
| `tddy-daemon` | ≈21,500 → **≈4,900** lines. 11 modules and 59 handler files leave or are deleted; the god object is gone |
| `tddy-service` | `connection.proto` **deleted**; `session.proto`, `project.proto`, `demo_vm.proto`, `local_token.proto` appear |
| `tddy-daemon-sandbox` | gains the sandbox-IPC `HostRpcHandler` bridge, the only genuine user of `self_arc` |
| `tddy-vm`, `tddy-daemon-auth` | one service each |
| `tddy-web` | every session, project and demo-VM call site; `sessionManager.ts`, `CreateSessionPane.tsx`, `useSessionAttachment.ts`, `ProjectsAppPage.tsx`, `DemoVmControls.tsx`, and the 736-line Cypress `connectionServiceBackend.ts` fake is **deleted** along with the service it faked |
| `tddy-coder` | its session participant registered `connection.ConnectionService`; it now registers the services it actually serves |
| `tddy-desktop` | embeds `runtime::build`. **Outside the CI gate** |
| `tddy-daemon/tests/` | ~85 files reached `connection_service`, most via `test_util::test_service`; they move with their families |

### User-facing

The last 17 coordinates move and a service disappears. After this node **no client may address
`connection.ConnectionService` at all** — there is nothing to address.

## Implementation Plan

1. Four protos created; `connection.proto` deleted last, once nothing references it.
2. `tddy-projects` extracted (and node 1 stops claiming its two modules).
3. `tddy-vm` and `tddy-daemon-auth` gain their services.
4. `tddy-session-lifecycle` extracted, taking the `TaskRegistry` origin with it.
5. The sandbox-IPC bridge moves to `tddy-daemon-sandbox`; `self_arc` is deleted.
6. `ConnectionServiceImpl`, `connection_service/`, the tonic adapter and `test_util.rs` deleted.
7. `local_socket_server.rs` becomes multi-service.
8. `tddy-web` and `tddy-coder` migrated; the Cypress fake deleted.

## Acceptance Criteria

- [ ] `session.SessionService` serves all 8 family-C methods; `project.ProjectService` all 5;
      `demo_vm.DemoVmService` all 3; `local_token.LocalTokenService` its 1
- [ ] `MintLocalToken` still refuses a caller whose `SO_PEERCRED` uid resolves to no known user, and
      is still unavailable on any transport but the local socket
- [ ] `packages/tddy-service/proto/connection.proto` **does not exist**
- [ ] `grep -rn 'ConnectionServiceImpl\|connection.ConnectionService' packages/` finds nothing
- [ ] `self_arc` and `set_self_handle` do not exist; the recorded
      `self_arc called before set_self_handle` failure is gone because its subject is
- [ ] `tddy-daemon` is under 6,000 non-blank source lines, and every file in it is wiring, config, or
      its own settings service
- [ ] the UDS socket serves every service a local caller needs, not one
- [ ] `tddy-desktop` builds (verified locally — outside the CI gate)
- [ ] `./test` per touched package matches the recorded baseline

## References

- Changeset: [2026-09-10-unbundle-daemon-becomes-wiring.md](../../../docs/dev/1-WIP/2026-09-10-unbundle-daemon-becomes-wiring.md)
- Node 8's PRD, whose residual argument this node overturns: [PRD-2026-09-09-exec-prstack-services.md](./PRD-2026-09-09-exec-prstack-services.md)
