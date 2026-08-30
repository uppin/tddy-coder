# Changeset: Workspace tool sandbox — confine tool execution on the codebase host

**Date**: 2026-08-30
**Status**: 🚧 In Progress
**Type**: Feature

Root PR of the `split-codebase-sandbox` stack. Consumed by `split-sandbox-orchestration`,
`split-sandbox-resume` and `web-split-sandbox-toggle`.

## Problem

`sandbox = true` today means one thing: **spawn the agent inside a jail** on the daemon that runs
it (`start_sandboxed_claude_cli_session`). The agent is confined; the tools it calls are not run
where it is. An in-jail agent reaches files by asking the *host* to run the tool for it —
`DaemonToolHandler::execute` (`sandbox_session.rs:201`) calls `tool_engine::execute_tool_with_env`
against the host worktree, outside any jail.

For a co-located session that is sound: the jail already bounds the agent, and the tool engine
bounds each path to the worktree. It stops being sound the moment the codebase moves to another
host. On a split placement the agent's jail is on host A and the checkout is on host B, so the
thing the jail confines is not the thing that touches the repository. `run_exec_tool_locally`
(`connection_service.rs:8686`) runs every tool directly on B's filesystem, as B's daemon user, with
whatever that user can reach. That is why `StartSession` refuses `sandbox` together with
`codebase_daemon_instance_id` (`connection_service.rs:9171`) — there is nothing on the codebase
host for the flag to mean.

So the primitive is missing rather than the wiring: **there is no way to confine tool execution on
the host that holds the worktree.** A `workspace` session is exactly that host's half, and it
hardcodes `sandbox: None` in its metadata (`workspace_session.rs:152`).

## Change

`session_type: "workspace"` gains real sandbox semantics: a per-session jail on the codebase host,
holding the worktree and nothing else of that host, through which every exec tool runs.

### Start

- `StartSession` with `session_type = "workspace"` accepts `sandbox = true` and persists
  `sandbox: Some(true)` in the workspace `.session.yaml`.
- After the worktree is cut, the roster is seeded and the semantic index is built, the daemon
  provisions a per-session jail rooted at `<session_dir>/sandbox`, running
  `tddy-sandbox-runner --stdio` with the worktree mounted read-write.
- A platform with no sandbox backend is refused with `failed_precondition`. **No fallback to
  direct host execution** — a session that came up unconfined looks exactly like the session that
  was asked for.
- A jail that cannot be provisioned unwinds the seeded roster and leaves no session behind, the
  same way a failed semantic index already does.

### Tool dispatch

`run_exec_tool_locally` is the single choke point — `ExecuteTool`, `StreamExecuteTool` and roster
`local_agent_codebase_access` (`connection_service.rs:5085`) all funnel through it. It gains one
branch: when the session's metadata says sandboxed and a jail is registered for it, the call goes
to the jail instead of to `tool_engine::execute_tool` on the host worktree. All three callers are
covered by that one hook; none of them changes.

### Host → jail tool execution (new)

The `SessionChannel` carries tool calls in one direction today: **jail → host**
(`sandbox.proto:29`, `runner.rs:659`), which is the in-jail agent asking the host to run a tool for
it. The workspace jail needs the reverse, so `SessionFrame` gains two payloads:

| Field | Direction | Meaning |
|---|---|---|
| `in_jail_tool_request = 16` | host → jail | run this `ExecuteToolRequest` inside the jail |
| `in_jail_tool_response = 17` | jail → host | its `ExecuteToolResponse` |

`tddy-sandbox-runner` answers an `in_jail_tool_request` by running `tddy_tool_engine::execute_tool`
against the worktree **as mounted inside the jail**, so the confinement is the kernel's, not the
tool engine's path checks.

### New daemon module — `workspace_tool_sandbox.rs`

```rust
pub trait WorkspaceSandbox: Send + Sync {
    async fn execute_tool(&self, req: &ExecuteToolRequest) -> ExecuteToolResponse;
    fn stop(&self);
}

pub trait WorkspaceSandboxProvisioner: Send + Sync {
    async fn provision(&self, spec: &WorkspaceSandboxSpec)
        -> Result<Arc<dyn WorkspaceSandbox>, SandboxError>;
}

pub struct JailedWorkspaceSandboxProvisioner;   // the production one
pub struct WorkspaceSandboxLayout;              // rooted at <session_dir>/sandbox
pub struct WorkspaceSandboxRegistry;            // keyed by session id
pub fn build_workspace_tool_plan(WorkspaceToolPlanRequest) -> Result<SandboxPlan, SandboxError>;
pub fn workspace_sandbox_platform_support() -> Result<(), SandboxError>;
```

The provisioner is injected the way `HostStats`, `RoomRoster` and `EligibleDaemonSource` already
are — `ConnectionServiceImpl::with_workspace_sandbox_provisioner` — so the dispatch, refusal and
ordering contracts are testable without booting a jail, and the jail's *confinement* is proven
separately against a real one.

## Scope

### In

- `sandbox = true` accepted and persisted on `session_type: "workspace"` starts.
- Per-session jail provisioning on the codebase host (plan + spawn).
- `ExecuteTool` / `StreamExecuteTool` / roster `local_agent_codebase_access` routed through it.
- Semantic index built on the host worktree **before** the jail exists.
- `failed_precondition` on an unsupported platform; no session left behind on a failed provision.
- **Shipping `tddy-sandbox-runner`** from `./release`, `./install` and `publish.sh`. Not a
  pre-existing bug this PR merely noticed: the workspace jail spawns it, so on an installed host
  the feature cannot work without it. A developer checkout always has it in `target/debug` and
  `resolve_sandbox_runner_path` finds it as a sibling of `current_exe()`, which is exactly why
  three scripts could omit it unnoticed.
- Product-doc note for workspace-only sandbox semantics.

### Out

| Not here | Owner |
|---|---|
| Co-located `claude-cli` sandbox behavior (`start_sandboxed_claude_cli_session`) | unchanged |
| Removing the split+sandbox refusal; cross-host split orchestration | `split-sandbox-orchestration` |
| Re-provisioning the jail on resume; `DeleteSession` teardown | `split-sandbox-resume` |
| `CreateSessionPane` sandbox toggle, Cypress | `web-split-sandbox-toggle` |
| A new `StartSessionRequest` field — `sandbox` already exists | — |

### Deliberately narrowed

The PRD's acceptance criterion 2 reads "`ListSessions` / `.session.yaml` shows `sandbox: true`".
`SessionEntry` (connection.proto) **has no `sandbox` field** and nothing in this PR would consume
one — the only reader is the web session list, which `web-split-sandbox-toggle` owns. AC2 is
therefore met through `.session.yaml` alone; adding `SessionEntry.sandbox` belongs with the UI
that displays it.

## Acceptance criteria

1. Start workspace + `sandbox = true` → `ExecuteTool` `Shell`/`Write` succeeds through the jail,
   and cannot read or write a host path outside the worktree.
2. `.session.yaml` round-trips `sandbox: true` on the workspace session.
3. A seeded specialized agent on a sandboxed workspace session reaches files through the same jail.
4. An unsupported platform, or a jail that will not provision, returns `failed_precondition` and
   leaves no session behind.
5. Workspace-only sandbox semantics recorded in `docs/ft/daemon/remote-codebase-mode.md`.

## Tests

| File | Kind |
|---|---|
| `tddy-daemon/tests/workspace_tool_sandbox_acceptance.rs` | Dispatch, metadata, refusal, ordering — injected provisioner, platform-independent |
| `tddy-daemon/tests/workspace_tool_sandbox_plan_unit.rs` | Layout, plan mounts, platform gate |
| `tddy-daemon/tests/workspace_tool_sandbox_seatbelt_acceptance.rs` | Real macOS Seatbelt jail — confinement |
| `tddy-e2e/tests/sandbox_runner_shipping_acceptance.rs` | `./release` / `./install` / `publish.sh` ship the runner — fast, no VM |
| `tddy-e2e/tests/vm_workspace_tool_sandbox_acceptance.rs` | Real Linux cgroups jail in a QEMU guest (`#[ignore]`d, `./vm-tests`) |
| `connection_service.rs` § `workspace_sandbox_roster_dispatch_unit_tests` | Roster agent reaches the jail |

## Affected Packages

- **tddy-daemon**: `workspace_tool_sandbox.rs` (new), `workspace_session.rs`,
  `connection_service.rs`
- **tddy-service**: `proto/sandbox.proto` (`SessionFrame` += `in_jail_tool_request` /
  `in_jail_tool_response`)
- **tddy-sandbox-runner**: `runner.rs` — answer an `in_jail_tool_request` from the jail's own
  worktree
- **tddy-e2e**: runner-shipping suite (fast) and VM-backed cgroups suite
- **repo root**: `release` (build `-p tddy-sandbox-runner`), `install` (ship it beside `tddy-daemon`),
  `publish.sh` (verify + stage it into the `.deb`), `vm-tests` (register the new suite)

## Draft PR contract

Three dependent nodes branch off this ref, so the draft PR goes up **partway through the
implementation** — as soon as the API surface below is real and its failing tests are green — not
when the whole PR is finished. Dependents then code against a real signature while jail
provisioning hardens in the same PR.

Land first, in this order:

1. **Metadata.** `start_workspace_session` takes `sandbox: bool` and persists `sandbox: Some(true)`.
   Turns green: `a_workspace_session_started_with_sandbox_records_sandbox_true_in_its_metadata`
   plus the fixture precondition guarding all five Seatbelt tests.
2. **The API.** `workspace_tool_sandbox.rs` with `WorkspaceSandbox` /
   `WorkspaceSandboxProvisioner` / `WorkspaceSandboxSpec` (`Clone + Debug + PartialEq`),
   `WorkspaceSandboxLayout`, `WorkspaceSandboxRegistry`, and
   `ConnectionServiceImpl::with_workspace_sandbox_provisioner`. This is the surface dependents
   compile against; it must not change after the draft goes up.
   Turns green: `workspace_tool_sandbox_plan_unit.rs`, and the two suites stop failing to compile.
3. **The dispatch hook.** `run_exec_tool_locally` routes to the registered jail when metadata says
   sandboxed. One branch, covering all three callers.
   Turns green: the dispatch, refusal and ordering tests in
   `workspace_tool_sandbox_acceptance.rs`, and the roster tests in `connection_service.rs`.

**→ Open the draft PR here.** `split-sandbox-orchestration`, `split-sandbox-resume` and
`web-split-sandbox-toggle` branch off this ref.

Continuing in the same PR, after the draft is up:

4. Real jail provisioning — `JailedWorkspaceSandboxProvisioner`, the `in_jail_tool_*` frames, and
   the runner's in-jail execution. Turns green: `workspace_tool_sandbox_seatbelt_acceptance.rs`.
5. Shipping the runner from `release` / `install` / `publish.sh`. Turns green:
   `sandbox_runner_shipping_acceptance.rs`.

Steps 4 and 5 change no signature a dependent compiles against, which is what makes it safe to cut
the draft at step 3.

## TODO

- [x] Create/update PRD documentation
- [x] Create changeset
- [x] Acceptance tests (failing)
- [x] Unit/integration tests (failing)
- [x] Implementation steps 1–3 (`/green`)
- [x] Open draft PR — dependents branch off this ref (#427)
- [x] Step 5 — ship `tddy-sandbox-runner` from `release` / `install` / `publish.sh`
- [ ] Step 4 — real jail provisioning, `in_jail_tool_*` frames, runner in-jail execution
- [ ] Mark PR ready for review
