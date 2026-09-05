# PRD Amendment: split-sandbox-orchestration

Amends [`docs/ft/daemon/remote-managed-worktree.md`](../remote-managed-worktree.md) (State A).
Dependency: [`docs/dev/1-WIP/2026-08-30-workspace-tool-sandbox.md`](../../dev/changesets/) — workspace tool sandbox (#427, landed).

## Problem

`StartSessionRequest.sandbox = true` together with `codebase_daemon_instance_id` (a split placement) is refused with `invalid_argument`:

> sandbox sessions resolve their worktree on this daemon; it cannot be combined with codebase_daemon_instance_id

The refusal predates the workspace tool sandbox. `sandbox = true` meant one thing — jail the *agent* on the daemon running it — and that jail resolves a worktree on the same daemon, which a split session has none of. With the workspace tool sandbox landed (#427), `sandbox = true` on a `workspace` session provisions a per-session jail on the host holding the checkout and routes `ExecuteTool`/`StreamExecuteTool` (and a roster agent's own loop) through it. The thing the flag confines is no longer the thing that touches the repository only on the agent's host: on a split placement the codebase host is exactly the host that holds the checkout, so the flag has a meaning there it did not have before.

## State A (current)

§ "What a split session cannot also ask for" lists `sandbox` beside `recipe` as refused on a split placement, on the premise that a sandboxed spawn resolves its worktree on the daemon running the agent. The web withdraws the Sandbox control on the same terms (`CreateSessionPane` forces `sandbox: false` on submit when `isSplitCodebase`).

## State B (target)

### Inverted, placement-dependent semantics

On a split placement `sandbox = true` confines the **codebase half**, not the agent half. The agent runs unsandboxed on A; the codebase workspace session on B is sandboxed via the existing `workspace-tool-sandbox` path.

| Half | Sandbox | Why |
|------|---------|-----|
| Agent (daemon A) | **Unsandboxed** — `sandbox: None` metadata | The agent runs on the operator's host with managed MCP tools; jailing it there would confine nothing that touches the repository, which lives on B. Keeping the agent half unsandboxed also preserves the existing `spawn_split_agent` path and resume routing (`resume_split_wiring`; `split-sandbox-resume` owns the resume half). |
| Codebase (daemon B) | **Sandboxed** — `sandbox: Some(true)` metadata | The workspace session on B holds the checkout, so the workspace tool sandbox is the jail that confines the repository-side `Shell`/`Write` work the agent proxies to it. |

### What changes

- `start_split_claude_cli_session` removes the `req.sandbox` `invalid_argument` block. `workspace_start_request` already forwards `sandbox` via `..req.clone()`, and the codebase host's workspace start already persists `sandbox: Some(true)` and provisions the jail (#427), so no new wiring is needed on the forward path.
- The agent half metadata stays `sandbox: None` (existing `spawn_split_agent`), so resume continues to route the agent half through the unsandboxed path.
- Validation gates for split+sandbox are the same as split today: `managed_codebase`, `session_type = claude-cli`, eligible codebase daemon. `recipe` is **still refused** on a split placement (a recipe resolves `TDDY_REPO_DIR` on the agent's host, which a split session still lacks).

### What does not change

- Co-located sandbox (`codebase_daemon_instance_id` empty or self) keeps today's meaning: `sandbox = true` jails the *agent* on this daemon. The split inversion is a property of the split placement, not of the flag.
- `recipe` on a split placement stays refused.
- The web form and Cypress are a separate PR (`web-split-sandbox-toggle`): this PR stops the daemon refusing the combination; exposing the checkbox is downstream.

## Acceptance criteria

1. Cross-host acceptance (extend `remote_managed_worktree_cross_host_acceptance.rs`): split start with `sandbox = true` succeeds; agent on A has `sandbox: None` metadata and no sandbox dir; a tool on B is confined (reaches the workspace jail, not the host worktree).
2. Split+sandbox + non-empty `recipe` still refused (existing test, unchanged).
3. Agent half metadata: `sandbox: None`; workspace half metadata: `sandbox: Some(true)`.
4. Placement validation (`remote_managed_worktree_acceptance.rs`): split+sandbox is admissible and fails over the missing LiveKit room with `FailedPrecondition` (the same code split as `semantic_index`), not refused outright with `InvalidArgument`.
5. `docs/ft/daemon/remote-managed-worktree.md` updated: remove `sandbox` from "What a split session cannot also ask for"; document the placement-dependent semantics.

## What is deliberately not fixed here

- Resume/relaunch of the sandboxed codebase half after a stop or daemon restart — `split-sandbox-resume` re-provisions the jail from persisted `sandbox: Some(true)`.
- The web Sandbox toggle on a split placement — `web-split-sandbox-toggle` removes the `isSplitCodebase` guard.
- Allowing `recipe` on a split placement — out of scope; a recipe still resolves on the agent's host.
