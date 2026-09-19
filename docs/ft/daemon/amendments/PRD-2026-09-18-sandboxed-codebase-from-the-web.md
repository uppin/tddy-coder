# PRD Amendment: sandboxed codebase placement from the web

**Date**: 2026-09-18
**PRD Type**: Enhancement

Amends [`docs/ft/daemon/remote-managed-worktree.md`](../remote-managed-worktree.md) (State A — the
placement table and the split orchestration).

## Affected Features

- **Primary**: [remote-managed-worktree.md](../remote-managed-worktree.md) — gains a third codebase
  placement beside `CoLocated` and `Split`, and the wire field that requests it.
- **Primary**: [sandboxed-codebase-mode.md](../../coder/sandboxed-codebase-mode.md) — the mode this
  brings to the daemon. Its criterion 10 ("`sandboxed` on Linux is refused, naming macOS") is
  superseded for the **daemon-served** form; the `tddy-sandbox-app` CLI's own refusal is unchanged.
- **Related**: [remote-codebase-mode.md](../remote-codebase-mode.md) § Workspace tool sandbox — the
  jail this reuses, now provisioned for a co-located agent as well as a remote one.
- **Related**: [session-drawer.md](../../web/session-drawer.md) — the create-session form gains one
  control; a sandboxed-codebase session is badged like a split one.
- **Related**: [managed-codebase-subagents.md](../../coder/managed-codebase-subagents.md) — the
  `managed` mode this sits beside and is mutually exclusive with.

## Summary

`tddy-web` can start a session whose **codebase is jailed and whose agent is not**. Today that shape
is reachable from the web only across two hosts (a split placement with `sandbox = true`), and on
one host only from the `tddy-sandbox-app` CLI, only on macOS, via `--codebase-mode sandboxed`.

This adds a **co-located sandboxed codebase placement**: one daemon provisions a
`--workspace-tools` jail over its own checkout and spawns `claude-cli` beside it, unconfined, with
every native filesystem and shell tool withdrawn, reaching the code only through
`mcp__tddy-tools__*` calls that land in the jail.

## Background

### The gap, stated as a table

On one host, the web can currently produce three of the four placements:

| Web form state | Agent | Checkout | Confines |
|---|---|---|---|
| neither toggle | host | host | nothing |
| `Sandbox` | **jail** | jail | the agent, and the code with it |
| `Sandbox` + `Managed codebase` | **jail** | host | the agent; the build runs on the host, unconfined |
| — no combination — | **host** | **jail** | the codebase and every build against it ← **the gap** |

The mode that is strictest about the agent is the least strict about the build. The thing you did
not write is the build; `cargo build` runs `build.rs`, `bun install` runs postinstall scripts.
`docs/ft/coder/sandboxed-codebase-mode.md` § Motivation makes this argument in full.

### Why the daemon refuses it today

`tddy-sandbox-app` refuses the mode client-side, before any RPC
(`packages/tddy-sandbox-app/src/codebase_mode.rs:116-120`):

> `--codebase-mode sandboxed` is supported only on macOS: it needs a `--workspace-tools` jail this
> app provisions itself, which the Linux daemon-assisted path cannot yet do

with the doc comment *"The daemon would have to provision it, and does not yet know how to."*

That was true of the `tddy-sandbox-app` shape. It is no longer true of the shape the daemon already
runs: since `workspace-tool-sandbox` (#427) and `split-sandbox-orchestration` (2026-08-31), a daemon
**does** provision a `--workspace-tools` jail over a checkout and route `ExecuteTool` into it. What
it has never done is place the *agent* for that jail on the same host.

## Proposed Changes

### What's changing

#### 1. A third codebase placement

`CodebasePlacement` (`packages/tddy-session-lifecycle/src/connection_service.rs:1002-1008`) gains a
variant:

```rust
pub enum CodebasePlacement {
    CoLocated,
    Split { codebase_instance_id: String },
    /// Agent and worktree on this daemon, and the worktree inside a `--workspace-tools` jail the
    /// agent reaches only through `mcp__tddy-tools__*`.
    SandboxedCodebase,
}
```

It is **requested explicitly**, never inferred. `classify_codebase_placement`'s existing rule —
*"An empty or self-matching id is co-located — the pre-existing behaviour, which this must never
change"* — stays exactly as written. A session that names its own host is still co-located.

#### 2. One new wire field

```proto
  // Jail this session's own checkout and run the agent beside it, unconfined, with its native
  // filesystem and shell tools withdrawn — the inverted placement of
  // docs/ft/coder/sandboxed-codebase-mode.md, served by this daemon rather than by
  // tddy-sandbox-app. Requires session_type = "claude-cli". Mutually exclusive with
  // managed_codebase (which jails the agent instead), with sandbox (same), and with
  // codebase_daemon_instance_id (that is the same inversion across two hosts, already served).
  bool sandboxed_codebase = 39;
```

— `packages/tddy-service/proto/session.proto`, field 39 (next free).

Not a `codebase_mode` string. The daemon has never had a mode concept; the wire has two booleans
that name *placements*, and a third boolean naming a third placement is the smaller, checkable
change. A string field would also let a typo become a silently-downgraded session, which is exactly
what the app's R3 refusal exists to prevent.

#### 3. The placement is a split onto this daemon

The daemon already orchestrates "agent here, jailed checkout there". The co-located form is that
orchestration with the peer hop removed:

| Step | Split (A → B), today | Sandboxed codebase (A → A), new |
|---|---|---|
| Worktree | B creates a `workspace` session, `sandbox: Some(true)` | **this daemon** creates a local `workspace` session, `sandbox: Some(true)` |
| Jail | `JailedWorkspaceSandboxProvisioner` on B | same provisioner, here |
| Agent | `claude-cli` on A, no repo, managed-codebase argv | `claude-cli` here, no repo, same argv |
| Tool route | `tddy-tools --mcp` → LiveKit → B's `ExecuteTool` | `tddy-tools --mcp` → **this daemon's own** `ExecuteTool` over HTTP |
| Jail dispatch | `exec_tool_route` on B: `workspace` + `sandbox` ⇒ `Jail` | **unchanged** — same function, same daemon |
| Pairing record | `codebase_daemon_instance_id` + `codebase_session_id` | same two fields; the instance id is this daemon's |

`exec_tool_route` (`svc_resolve_os_user.rs:218-246`) needs **no change at all**. It keys on the
session whose tools are running, and the agent's MCP addresses the *workspace* session's id
(`split_session.rs:427-429` — *"`codebase_session_id` is the workspace session on the codebase
daemon, not this session"*). Point that at a local workspace session and the existing routing
already lands in the jail.

#### 4. The one genuinely new seam: a local tool env

`split_remote_tool_env` (`split_session.rs:442-474`) deliberately leaves `daemon_url` empty:

> A split session has no HTTP route to its worktree: this daemon's own URL would answer, but from
> the wrong host's filesystem. Left empty so the LiveKit transport is the only one configured
> rather than a wrong one waiting behind it.

For a sandboxed-codebase session that reasoning inverts: this daemon's own URL is exactly the right
one. A sibling builder sets `daemon_url` to this daemon's, `session_id` to the local workspace
session's id, and **no LiveKit fields at all** — so the placement needs no common room, no peer
discovery and no join token, which the split form all require.

#### 5. Web: one checkbox

A `Sandboxed codebase` checkbox in `CreateSessionPane`'s claude-cli branch, **mutually exclusive**
with `Managed codebase` and with `Sandbox` — checking one clears the others, because all three name
a placement and a session has one. It rides through as `sandboxedCodebase` on `StartSession`.

For a host that cannot serve it, the control renders **disabled with the reason** rather than
hidden, so the capability is discoverable from the wrong host.

#### 6. Linux, with the caveat stated

The mode is served on Linux as well as macOS —
`workspace_sandbox_platform_support()` already returns `Ok(())` on both
(`workspace_tool_sandbox.rs:233-243`).

**The Linux jail confines process and network, not filesystem writes outside the checkout.** It
shares the host filesystem root; the minimal read-only root with `pivot_root` is unbuilt
(`docs/dev/todo/2026-06-28-tddy-sandbox-cgroups.md`). This is documented in the feature doc and
surfaced to the operator, not left for them to assume otherwise. It is **not** silently weaker
confinement under a name that promises more: the UI and the feature doc both say which platform
gives which guarantee.

### What's staying the same

- **Co-located `sandbox = true`** keeps today's meaning: jail the *agent* on this daemon. This
  amendment adds a placement; it changes none.
- **`classify_codebase_placement`'s self-match rule.** Naming your own host in
  `codebase_daemon_instance_id` is still `CoLocated`.
- **Split placement**, its LiveKit wiring, its withdrawals and its resume path.
- **`exec_tool_route`**, `WorkspaceSandboxRegistry`, `JailedWorkspaceSandboxProvisioner`,
  `build_workspace_tool_plan` — all consumed unchanged.
- **`tddy-sandbox-app`'s own refusals.** R3 (`managed_codebase_for_daemon_path`) still refuses
  `--codebase-mode sandboxed` on the app's Linux path. Re-pointing the CLI at this new daemon
  capability is a separate change; this one is about the web.
- **`recipe` and `--dangerously-skip-permissions`** stay withdrawn, for the same reasons they are
  withdrawn on a split: a recipe resolves `TDDY_REPO_DIR` where the agent is, and the confinement
  claim rests on a deny list whose survival under the bypass flag this repo does not pin. Both are
  withheld in the form **and** refused by the daemon — see AC11 and AC20.
- **cursor-cli** is refused, as it is for split
  (`docs/dev/todo/2026-08-13-cursor-cli-cannot-enforce-managed-codebase-mode.md`).

## Impact Analysis

### Technical Impact

| Package | Change |
|---|---|
| `tddy-service` | one proto field |
| `tddy-session-lifecycle` | `CodebasePlacement` variant, request validation, the start path, the local tool env, resume + delete pairing |
| `tddy-daemon-sandbox` | none expected — the jail is consumed as-is |
| `tddy-web` | one checkbox, the exclusivity rule, capability gating, the request field |
| `tddy-daemon` | new acceptance suites only |

**Performance.** A sandboxed-codebase session pays one extra jail per session (a
`tddy-sandbox-runner` process) and routes every tool call through the in-jail exchange, serialised
one call at a time (`InJailToolDispatcher`'s `turn` mutex, `IN_JAIL_TOOL_TIMEOUT = 600s`). It saves
the LiveKit hop the split form pays.

**Integration points.** `session_deletion.rs` already tears down a jail via `RUNNER_PID_FILE`; the
resume path already re-provisions from persisted `sandbox: Some(true)`. Both are reached through
the pairing fields this placement records, so both are expected to work unchanged — and are pinned
by tests rather than assumed.

### User Impact

- **New capability**, no breaking change. Every existing form state produces the session it
  produces today.
- **UX**: one checkbox, mutually exclusive with two existing ones. Checking it clears them — a
  visible state change, not a silent strip at submit (the rule `CreateSessionPane` already follows
  for `Managed codebase`: *"a selection the operator can no longer see is one the form must no
  longer hold"*).
- **Platform honesty**: on a Linux host the control states what the jail does and does not confine.
- **No migration.** The field defaults false.

## Implementation Plan

1. Proto field + generated TS.
2. `CodebasePlacement::SandboxedCodebase` and the request validation refusals.
3. The local workspace session + jail provisioning on the start path.
4. The local tool env, and the agent spawn with the withdrawal argv.
5. Resume and delete over the pairing.
6. Web checkbox, exclusivity, capability gating.
7. Feature-doc updates, including the Linux caveat.

## Acceptance Criteria

1. [ ] `StartSessionRequest.sandboxed_codebase = true` with `session_type = "claude-cli"` starts a
   session whose worktree is held by a **local** `workspace` session recorded
   `sandbox: Some(true)`, and whose agent half records the pairing.
2. [ ] The agent process is **not** jailed: its metadata records `sandbox: None` and no agent
   sandbox directory exists.
3. [ ] The agent's argv withdraws every native filesystem and shell tool
   (`Read`, `Write`, `Edit`, `MultiEdit`, `NotebookEdit`, `Grep`, `Glob`, `Bash`, `BashOutput`,
   `KillShell`) and allows the `mcp__tddy-tools__*` forms.
4. [ ] A tool call from that agent is executed **inside the jail** — `exec_tool_route` answers
   `Jail`, not `HostWorktree`.
5. [ ] A sandboxed-codebase session whose jail this daemon does not hold is **refused**, never
   served from the bare host worktree.
6. [ ] The agent's tool env names **this daemon over HTTP** and carries no LiveKit fields — the
   placement starts with no common room configured.
7. [ ] `sandboxed_codebase` together with `managed_codebase` is refused, naming both placements.
8. [ ] `sandboxed_codebase` together with `sandbox` is refused, naming both placements.
9. [ ] `sandboxed_codebase` together with `codebase_daemon_instance_id` is refused, naming the
   split as the cross-host form of the same inversion.
10. [ ] `sandboxed_codebase` with any `session_type` other than `claude-cli` is refused, naming
    why (no other agent's tool surface can be withdrawn).
11. [ ] `sandboxed_codebase` with a non-empty `recipe` is refused.
12. [ ] `classify_codebase_placement` still answers `CoLocated` for an empty and for a
    self-matching `codebase_daemon_instance_id` — the rule this must never change.
13. [ ] Deleting the session tears the jail down; no runner process survives.
14. [ ] Resuming re-provisions the jail from persisted metadata.
15. [ ] Web: the `Sandboxed codebase` checkbox sends `sandboxedCodebase: true` on `StartSession`.
16. [ ] Web: checking it clears `Managed codebase` and `Sandbox`, visibly, in the form.
17. [ ] Web: checking `Managed codebase` or `Sandbox` clears `Sandboxed codebase`, visibly.
18. [ ] Web: on a host that cannot serve the mode the control is **disabled and states the
    reason**, and is not silently submittable.
19. [ ] Confinement under a real jail: a `Write` lands in the checkout, a `Shell` runs with the
    checkout as cwd, and a shell reaching outside the checkout is refused **by the kernel** — the
    `Shell`, not the `Read`: the tool engine's own `contain_path` refuses an escaping `Read`
    before any syscall, so that route cannot tell a jail from no jail.
20. [ ] `sandboxed_codebase` together with `dangerously_skip_permissions` is refused, naming both.
    Web: the bypass control is withheld for this placement, and the request never carries it.

## What is deliberately not in scope

- **`pivot_root` / minimal read-only root on Linux.** The jail shares the host filesystem root, so
  Linux confinement covers process and network but not writes outside the checkout. Documented,
  surfaced, and tracked in `docs/dev/todo/2026-06-28-tddy-sandbox-cgroups.md`.
- **An egress shim in the daemon's jail.** `build_workspace_tool_plan` sets
  `loopback_allow_ports: vec![]` and passes no `--egress-shim-port`, so a jailed `cargo fetch` /
  `bun install` has no network. The app's `sandboxed` mode has one; this placement does not.
- **A per-repository build `$HOME` in the jail.** The app mounts one
  (`repo_build_home`, keyed by `derive_repo_key`); the daemon's jail mounts the worktree only.
- **Re-pointing `tddy-sandbox-app --codebase-mode sandboxed` at this daemon capability.** R3 stands.
- **cursor-cli**, for the reason split refuses it.
- **`recipe` on this placement**, for the reason split refuses it.
- **Surfacing `--codebase-home-dir` in the web.**
