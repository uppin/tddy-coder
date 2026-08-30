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
- [x] Step 4 — real jail provisioning, `in_jail_tool_*` frames, runner in-jail execution
- [x] **Blocker** — the darwin renderer's blanket `/var/folders` grant (see below)
- [ ] Mark PR ready for review

## Validation findings (`/pr-wrap`)

Gates: `cargo fmt --check`, `clippy -p tddy-daemon -p tddy-sandbox-runner --all-targets -D warnings`,
`check --workspace --all-targets` — all clean. No `TODO`/`FIXME` left behind, no test-only branches
in production code, one `unwrap` (a mutex-poison unwrap on the jail handle). Suites: 5 + 13 + 9
Seatbelt/acceptance/plan-unit, 600 daemon lib, 36 sandbox-runner, 3 + 22 + 18 e2e install — all
passing.

Two findings, neither fixed here:

**1. The jail outlives its session.** `WorkspaceSandboxRegistry::remove` is never called —
`connection_service.rs` only ever `insert`s (:4232) and `get`s (:8779). `JailedWorkspaceSandbox`
implements `Drop`, so the jail *would* stop when its `Arc` drops, but the registry holds that `Arc`
for the daemon's lifetime. So `DeleteSession` on a sandboxed workspace session removes the worktree
and leaves a jailed `tddy-sandbox-runner` process running against a directory that no longer
exists, until the daemon exits.

The Out table assigns `DeleteSession` teardown to `split-sandbox-resume`, which is why it was not
wired here. Worth reconsidering: a leaked *process* is a different class of gap from a missing
resume, and the teardown itself already exists — it is one `remove()` call on the session-deletion
path.

**2. The `/var/folders` grant** — fixed; see below.

## Closed hole — the jail is now "the worktree and nothing else"

`tddy-sandbox-darwin`'s `render_plan` used to grant **every** plan `(subpath "/var/folders")` for
both read and write (`profile.rs:48` and `:79`), plus `darwin_user_temp_base()` — `TMPDIR`'s
*grandparent*, which on a stock macOS host is the hashed bucket `/var/folders/36`, not even the
session's own directory. `tddy-sandbox`'s `system_baseline_reads()` granted a third one:
`(subpath "/private/var/folders")` as an "OS caches" read. So every jail — workspace tools,
`claude-cli`, `cursor-cli`, confined actions — could read and write every other session's scratch
and any other application's per-user temp files.

Demonstrated, not inferred: with `TMPDIR=$(getconf DARWIN_USER_TEMP_DIR)`,
`a_shell_tool_in_the_jail_cannot_climb_out_of_the_worktree_with_a_relative_path` failed with the
host file's contents in stdout. Under the nix dev shell's `TMPDIR=/tmp/nix-shell.…` it passed,
because the test's host file then landed outside the granted subpath — the suite was green for an
environmental reason.

All three grants are gone, and `darwin_user_temp_base()` with them. A jail's writable tree is now
its plan's own `project_root` + `scratch_dir` + `egress_dir` and its writable mounts; its readable
tree is that plus its declared `reads`. Nothing needed the removed grants: a confined process has
`HOME` and `TMPDIR` pointed into the plan's own scratch dir by `scratch_runner_env`, so its temp
files never land in the host's per-user base. Established by running every real-jail suite on macOS
under **both** TMPDIRs — including `a_strict_profile_still_lets_the_claude_binary_report_its_version`,
which boots the real `claude` binary in a jail rendered from the Claude recipe.

`profile.rs`'s unit test `rendered_plan_denies_writes_and_allows_the_project_tree` asserted
`profile.contains("/var/folders")` — the behaviour being removed. That assertion is dropped and
replaced by `rendered_profile_grants_no_part_of_the_host_per_user_temp_base`, which asserts the
inverse: a plan declaring no path under the per-user temp base renders no grant naming it, blanket
or `TMPDIR`-derived.

Not verified on this machine: `sandbox_behavior_acceptance`, `sandboxed_claude_cli_acceptance`,
`sandboxed_cursor_cli_acceptance` and `sandboxed_session_lifecycle_acceptance` fail before reaching
a jail on a pre-existing, unrelated `ConnectionServiceImpl::self_arc called before
set_self_handle`. A full interactive `claude-cli` and `cursor-cli` sandboxed session is therefore
unproven against the narrowed profile.
