# Changeset: sandboxed codebase mode from the web

**Date**: 2026-09-18
**Status**: 🚧 In Progress
**Type**: Feature

## Initial Discovery

[2026-09-18-sandboxed-codebase-mode-from-the-web-initial-discovery.md](./2026-09-18-sandboxed-codebase-mode-from-the-web-initial-discovery.md)

## Prerequisites

Items in `packages/*/docs/code-issues/` and [`docs/dev/todo/`](../todo/) this change runs into.

### 🚧 CLAIMED — `tddy-daemon`'s test suites are being moved out from under us

`packages/tddy-daemon/docs/code-issues/misplaced-tests-integration-suites.md`
`packages/tddy-daemon/docs/code-issues/heavy-dependency-tests-only-runtime-deps.md`

Both claimed by **#498** (`#carve` 4/10 `test-homes`, draft, `feature/carve/test-homes`), which
moves 122 of 139 suites to their owning crates and **deletes the `tddy_daemon::` re-export facade**.
`Lands after:` #488 ✅merged, #489 ✅merged, #490 ⏳open — one open predecessor.

**Developer decision: proceed on today's shape.** New daemon suites land in
`packages/tddy-daemon/tests/` beside the existing sandbox suites; #498 routes them to their owners
with the rest. The issue's own § *If you are about to change this code* pre-authorises exactly this
— *"Adding a test to `tddy-daemon/tests/`: fine, and usually right."*

⚠ **Constraint this places on `/green`**: new **production** code must not import through
`tddy_daemon::` re-export paths — #498 deletes that facade. Name the owning crate directly
(`tddy_session_lifecycle::`, `tddy_daemon_sandbox::`).

A `## Concurrent changes` line naming this changeset is appended to both issue records, so #498's
`/green` learns what arrived after it was planned.

### ⚠ DURING — the Linux jail shares the host filesystem root

[`2026-06-28-tddy-sandbox-cgroups.md`](../todo/2026-06-28-tddy-sandbox-cgroups.md)

The minimal read-only root with `pivot_root` is unbuilt, so a Linux jail confines **process and
network, not filesystem writes outside the checkout**. This change ships on Linux anyway (developer
decision), which makes the entry a constraint on *how*: the limitation is **stated in the feature
doc and surfaced in the UI**, never implied away by the word "sandboxed". Not fixed here — see
`## Scope` item 7 for what is shipped instead.

### ⚠ DURING — the daemon orphans its sandbox children on shutdown

[`2026-09-15-the-daemon-orphans-its-sandbox-children-on-shutdown.md`](../todo/2026-09-15-the-daemon-orphans-its-sandbox-children-on-shutdown.md)

`SandboxSessionState::stop()` is reached from `DeleteSession`, the workspace jail and the split
teardown — **never from the shutdown path** (`tddy-daemon/src/main.rs:145-168` kills CLI sessions
only). A jail started by this placement would survive the daemon that spawned it.

**Fixed here** — the developer chose "session-scoped, and fix the shutdown gap here". See
`## Scope` item 6. The narrow fix is wiring the workspace-jail registry teardown into the SIGTERM
path; the crash-detector / restart-policy half of the entry stays open and the entry is **narrowed,
not closed**.

### ⚠ DURING — cursor-cli cannot enforce this placement

[`2026-08-13-cursor-cli-cannot-enforce-managed-codebase-mode.md`](../todo/2026-08-13-cursor-cli-cannot-enforce-managed-codebase-mode.md)

`cursor-agent` has no `--disallowedTools` equivalent, so an inverted placement there would confine
nothing. This change refuses `session_type != "claude-cli"`, for the same reason split does.
Recorded, not fixed — the entry stays open.

### ℹ ANSWERED — "Linux `sandboxed` mode" was an open question

[`2026-09-05-from-2026-09-05-sandboxed-codebase-mode.md`](../todo/2026-09-05-from-2026-09-05-sandboxed-codebase-mode.md)

Its last bullet asks whether the daemon should carry a third codebase mode over
`StartSessionRequest` and provision the cgroups equivalent of the `--workspace-tools` jail.
**Answered**: not a third *mode*, but a third *placement* — `sandboxed_codebase = 39` — and the jail
is the one the daemon already provisions for a sandboxed `workspace` session, not a new one.

Its other four bullets (two in-jail dispatch implementations, cursor, workflow recipes on a jailed
checkout, the `NAME_MAX` build-home key) are **untouched** by this change: three are about the
`tddy-sandbox-app` path, and the build-home key does not exist on the daemon's jail. The entry is
narrowed at wrap, not deleted.

### ⚠ DURING — `CreateSessionPane.tsx` is 1412 lines

[`2026-08-19-session-creation-agent-catalog-the-rest-of-the-fan-out.md`](../todo/2026-08-19-session-creation-agent-catalog-the-rest-of-the-fan-out.md)
(the `CreateSessionPane.tsx is 1351 lines` bullet; now 1412)

This change adds one checkbox and one exclusivity rule. The entry's own guidance is followed: the
placement's *rules* go in a pure, separately-tested helper module, not inline in the pane — the
shape `CreateSessionSshConfigSelect`'s `sshConfigListDaemonId` set. The pane grows by the control
and its wiring only. **Not split here** — that is its own changeset, as the entry says.

### — Packages with no `docs/code-issues/` directory

`tddy-web`, `tddy-service`, `tddy-daemon-sandbox`, `tddy-sandbox`, `tddy-sandbox-cgroups`,
`tddy-sandbox-darwin`, `tddy-sandbox-runner`, `tddy-sandbox-recipes` have never been analyzed.
**Not analyzed is not clean.** `tddy-session-lifecycle` and `tddy-daemon` have directories; the
others are unmeasured going into this change. Worth an `/analyze-code-issues` pass on
`tddy-session-lifecycle` and `tddy-web` after green.

## Affected Packages

- **tddy-service**: [README.md](../../packages/tddy-service/README.md) — one field on
  `StartSessionRequest`
  - `proto/session.proto` — `bool sandboxed_codebase = 39;`
- **tddy-session-lifecycle**: [README.md](../../packages/tddy-session-lifecycle/README.md) — the
  placement, its validation, its start/resume/delete paths
  - `src/connection_service.rs` — `CodebasePlacement::SandboxedCodebase`,
    `classify_codebase_placement`
  - `src/split_session.rs` — the local tool env beside `split_remote_tool_env`
  - `src/connection_service/svc_start_session_core.rs` — the start path
  - `src/session_deletion.rs` — teardown over the pairing
- **tddy-daemon**: [README.md](../../packages/tddy-daemon/README.md) — shutdown teardown; new
  acceptance suites
  - `src/main.rs` — SIGTERM reaches the workspace-jail registry
- **tddy-daemon-sandbox**: [README.md](../../packages/tddy-daemon-sandbox/README.md) — consumed
  unchanged; named because the contract it exposes is now relied on by a second caller
- **tddy-web**: [README.md](../../packages/tddy-web/README.md) — the control and its rules
  - `src/components/sessions/CreateSessionPane.tsx`
  - `src/components/sessions/codebasePlacement.ts` (new — the pure rules)
  - `src/gen/session_pb.ts` (regenerated)

## Related Feature Documentation

- **PRD (this change)**:
  [PRD-2026-09-18-sandboxed-codebase-from-the-web.md](../../ft/daemon/amendments/PRD-2026-09-18-sandboxed-codebase-from-the-web.md)
- **Primary target**: [remote-managed-worktree.md](../../ft/daemon/remote-managed-worktree.md)
- **The mode being brought over**:
  [sandboxed-codebase-mode.md](../../ft/coder/sandboxed-codebase-mode.md)
- **The jail**: [remote-codebase-mode.md](../../ft/daemon/remote-codebase-mode.md) § Workspace tool
  sandbox
- **Prior amendment this builds on**:
  [PRD-2026-08-31-split-sandbox-orchestration.md](../../ft/daemon/amendments/PRD-2026-08-31-split-sandbox-orchestration.md)

## Summary

`tddy-web` gains a **Sandboxed codebase** placement: one daemon jails its own checkout in a
`--workspace-tools` jail and runs `claude-cli` beside it, unconfined, with every native filesystem
and shell tool withdrawn. It is implemented as the existing split orchestration with the peer hop
removed, so the jail, the routing, the resume and the teardown are consumed rather than rebuilt.

## Background

The inverted placement — jail the code, run the agent on the host — exists twice in this repo and
is reachable from neither the web nor one host:

- `tddy-sandbox-app --codebase-mode sandboxed` (#447) does it on **one host**, but only from the
  CLI and only on **macOS**; it refuses itself on the daemon-assisted Linux path because "the
  daemon would have to provision it, and does not yet know how to"
  (`packages/tddy-sandbox-app/src/codebase_mode.rs:105-120`).
- A **split** session with `sandbox = true` (2026-08-31) does it from the web, but across **two
  hosts** — it needs a second eligible daemon and a common LiveKit room.

So on one host the web can only produce a jail that holds the *agent*. The mode that is strictest
about the agent is the least strict about the build, and the build is the part you did not write.

The refusal's premise has expired. Since #427 and 2026-08-31 the daemon **does** provision a
`--workspace-tools` jail over a checkout and route `ExecuteTool` into it. What it has never done is
place the agent for that jail on the same host.

## Scope

- [x] 1. `bool sandboxed_codebase = 39` on `StartSessionRequest`, and the generated TS
- [x] 2. `CodebasePlacement::SandboxedCodebase` and its request validation (six refusals)
- [x] 3. The start path: a local `workspace` session with `sandbox: Some(true)`, its jail, and the
      agent spawned beside it with the withdrawal argv
- [x] 4. `colocated_jail_tool_env` — this daemon over HTTP, no LiveKit
- [x] 5. Resume and delete over the existing pairing fields
- [x] 6. **Shutdown teardown** — SIGTERM reaches the workspace-jail registry
      (`## Prerequisites` ⚠ daemon-orphans-children)
- [x] 7. The web control, its exclusivity rules, and capability gating that **states the reason**
- [x] 8. **The daemon advertises the capability on both paths it describes itself over** —
      `sandboxed_codebase: { confines_filesystem: bool }` on the common-room `DaemonAdvertisement`
      **and** on the serving daemon's own `/api/config` (`ClientConfig`) with its `GetClientConfig`
      RPC mirror, which the desktop reads. **Missed at planning time** (see
      `## Decisions & Trade-offs`); without the advertisement the control is disabled on every
      host, and without the serving payload it stays disabled on every daemon with **no common
      room** — which is the deployment AC6 is about and every acceptance fixture uses. Both ends
      read one function, `livekit_peer_discovery::sandboxed_codebase_support()`, so the two
      descriptions of one host cannot drift
- [ ] 9. Feature-doc updates, including the Linux confinement caveat

## Technical Changes

### State A

**The wire** carries two independent booleans and no notion of a codebase *mode*
(`grep -rn "codebase_mode" packages/tddy-daemon/src packages/tddy-session-lifecycle/src
packages/tddy-daemon-sandbox/src` → nothing):

```proto
  bool sandbox = 16;                          // session.proto:411-413
  bool managed_codebase = 17;                 // session.proto:414-417
  string codebase_daemon_instance_id = 32;    // session.proto:462-471
```

**Placement classification** — `packages/tddy-session-lifecycle/src/connection_service.rs:1002-1051`:

```rust
pub enum CodebasePlacement {
    CoLocated,
    Split { codebase_instance_id: String },
}

pub fn classify_codebase_placement(
    local_instance_id: &str, requested_codebase_id: &str, eligible_ids: &[String],
    managed_codebase: bool, session_type: &str,
) -> Result<CodebasePlacement, String> {
    let requested = requested_codebase_id.trim();
    if requested.is_empty() || requested == local_instance_id.trim() {
        return Ok(CodebasePlacement::CoLocated);
    }
    …
}
```

Its doc comment: *"An empty or self-matching id is co-located — the pre-existing behaviour, which
this must never change."*

**Tool routing** — `packages/tddy-session-lifecycle/src/connection_service/svc_resolve_os_user.rs:218-246`:

```rust
        let sandboxed_workspace =
            meta.session_type.as_deref() == Some("workspace") && meta.sandbox == Some(true);
        if !sandboxed_workspace {
            return seeded_clone_guard::ExecToolRoute::HostWorktree;
        }
        match self.workspace_sandboxes.get(session_id).await {
            Some(jail) => seeded_clone_guard::ExecToolRoute::Jail(jail),
            None => seeded_clone_guard::ExecToolRoute::Refused(…),
        }
```

**The split agent's tool env** — `packages/tddy-session-lifecycle/src/split_session.rs:442-474`,
with the comment that inverts for a co-located jail:

```rust
        // A split session has no HTTP route to its worktree: this daemon's own URL would answer,
        // but from the wrong host's filesystem. Left empty so the LiveKit transport is the only one
        // configured rather than a wrong one waiting behind it.
        daemon_url: String::new(),
        session_id: codebase_session_id.to_string(),
```

**Shutdown** — `packages/tddy-daemon/src/main.rs:145-168` wires SIGTERM to
`daemon.cli_sessions.kill_all()` and to nothing else. The workspace-jail registry is reached only
from `DeleteSession`, the workspace jail path and the split teardown.

**The web form** — `packages/tddy-web/src/components/sessions/CreateSessionPane.tsx:172-283` holds
`sandbox`, `managedCodebase`, `codebaseDaemonInstanceId` as independent state;
`isSplitCodebase` (`:243-276`) is the single predicate governing every withdrawal.

### State B

**A third placement, requested explicitly.**

```proto
  // Jail this session's own checkout and run the agent beside it, unconfined, with its native
  // filesystem and shell tools withdrawn — the inverted placement of
  // docs/ft/coder/sandboxed-codebase-mode.md, served by this daemon rather than by
  // tddy-sandbox-app. Requires session_type = "claude-cli". Mutually exclusive with
  // managed_codebase (which jails the agent instead), with sandbox (same), and with
  // codebase_daemon_instance_id (that is the same inversion across two hosts, already served).
  bool sandboxed_codebase = 39;
```

```rust
pub enum CodebasePlacement {
    CoLocated,
    Split { codebase_instance_id: String },
    /// Agent and worktree on this daemon, and the worktree inside a `--workspace-tools` jail the
    /// agent reaches only through `mcp__tddy-tools__*`.
    SandboxedCodebase,
}
```

**The self-match rule is unchanged.** `classify_codebase_placement` still answers `CoLocated` for an
empty and for a self-matching id. The new placement is a separate input, so nobody's existing
session changes meaning.

**Six refusals**, each naming both placements so an operator learns which one they got:

| Combination | Refused because |
|---|---|
| `sandboxed_codebase` + `managed_codebase` | both name a placement; `managed` jails the agent, this jails the code |
| `sandboxed_codebase` + `sandbox` | `sandbox` jails the agent on this daemon — the opposite placement |
| `sandboxed_codebase` + `codebase_daemon_instance_id` | that is the same inversion across two hosts, already served by split |
| `sandboxed_codebase` + `session_type != "claude-cli"` | no other agent's tool surface can be withdrawn |
| `sandboxed_codebase` + non-empty `recipe` | a recipe resolves `TDDY_REPO_DIR` where the agent is, which is not where the checkout is reachable |
| `sandboxed_codebase` + `dangerously_skip_permissions` | the placement confines only by withdrawing the agent's native tools, and this repo does not pin whether that deny list survives the bypass flag — the same reason a split withdraws it |

**The start path** is the split path with the peer hop removed:

1. Create a **local** `workspace` session holding the worktree, `sandbox: Some(true)` — the same
   `workspace_start_request` shape the split path forwards to B, served here.
2. Provision its jail through `JailedWorkspaceSandboxProvisioner` — unchanged.
3. Record the pairing on the agent half: `codebase_daemon_instance_id` = **this daemon's id**,
   `codebase_session_id` = the local workspace session's id. `split_pairing` then reads it
   unchanged, so resume and delete follow it for free.
4. Spawn `claude-cli` locally with the managed-codebase withdrawal argv, `cwd` = the split context
   directory (no repo), env from `colocated_jail_tool_env`.

**`exec_tool_route` is not touched.** The agent's `tddy-tools --mcp` addresses the *workspace*
session's id (`split_session.rs:427-429`), so the existing predicate —
`session_type == "workspace" && sandbox == Some(true)` — already answers `Jail`.

**`colocated_jail_tool_env`**, the one new seam, beside `split_remote_tool_env`:

```rust
RemoteToolEnv {
    // This daemon's own URL is the right one here: the checkout is on this filesystem, inside a
    // jail this daemon holds. The split builder blanks it for the opposite reason.
    daemon_url: this_daemon_url,
    session_id: workspace_session_id,
    session_token: agent_session_token,
    daemon_instance_id: None,
    livekit_url: None, livekit_room: None, server_identity: None, livekit_token: None,
}
```

No common room, no peer discovery, no join token — the placement works on a daemon with LiveKit
unconfigured.

**Shutdown** wires the workspace-jail registry into the SIGTERM path beside
`cli_sessions.kill_all()`.

**The web** gains `Sandboxed codebase` in the claude-cli branch. Its rules live in a pure module:

```ts
// packages/tddy-web/src/components/sessions/codebasePlacement.ts
export type CodebasePlacementChoice = "none" | "sandbox" | "managed" | "sandboxedCodebase";
export function placementAfterToggling(
  current: CodebasePlacementChoice, toggled: CodebasePlacementChoice, on: boolean,
): CodebasePlacementChoice;
export function sandboxedCodebaseUnavailability(host: DaemonHost): string | null;
```

Checking one clears the others **visibly in the form**, not silently at submit — the rule
`CreateSessionPane` already follows for `Managed codebase`: *"a selection the operator can no longer
see is one the form must no longer hold."*

On a host that cannot serve it, the control is `disabled` and renders the reason.

### Delta

| Package | File | Change |
|---|---|---|
| `tddy-service` | `proto/session.proto` | `+ bool sandboxed_codebase = 39;` with the mutual-exclusion comment |
| `tddy-session-lifecycle` | `src/connection_service.rs` | `CodebasePlacement::SandboxedCodebase`; `classify_placement` carries the six refusals in front of the unchanged `classify_codebase_placement` |
| `tddy-session-lifecycle` | `src/split_session.rs` | `+ colocated_jail_tool_env` |
| `tddy-session-lifecycle` | `src/connection_service/svc_start_session_core.rs` | the local-workspace + local-agent start branch |
| `tddy-session-lifecycle` | `src/session_deletion.rs` | pairing teardown reaches a local codebase session |
| `tddy-daemon` | `src/main.rs` | SIGTERM reaches `workspace_sandboxes` |
| `tddy-daemon-livekit` | `src/livekit_peer_discovery.rs` | `DaemonAdvertisement` gains `sandboxed_codebase: { confines_filesystem }`, beside `repos_base_path` and `max_attachment_bytes`. `confines_filesystem` is **true on macOS Seatbelt, false on the Linux cgroups jail** — that boolean is what the web turns into the caveat. `sandboxed_codebase_support()` becomes `pub`: it is now the single source both descriptions of this host read |
| `tddy-coder` | `src/web_server.rs` | `ClientConfig` gains `sandboxed_codebase: Option<ClientSandboxedCodebaseSupport>`, `skip_serializing_if` like its neighbours — the **serving daemon's own** self-description, which is the only source on a host with no common room. `src/run.rs` passes `None`: the standalone web server provisions no jail |
| `tddy-daemon` | `src/server.rs` | `RunServerOptions.sandboxed_codebase`, plus `serving_sandboxed_codebase_support()` — the one translation of the LiveKit crate's function into the web server's vocabulary, so no second `cfg!` restates the platform rule |
| `tddy-daemon` | `src/main.rs` | the composition root populates it |
| `tddy-service` | `proto/daemon_config.proto` | `GetClientConfigResponse.sandboxed_codebase = 9` + `message SandboxedCodebaseSupport` — the RPC mirror of `/api/config` the desktop reads, which has no HTTP origin to fetch the endpoint from |
| `tddy-daemon` | `src/daemon_config_service.rs` | serves it on that mirror, from the same function |
| `tddy-web` | `src/rpc/clientConfig.ts` | both transports read the capability into one shape; absent stays absent |
| `tddy-web` | `src/rpc/hostDirectory/servingSource.ts` | `useServingHostDirectorySource` takes the capability and puts it on its descriptor |
| `tddy-web` | `src/rpc/selectedDaemon.tsx`, `src/index.tsx` | `servingSandboxedCodebase` reaches the serving source from `/api/config` |
| `tddy-web` | `src/rpc/hostDirectory/{types,daemonHost}.ts` | the capability crosses the host-descriptor round trip |
| `tddy-web` | `src/gen/daemon_config_pb.ts`, `packages/tddy-rust-typescript-tests/gen/` | regenerated |
| `tddy-daemon-sandbox` | — | **no change**; consumed as-is |
| `tddy-web` | `src/components/sessions/codebasePlacement.ts` | **new** — the pure placement rules |
| `tddy-web` | `src/components/sessions/CreateSessionPane.tsx` | the control, the exclusivity wiring, the request field |
| `tddy-web` | `cypress/support/pages/createSessionPage.ts` | selectors for the new control |
| `tddy-web` | `src/gen/session_pb.ts` | regenerated |

## Implementation Milestones

- [ ] **M1 — the wire.** `sandboxed_codebase = 39` in the proto; `bun run build` regenerates
      `session_pb.ts`; `cargo build -p tddy-service` green.
- [ ] **M2 — the placement and its refusals.** `CodebasePlacement::SandboxedCodebase`;
      `classify_placement` unit tests green, including the unchanged self-match rule.
- [ ] **M3 — the local tool env.** `colocated_jail_tool_env` unit tests green: this daemon's URL,
      the workspace session id, no LiveKit field set.
- [ ] **M4 — the start path.** A sandboxed-codebase start produces a local `workspace` session with
      `sandbox: Some(true)`, a registered jail, an agent half with `sandbox: None` and the pairing
      recorded.
- [ ] **M5 — routing proven.** A tool call from that agent reaches the jail, and a session with no
      registered jail is refused rather than served from the host.
- [ ] **M6 — lifecycle.** Delete tears the jail down; resume re-provisions it; SIGTERM leaves no
      runner process behind.
- [ ] **M7 — confinement under a real jail.** Seatbelt acceptance: `Write` lands in the checkout,
      `Shell` cwd is the checkout, a read outside is refused by the jail.
- [ ] **M8 — the web.** The control, the three-way exclusivity, the disabled-with-reason state, and
      `sandboxedCodebase: true` on the wire.
- [ ] **M9 — docs.** `remote-managed-worktree.md` placement table; `sandboxed-codebase-mode.md`
      criterion 10; the Linux caveat; web changelog entry.

## Testing Plan

### Test level, per deliverable

| Deliverable | Level | Why not lower / higher |
|---|---|---|
| The six refusals | **Unit** (`classify_placement`) | A pure function over five inputs. An integration test would pay a daemon to assert a `match` arm. |
| The local tool env | **Unit** | Pure struct construction; the assertion that matters is *which fields are unset*, which reads best directly. |
| The web exclusivity rules | **Unit** (`codebasePlacement.ts`) + **Cypress component** | The algebra is exhaustively testable as a function; that the *form* obeys it is a rendering fact and needs the component. |
| Start / resume / delete | **Integration** (daemon acceptance) | Crosses session metadata, the jail registry and the spawner — the interaction is the subject. |
| Tool routing into the jail | **Integration** | `exec_tool_route` is not being changed; what must be proven is that the new session shape *reaches* it correctly. |
| Confinement | **Production** (Seatbelt acceptance) | The only level at which "the kernel refused it" is a fact rather than a mock's opinion. |
| Shutdown teardown | **Integration** | Needs a real child process to observe surviving or not. |

### Options considered

**(a) Prove the placement end-to-end through the web only.** Rejected — the existing e2e surface
does not cover session creation at all (`cypress/e2e/terminal-rendering.cy.ts` drives the form
incidentally, against a real daemon), and building that harness is a larger job than this feature.

**(b) Reuse `remote_managed_worktree_cross_host_acceptance.rs`.** Rejected as the *primary* home —
that suite needs a LiveKit testkit container and is currently blocked in this environment by a
WebRTC signalling timeout (recorded in `2026-08-31-split-sandbox-orchestration.md` § Validation).
This placement's whole point is that it needs **no** LiveKit, so its tests must not require one.
A single regression case is added there to prove the split path is unaffected.

**(c) Unit-test the placement, integration-test the lifecycle, production-test the confinement.**
**Chosen.** It keeps the fast feedback on the algebra, does not pretend a mock proves confinement,
and gives the one claim a reviewer will actually doubt — *"is the code really in a jail?"* — a test
at the only level that can answer it.

### Coverage requirements

- Every one of the six refusals has its own test, asserting the **message names both placements**
  — a refusal that says "invalid combination" teaches nobody which flag to drop.
- The unchanged self-match rule is pinned by a test that would fail if someone later "simplified"
  `classify_codebase_placement` by folding the new placement into the self-match branch.
- The negative routing case (sandboxed session, no jail ⇒ refused) is as important as the positive
  one: *"a tool that ran unconfined on a session that asked to be confined is the one failure
  nobody can see afterwards"* (`seeded_clone_guard.rs:130-134`).
- The web's exclusivity is tested in **both directions** — the new control clearing the old ones,
  and the old ones clearing it.

### Platform gating

The Seatbelt production suite is macOS-only, as its siblings are. Per
[`2026-09-12-the-in-jail-conversation-suite-runs-nowhere.md`](../todo/2026-09-12-the-in-jail-conversation-suite-runs-nowhere.md),
an inner `#![cfg(target_os = "macos")]` compiles to a **zero-test binary** on Linux CI, which reads
as a pass. This changeset's macOS-only suite therefore carries a Linux-side `#[ignore]` case whose
name says why, so the CI summary shows an ignored test rather than an empty binary.

## Acceptance Tests

### `tddy-daemon` — `tests/sandboxed_codebase_placement_acceptance.rs` (new, 16 tests)

`classify_placement` — the pure three-way decision (`classify_placement` and `PlacementRequest` are
`pub`, so this suite reaches them without a test-only accessor):

| Test | Validates |
|---|---|
| `a_sandboxed_codebase_request_is_classified_as_its_own_placement` | AC1 |
| `an_empty_codebase_host_is_still_co_located` | AC12 |
| `naming_this_daemon_as_the_codebase_host_is_still_co_located` | AC12 — the rule that must never change |
| `a_known_peer_is_still_a_split_placement` | regression on the cross-host form |
| `a_sandboxed_codebase_request_with_managed_codebase_is_refused_naming_both_placements` | AC7 |
| `a_sandboxed_codebase_request_with_the_agent_sandbox_is_refused_naming_both_placements` | AC8 |
| `a_sandboxed_codebase_request_with_a_codebase_host_is_refused_naming_the_split` | AC9 |
| `a_sandboxed_codebase_request_for_cursor_cli_is_refused_naming_the_withdrawable_tool_surface` | AC10 |
| `a_sandboxed_codebase_request_for_a_tool_session_is_refused_naming_the_session_type` | AC10 |
| `a_sandboxed_codebase_request_carrying_a_recipe_is_refused` | AC11 |
| `a_sandboxed_codebase_request_with_the_permission_bypass_is_refused_naming_both` | AC20 |

`StartSession` — what the placement persists, and `ExecuteTool` — where its tools run:

| Test | Validates |
|---|---|
| `a_sandboxed_codebase_start_places_the_worktree_in_a_local_workspace_session` | AC1 |
| `a_sandboxed_codebase_start_pairs_its_checkout_with_this_daemon` | AC1 — the pairing resume and delete follow |
| `a_sandboxed_codebase_start_leaves_its_agent_unjailed` | AC2 |
| `a_sandboxed_codebase_session_starts_with_no_common_room_configured` | AC6 — the fixture carries **no** discovery handles and names no room |
| `a_sandboxed_codebase_sessions_tool_call_is_served_by_its_jail` | AC4 |
| `a_sandboxed_codebase_sessions_tool_call_is_refused_when_its_jail_is_gone` | AC5 |
| `start_session_refuses_a_jailed_codebase_alongside_a_codebase_host` | AC9 at the RPC |
| `start_session_refuses_a_jailed_codebase_alongside_the_agent_sandbox` | AC8 at the RPC |
| `start_session_refuses_a_jailed_codebase_on_a_cursor_cli_session` | AC10 at the RPC |
| `start_session_refuses_a_jailed_codebase_alongside_the_permission_bypass` | AC20 at the RPC |

### `tddy-daemon` — `tests/sandboxed_codebase_lifecycle_acceptance.rs` (new, 5 tests)

| Test | Validates |
|---|---|
| `deleting_a_sandboxed_codebase_session_tears_its_jail_down` | AC13 |
| `deleting_a_sandboxed_codebase_session_removes_its_paired_checkout_session` | AC13 |
| `resuming_a_sandboxed_codebase_session_re_provisions_its_jail` | AC14 |
| `a_daemon_shutdown_leaves_no_sandbox_runner_behind` | `## Scope` 6 |
| `deleting_a_sandboxed_codebase_session_whose_checkout_is_already_gone_succeeds` | AC13 — the paired teardown is idempotent, so a checkout removed on its own does not strand its agent |

Liveness is the real pid from `workspace_tool_sandbox::RUNNER_PID_FILE`, probed with `kill(pid, 0)`.

### `tddy-daemon` — `tests/sandboxed_codebase_seatbelt_acceptance.rs` (new, 4 tests)

| Test | Validates |
|---|---|
| `a_write_from_a_jailed_codebase_session_lands_inside_the_checkout` | AC19 |
| `a_shell_from_a_jailed_codebase_session_runs_with_the_checkout_as_its_cwd` | AC19 |
| `a_read_outside_the_checkout_is_refused_by_the_tool_engines_path_containment` | pins the in-jail engine's own guard. Deliberately **not** AC19: `contain_path` rejects the path before any syscall, so this passes with no jail |
| `a_shell_cannot_reach_outside_the_checkout_either` | AC19 — the kernel's refusal, with its own positive control (a `cat` inside the checkout must succeed first, or a jail simply missing `cat` would pass) |

The fixture asserts **its own premise** before any test builds on it: the checkout is
`sandbox: Some(true)` *and* the agent is `sandbox: None`. Each test carries
`#[cfg_attr(not(target_os = "macos"), ignore = "…")]` rather than the file carrying
`#![cfg(target_os = "macos")]`, so on Linux CI they report as **ignored with a reason** instead of
compiling to a zero-test binary that reads as a pass —
[2026-09-12-the-in-jail-conversation-suite-runs-nowhere.md](../todo/2026-09-12-the-in-jail-conversation-suite-runs-nowhere.md).

### `tddy-session-lifecycle` — unit, in `src/split_session.rs`

`mod colocated_jail_tool_env_tests` — **red**, 6 tests. The env is a `pub` construction over no
I/O, and the assertion that matters is *which fields are left unset*, which an integration test can
only observe indirectly:

`a_jailed_codebase_agent_reaches_its_checkout_over_this_daemons_own_url` ·
`..._is_pointed_at_the_workspace_session_not_its_own` · `..._is_given_no_livekit_transport` ·
`..._names_no_peer_daemon` · `..._carries_its_own_minted_token` ·
`a_jailed_codebase_agents_env_exports_no_livekit_variables` (AC6)

`mod withdrawal_contract_tests` — **green on arrival**, 3 tests, and deliberately so. The
jailed-codebase agent reuses `split_claude_extra_args` verbatim, because a split agent needs the
same argv for the same reason. These pin the dependency so that narrowing
`NATIVE_FILESYSTEM_TOOLS`, or making the allowlist conditional, fails here rather than silently
un-confining a placement whose entire claim is that list:
`the_agent_argv_withdraws_every_native_route_to_the_host` ·
`the_agent_argv_keeps_the_mcp_tool_forms_it_is_left_with` ·
`the_agent_loads_no_mcp_configuration_but_its_own` (AC3)

### `tddy-web` — `cypress/component/CodebasePlacementRules.cy.ts` (new, 10 tests)

Pure-function tests over `codebasePlacement.ts`; nothing is mounted. Run as a Cypress spec because
that is this package's only working runner.

`chooses_the_jailed_codebase_placement_from_nothing_chosen` ·
`replaces_the_managed_placement_when_the_codebase_is_jailed` (AC16) ·
`replaces_the_agent_sandbox_when_the_codebase_is_jailed` (AC16) ·
`replaces_the_jailed_codebase_when_the_managed_placement_is_chosen` (AC17) ·
`replaces_the_jailed_codebase_when_the_agent_sandbox_is_chosen` (AC17) ·
`leaves_no_placement_when_the_only_chosen_one_is_unchecked` ·
`leaves_the_chosen_placement_alone_when_a_different_one_is_unchecked` ·
`serves_the_placement_on_a_host_that_advertises_it` ·
`serves_the_placement_on_a_host_whose_jail_shares_the_filesystem_root` ·
`names_the_host_when_it_does_not_advertise_the_placement_at_all` (AC18)

### `tddy-web` — `cypress/component/CreateSessionSandboxedCodebaseAcceptance.cy.tsx` (new, 12 tests)

| Test | Validates |
|---|---|
| `sends_a_sandboxed_codebase_placement_on_the_start_request` | AC15 |
| `clears_the_managed_codebase_placement_in_the_form_when_the_codebase_is_jailed` | AC16 — visibly, in the DOM |
| `clears_the_agent_sandbox_placement_in_the_form_when_the_codebase_is_jailed` | AC16 |
| `clears_the_sandboxed_codebase_placement_in_the_form_when_managed_codebase_is_chosen` | AC17 |
| `clears_the_sandboxed_codebase_placement_in_the_form_when_the_agent_sandbox_is_chosen` | AC17 |
| `stops_offering_to_skip_permissions_once_the_codebase_is_jailed` | AC20 at the UI |
| `sends_no_permission_bypass_for_a_sandboxed_codebase_session` | AC20 — the negative on the wire |
| `offers_no_sandboxed_codebase_placement_for_a_session_type_that_cannot_withdraw_its_tools` | AC10 at the UI |
| `disables_the_sandboxed_codebase_placement_and_states_why_on_a_host_that_cannot_serve_it` | AC18 |
| `never_submits_a_sandboxed_codebase_placement_a_disabled_control_could_not_have_chosen` | AC18 — the negative |
| `states_what_the_jail_does_not_confine_on_a_host_whose_jail_shares_the_filesystem_root` | the Linux honesty requirement |
| `states_no_caveat_on_a_host_whose_jail_confines_the_filesystem` | the negative of the above |

All Cypress specs use `mountWithRpc` + `anInMemoryRpcBackend` (never `cy.intercept`), selectors via
`createSessionPage`, Given/When/Then bodies, and a named
`theStartSessionRequest(backend)` helper for wire assertions.

## Technical Debt & Production Readiness

_Populated during development._

## Decisions & Trade-offs

**A boolean field, not a `codebase_mode` string.** The daemon has never had a mode concept; the wire
names *placements*. A string would let a typo become a silently-downgraded session — precisely the
outcome `tddy-sandbox-app`'s R3 refusal exists to prevent. Cost: a fourth placement would need a
fourth boolean and a fourth exclusion rule, at which point the enum becomes worth it.

**A self-split, not a port of the app's path.** Considered lifting `sandboxed_session.rs` +
`host_agent.rs` out of `tddy-sandbox-app` into a shared crate. Rejected: the daemon already runs
this exact shape across two hosts, and reusing it consumes the jail, the routing, the resume and the
teardown rather than growing second implementations of each. **What it costs**: no egress shim in
the jail (a jailed `cargo fetch` has no network) and no per-repository build `$HOME`. Both are named
in the PRD's out-of-scope list and become TODO entries.

**`exec_tool_route` deliberately untouched.** The temptation is to teach it about the new placement.
It does not need to know: the agent addresses the workspace session's id, and the existing predicate
already answers `Jail` for that. A change there would add a second way to reach the same decision.

**Linux ships with the caveat stated, not hidden.** The jail shares the host filesystem root, so it
confines process and network but not writes outside the checkout. The alternative — refusing Linux
until `pivot_root` is built — was weighed and declined by the developer. The honesty requirement is
therefore load-bearing: the UI and the feature doc both name which platform gives which guarantee,
and `/validate-prod-ready` should treat a missing caveat as a defect.

**The web rules live in a module, not in the pane.** `CreateSessionPane.tsx` is 1412 lines and on
record as needing a split of its own. Adding a three-way exclusivity rule inline would make that
split harder; a pure module makes the algebra exhaustively testable and the pane's growth the
control plus its wiring.

**Planning missed who writes the advertisement.** The PRD specified the web *reading*
`DaemonHost.sandboxedCodebase` and the changeset's Delta listed every consumer — but no package was
given the job of *emitting* it. Green found this: the web slice landed green against a key nothing
writes, so on a real daemon the control is disabled everywhere. Added as `## Scope` item 8. The
lesson is narrow and worth keeping: a capability-gated control needs the advertising end named in
the same Delta as the reading end, or the gate defaults closed and the tests still pass.

**Host capability is advertised, not inferred from platform.** `DaemonHost` gains
`sandboxedCodebase?: { confinesFilesystem: boolean }`, read from the daemon's common-room
advertisement. **Absent** means an older daemon that would answer an unrecognised field by starting
an ordinary session — so the control is disabled with a reason. **Present with
`confinesFilesystem: false`** is today's Linux cgroups jail — offered, with a caveat naming what it
does not confine. One field carries both the availability gate and the honesty requirement, and
neither is guessed from an OS string.

**Proceeding under #498's claim.** Recorded in `## Prerequisites`. The cost is a few more files for
that PR to route; the constraint is that no new production code may import through the
`tddy_daemon::` facade it deletes.

## Refactoring Needed

### Change-validation pass (2026-09-19) — seven findings, all repaired

| # | Finding | Repair |
|---|---|---|
| 1 | **`dangerously_skip_permissions` was not withdrawn for this placement.** The PRD says it stays withdrawn, but the web guard was `!isSplitCodebase` and choosing the jailed placement *clears* `codebaseDaemonInstanceId` — so the checkbox stayed enabled and the flag reached the spawn. No daemon-side refusal at all. | A **sixth refusal** in `classify_placement` (`dangerously_skip_permissions` on `PlacementRequest`, populated in `svc_start_session_core`), plus `placementWithdrawsPermissionBypass = isSplitCodebase \|\| sandboxedCodebase` governing both the control and the request field. Covered by two Rust tests and two Cypress cases |
| 2 | **`agent_session_token_for`'s `None` arm was a fallback and a test-only branch** — it forwarded the caller's token unverified, and was reachable only from the fixture (with `github:` and no secret, `tddy-daemon-auth` installs a resolver rejecting every token; with no `github:` the session host is never built) | The arm is now a `failed_precondition` naming `livekit.api_secret`. Its doc comment claimed "no token resolver is installed" — wrong, and corrected. The three new suites' configs gained a `livekit:` block with `api_secret` and mint a real caller token, so they exercise the **minting** path. `livekit.enabled` defaults false and no `common_room` is named, so no room is required — the no-room test is renamed to say exactly that |
| 3 | **The co-located paired delete was not idempotent.** `?` propagated, so a checkout already gone made the agent half permanently undeletable — while the cross-host arm forty lines below already treated not-found as success | The local arm is now symmetric with the remote one (`peer_has_no_such_session` ⇒ `log::info!`, continue). Pinned by `deleting_a_sandboxed_codebase_session_whose_checkout_is_already_gone_succeeds`, verified to fail without the fix |
| 4 | **An id mismatch stranded the checkout** — `svc_start_sandboxed_codebase_session` returned `Err` without tearing down the workspace session it had just created, leaking its worktree and jail | `tear_down_local_checkout_session(&workspace.session_id)` before the return — the id that actually exists, not the one that was asked for |
| 5 | **`stop_all` could panic mid-loop and orphan the rest.** `handle.lock().unwrap()` panics on a poisoned mutex, and the jails are already drained out of the registry, so nothing else can reach the ones after it | `unwrap_or_else(\|e\| e.into_inner())` in `stop()`; the sweep runs each stop on `spawn_blocking` (it kills and `wait`s, and the fallback sleeps 200 ms — blocking a tokio worker per jail), carries on past a failure, and logs what actually stopped |
| 6 | **The Seatbelt suite did not prove its headline claim.** `a_read_outside_the_checkout_is_refused_by_the_jail` was refused by `tddy-tool-engine`'s `contain_path` before the jail was consulted; the "readable off the jail" control was a tautology over `std::fs`; the Shell test had no positive control; and `unwrap_or_default()` / `unwrap_or(-1)` made the `Shell` assertions pass on a changed result shape | The `Read` test is renamed for what it actually pins and asserts the containment message; the tautology is deleted; the Shell test now `cat`s a file **inside** the checkout first and asserts exit 0 and its contents; the result parse uses `expect`-style panics |
| 7 | **Two weak assertions.** `error.contains("tool")` is satisfied statically by "tools"/"tool surface"; `contains("sandboxed_codebase") && contains("sandbox")` has a second conjunct implied by the first | `contains("\"tool\"")` for the session type, and `contains("mutually exclusive with sandbox:")` for the pair — a fragment only that refusal carries |

## Validation Results

### Red-phase verification (2026-09-18)

**Scoped to the packages touched.** No workspace build, no full Cypress run.

`./dev cargo check -p tddy-daemon --test <each>` and `-p tddy-session-lifecycle --lib --tests` —
every error is a piece of API these tests define, with no incidental breakage:

| Target | Missing API |
|---|---|
| `sandboxed_codebase_placement_acceptance` | `classify_placement`, `PlacementRequest`, `StartSessionRequest.sandboxed_codebase`, `CodebasePlacement::SandboxedCodebase` |
| `sandboxed_codebase_lifecycle_acceptance` | `StartSessionRequest.sandboxed_codebase`, `TestDaemon::shut_down_children` |
| `sandboxed_codebase_seatbelt_acceptance` | `StartSessionRequest.sandboxed_codebase` |
| `tddy-session-lifecycle` lib test | `split_session::colocated_jail_tool_env` |

Cypress, run **one spec at a time** (never the 207-spec suite):

- `CreateSessionSandboxedCodebaseAcceptance.cy.tsx` — **9 failing / 1 passing**, 40s. The failures
  are `Expected to find element: [data-testid='create-session-sandboxed-codebase-toggle'], but
  never found it` and `expected undefined to equal false` (the request field). The harness itself
  mounts and drives the form correctly, so the failures are about the missing feature, not the
  fixture.
- `CodebasePlacementRules.cy.ts` — **fails to load**: `Failed to fetch dynamically imported
  module`, because `src/components/sessions/codebasePlacement.ts` does not exist. Its 10 tests run
  only once green creates that module. This is the TypeScript analogue of a Rust compile failure;
  a weaker red signal than an assertion failure, and recorded as such.

### Green phase (2026-09-19) — all suites passing, scoped

| Suite | Result |
|---|---|
| `sandboxed_codebase_placement_acceptance` | **19/19** |
| `sandboxed_codebase_lifecycle_acceptance` | **4/4** |
| `sandboxed_codebase_seatbelt_acceptance` | **5/5** (real Seatbelt jail) |
| `tddy-session-lifecycle --lib` | 254/254 |
| `tddy-daemon-livekit --lib` | 70/70 |
| `CodebasePlacementRules.cy.ts` | 10/10 |
| `CreateSessionSandboxedCodebaseAcceptance.cy.tsx` | 10/10 |

Regressions, all green: `remote_managed_worktree_acceptance` 21, `workspace_tool_sandbox_acceptance`
15, `workspace_tool_sandbox_seatbelt_acceptance` 6, `split_session_resume_acceptance` 8,
`workspace_sandbox_resume_acceptance` 3, `claude_cli_session_acceptance` 12,
`daemon_advertisement_attachment_cap` 4. Plus 74 across 10 web create-session specs.
`cargo fmt --check` and `cargo clippy --all-targets -- -D warnings` clean on every touched crate.

**Whole-workspace health is CI's answer, not this one** — every run above was scoped with `-p`, and
the web specs were run one at a time.

### Refactor phase (2026-09-19) — re-verified after the seven repairs, scoped

| Suite | Result |
|---|---|
| `sandboxed_codebase_placement_acceptance` | **21/21** (+2: the sixth refusal at both levels) |
| `sandboxed_codebase_lifecycle_acceptance` | **5/5** (+1: the idempotent paired delete) |
| `sandboxed_codebase_seatbelt_acceptance` | **4/4** (−1: the deleted tautology; real Seatbelt jail) |
| `tddy-session-lifecycle --lib` | 254/254 |
| `tddy-daemon-livekit --lib` | 70/70 |
| `CreateSessionSandboxedCodebaseAcceptance.cy.tsx` | **12/12** (+2: the withheld bypass, and the wire) |
| `CodebasePlacementRules.cy.ts` | 10/10 |
| `CreateSessionCodebaseHostAcceptance.cy.tsx` | 21/21 |
| `CreateSessionSandboxToggle.cy.tsx` | 2/2 |

Regressions, all green: `remote_managed_worktree_acceptance` 21, `workspace_tool_sandbox_acceptance`
15, `workspace_tool_sandbox_seatbelt_acceptance` 6, `split_session_resume_acceptance` 8,
`workspace_sandbox_resume_acceptance` 3, `claude_cli_session_acceptance` 12,
`daemon_advertisement_attachment_cap` 4. `cargo clippy --all-targets -- -D warnings` and
`cargo fmt --check` clean on `tddy-daemon`, `tddy-session-lifecycle` and `tddy-daemon-sandbox`.

**Still open, deliberately.** `WorkspaceSandbox::stop()` is still called inline from
`tear_down_local_checkout_session` and from `session_deletion` — only the SIGTERM sweep
(`stop_all`) was moved onto the blocking pool, because those two are single-jail calls on a path
that is already awaiting I/O. And the id-mismatch teardown (finding 4) carries **no test**: the
branch needs `start_session_core` to answer with a session id other than the caller-chosen one,
which nothing short of a seam in the production path can produce.

### Blocker repair (2026-09-19) — the capability reached only the common-room path

The capability was published **only** in the LiveKit advertisement. On a daemon with no common room
the web's only host source is `useServingHostDirectorySource`, which built a descriptor from
`{ instanceId, label }` and carried no capability — so `sandboxedCodebaseUnavailability` disabled
the control, on exactly the deployment AC6 describes and every acceptance fixture uses. The Cypress
specs missed it because they inject `sandboxedCodebase` straight into the `DaemonHost` fixture,
bypassing both real sources.

Repaired by carrying the capability on the serving daemon's own self-description as well
(`## Scope` 8, and the Delta rows above). Re-verified, scoped:

| Suite | Result |
|---|---|
| `tddy-coder --lib web_server` | **7/7** (+3: the new key's wire shape and both polarities) |
| `tddy-daemon --test server_options_acceptance` | **11/11** (+5: `/api/config` carries it, omits it, and tracks the platform) |
| `tddy-daemon --test daemon_config_service` | **17/17** (+1: the RPC mirror matches `/api/config`) |
| `tddy-web` `ServingDaemonSandboxedCodebaseAcceptance.cy.tsx` | **7/7** (new — every case mounts with **no** LiveKit source) |
| `tddy-web` `bun test src/rpc` | **221/221** (+3 in `clientConfig.test.ts`, both transports) |

Regressions, all green: `sandboxed_codebase_placement_acceptance` 21,
`sandboxed_codebase_lifecycle_acceptance` 5, `sandboxed_codebase_seatbelt_acceptance` 4,
`daemon_advertisement_attachment_cap` 4, `tddy-session-lifecycle --lib` 254,
`tddy-daemon-livekit --lib` 70; `CodebasePlacementRules.cy.ts` 10,
`CreateSessionSandboxedCodebaseAcceptance.cy.tsx` 12, `CreateSessionCodebaseHostAcceptance.cy.tsx`
21, `HostDirectoryAcceptance.cy.tsx` 8, `DesktopIpcHostAcceptance.cy.tsx` 18.
`cargo clippy --all-targets -- -D warnings` and `cargo fmt --check` clean on `tddy-coder`,
`tddy-daemon` and `tddy-daemon-livekit`; `scripts/generated-code.sh check` reports every gen
directory up to date.

### ⚠ Four fixture defects in the red-phase tests, repaired during green

The implementation was right; the tests I shipped from `/plan-red` were not. Each was broken setup,
not a weakened assertion — no assertion was changed:

| Defect | Where | Repair |
|---|---|---|
| No project registered, so every start died `NotFound` | placement + lifecycle suites | a real git repo with a committed `README.md`, registered — the file the tool calls read. An empty repo let the jail answer *"file not found"* and the routing assertion would have passed for the wrong reason |
| Session dir spelled `sessions_base/<id>` | placement + lifecycle | it is `sessions_base/sessions/<id>` — `unified_session_dir_path`. My own Seatbelt suite already had this right, so the two disagreed |
| OS user `testuser` / `testdev` does not exist | all three | `current_os_user()`, the pattern `claude_cli_session_acceptance.rs:37` already uses. These are the first daemon suites to complete a claude-cli spawn, which is why nothing had hit it |
| `Write` arg spelled `content` | Seatbelt suite | the tool engine reads `contents` (`tddy-tool-engine/src/lib.rs:331`) |

A fifth, found while repairing: the repo fixture must be **idempotent**, because the restart tests
stand a second daemon on the same sessions base.

### ⚠ Tests that are green on arrival — deliberate, and listed so they are not mistaken for red

| Test | Why it passes today | Why it is still worth having |
|---|---|---|
| `offers_no_sandboxed_codebase_placement_for_a_session_type_that_cannot_withdraw_its_tools` | asserts the control does **not** exist for cursor-cli, and it does not exist for anything yet | becomes meaningful the moment green adds the control; without it, nothing stops the control being rendered for cursor-cli |
| `withdrawal_contract_tests` (3, in `split_session.rs`) | the jailed-codebase agent reuses `split_claude_extra_args` verbatim, which already withdraws the natives | pins the dependency, so narrowing `NATIVE_FILESYSTEM_TOOLS` fails here instead of silently un-confining this placement |

`/green` must treat all four as regression guards, not as work to make pass.

## TODO

- [x] Record initial discovery (`2026-09-18-sandboxed-codebase-mode-from-the-web-initial-discovery.md`)
- [x] Cross-check `packages/*/docs/code-issues/` and `docs/dev/todo/` for items this change touches (Step 2b)
- [x] Create/update PRD documentation
- [x] Create changeset (this document)
- [x] Create failing acceptance tests
- [x] Run acceptance tests (verify they fail) — `cargo check -p tddy-daemon --test <each>`, scoped; Cypress not yet run (no `node_modules` in this worktree)
- [ ] USER REVIEW — acceptance tests
- [x] TDD Red — write failing unit/integration tests
- [x] TDD Green — implement with quality code
- [ ] Update documentation with progress
- [ ] Repeat Red→Green→Update cycle until feature complete
- [ ] Run all tests (`./test`) — verify 100% pass
- [ ] Validate changes (/validate-changes)
- [ ] Refactor issues from change validation
- [ ] USER REVIEW — development complete
- [ ] Validate tests (/validate-tests)
- [ ] Refactor test issues
- [ ] Validate production readiness (/validate-prod-ready)
- [ ] Refactor production readiness issues
- [ ] Analyze code quality (/analyze-clean-code)
- [ ] Refactor code quality issues
- [ ] Final validation (/validate-changes)
- [ ] Linting and formatting (`cargo clippy -p <pkg> -- -D warnings`, `cargo fmt`)
- [ ] Wrap documentation (/wrap-context-docs) — also deletes `{slug}-initial-discovery.md`
- [ ] USER REVIEW — work complete, decide next steps

## Restructuring

### `packages/tddy-web` — the two TypeScript files the `/pr-wrap` file-length gate flagged

The `code-restructuring` skill is Rust-only (v1), so both were split **by hand** under the same
discipline: a green baseline first, mechanical moves only, no test file touched and no
`data-testid` changed, same green after. Every moved block was cut and pasted verbatim — the only
edits a move forced were the props/parameter list, the `import`/`export` lines and the indentation.
Each seam was cut and verified on its own pinning specs before the next was started, and every
Cypress run was a single `--spec` (the 207-spec suite was never run).

#### `src/index.tsx` — 534 → **250** ✅ under the 500 budget

The honest seam is the **standalone LiveKit connection screen** — everything `App` renders when
`/api/config` says this page is not served by a daemon (`daemonMode === false`). Its url, identity
and room come from query parameters rather than from a session, and the terminal it opens joins that
room directly with no daemon and no session behind it. Nothing in it is reachable from the
daemon-mode app and nothing in the daemon-mode app is reachable from it.

| New module | Lines | What moved |
|---|---|---|
| `src/components/connection/StandaloneConnectionScreen.tsx` | 304 | `getParamsFromUrl`, `pushParamsToUrl`, `ConnectedTerminal`, `ConnectionForm` (now exported) |
| `src/components/connection/standaloneFormStyles.ts` | 8 | `formClassName` / `inputClassName` / `labelClassName`, shared with the `DaemonLoginScreen` that stays behind |

`index.tsx` keeps what is genuinely the entry point: `HmrOverlay`, `DaemonLoginScreen`, `AppProps`,
`App` (the `/api/config` bootstrap, the local-host registration, the route dispatch and the provider
composition) and the `createRoot` mount. One pre-existing dead import (`useCallback`) was left in
place rather than tidied — it predates this change.

#### `src/components/sessions/CreateSessionPane.tsx` — 1412 → 1547 (this PR) → **752**

It is one component function with no module-level seams, so the split is **presentational
sub-components with explicit props, two data-loading hooks, and one pure function** — state left in
the pane, the shape `CreateSessionAgentSelect` and `CreateSessionSshConfigSelect` already set.

| New module | Lines | What moved |
|---|---|---|
| `createSessionRequest.ts` | 188 | `buildStartSessionRequest` — the three per-session-type `StartSessionRequestInit` branches, now pure over a named `CreateSessionFormValues` bag. Also owns the `SessionType` alias |
| `CreateSessionBranchFields.tsx` | 147 | Branch mode, Base branch, New branch name + Create Remote Branch, Branch to work on |
| `CreateSessionManagedCodebaseFields.tsx` | 130 | What the claude-cli **Managed codebase** toggle opens: Recipe, Codebase host, SSH host, the agent picker, Semantic index |
| `CreateSessionCursorCliFields.tsx` | 125 | The whole cursor-cli branch |
| `createSessionPaneProps.ts` | 108 | `CreateSessionInitialValues` and `CreateSessionPaneProps`, re-exported from the pane so every existing import keeps working |
| `CreateSessionToolFields.tsx` | 103 | The whole tool branch — Agent select, Recipe, "Base the stack on", Model |
| `useCreateSessionCatalogs.ts` | 95 | The mount load of sessions / projects / tools, with those three pieces of state. Effect body unchanged; the two auto-selections are handed back as callbacks, so they still fire at the same moment on the same inputs |
| `CreateSessionPermissionFields.tsx` | 94 | Permission mode, the permission bypass, the agent **Sandbox** placement |
| `CreateSessionAttachmentsSection.tsx` | 93 | The drop zone, the row list, the refusal line and the host-document picker. Takes the whole `useSessionAttachments` result rather than fifteen props |
| `useProjectBranches.ts` | 85 | The `ListProjectBranches` load and `remoteBranches`. Effect body unchanged, dependency array included — narrowing it would be a behaviour change, not a tidy-up |
| `CreateSessionHostAndProjectFields.tsx` | 77 | The Host and Project selects |
| `CreateSessionSandboxedCodebaseToggle.tsx` | 69 | The Sandboxed codebase control, its unavailable-reason line and its Linux caveat |
| `CreateSessionTypeToggle.tsx` | 59 | The Tool / Claude CLI / Cursor CLI buttons |
| `CreateSessionAgentPickerSection.tsx` | 59 | The specialized-agent multi-select shared by both managed-codebase blocks |
| `CreateSessionActions.tsx` | 53 | The refusal line and the Cancel / Create buttons |
| `CreateSessionModelField.tsx` | 50 | The Model select, its loading line and its probe error |
| `CreateSessionStackParentSelect.tsx` | 42 | The PR-stack parent picker |
| `createSessionRecipes.ts` | 11 | `WORKFLOW_RECIPES`, read by three of the new modules |

The extracted pure helper got its own unit test, in the style of `CodebasePlacementRules.cy.ts`:
**`cypress/component/CreateSessionRequestBuilder.cy.ts` (new, 14 tests)** — pure-function tests over
`buildStartSessionRequest`, nothing mounted. They pin what each session type sends and, more
usefully, what it deliberately sends *empty*: a tool session's stack-parent host, a base session
held past a recipe switch, a cursor-cli codebase host, a split session's recipe, and the permission
bypass under a placement that withdraws it — claims that until now could only be reached through the
DOM.

**Stopped at 752, deliberately — the remainder is not markup.** What is left is ~80 lines of
imports, ~175 lines of `useState` declarations whose doc comments carry wire contracts, ~110 lines
of derived placement and agent values, ~130 lines of submit / branch-conflict orchestration, and
~200 lines of JSX that is now almost entirely composition of the modules above. Every remaining
block would need a 20-to-32-prop interface to come out — the exact shape already recorded as a
defect on this file's sibling (`SessionMainPane.tsx`'s 42-prop interface,
[2026-09-12-the-acp-replay-framing-is-written-twice.md](../todo/2026-09-12-the-acp-replay-framing-is-written-twice.md)).
Going under 500 from here needs a `useCreateSessionForm` hook owning all ~30 state fields, which
rewrites every reference rather than relocating a block: not a mechanical move, and not safe to ride
on this one. **That is its own changeset**, and the entry in
[2026-08-19-session-creation-agent-catalog-the-rest-of-the-fan-out.md](../todo/2026-08-19-session-creation-agent-catalog-the-rest-of-the-fan-out.md)
should be narrowed to it rather than closed.

#### Verification — scoped, one spec at a time, never the 207-spec suite

Baseline taken before any edit; every seam re-verified on its own pinning specs before the next was
cut; a final pass over all 30 named specs, each its own `cypress run --spec`.

| | Specs | Tests | Pass | Fail |
|---|---|---|---|---|
| Baseline (before any edit) | 26 | 212 | 212 | 0 |
| Final (same 26) | 26 | 212 | 212 | 0 |
| New builder spec | 1 | 14 | 14 | 0 |
| Also run (touched code) — `CreateSessionAttachmentProgress`, `CreateSessionHostDocumentPicker`, `CreateSessionAgentPicker` | 3 | — | all pass | 0 |

`bun test src/rpc src/routing src/lib` — **637 pass / 0 fail**, before and after.

Two defects were caught by the per-seam runs and repaired before moving on — a JSX boundary that
left an orphaned `</div>` (host/project fields), and a hook call placed below a `useMemo` that read
its state (`Cannot access 'projects' before initialization`). Both were caught by the very next
spec run, which is the argument for cutting one seam at a time.

No test file was modified and no `data-testid` changed; the 212 unchanged assertions are the whole
proof that the moves were behaviour-preserving.
