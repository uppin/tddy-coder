# PR-Stack live status & repoint

**Product area:** coder (PR stacking) + web (PR-Stack Chat Screen)
**Related:** [PR stacking](pr-stacking.md), [Session drawer § PR-Stack Chat Screen](../web/session-drawer.md#per-workflow-session-views-the-pr-stack-chat-screen)

## Summary

Today a Planned PR in the PR-Stack Chat Screen is only loosely connected to the work it
represents: the `branch` is populated after a worktree exists, the child session link
(`session_id`) is set by the orchestrator agent, and the GitHub PR number/state is only
refreshed when the orchestrator agent runs an assess pass. Operators looking at the stack
cannot reliably tell, at a glance, *which* branch a Planned PR owns, whether a session is
already working it, what its PR number/link is, or whether it needs re-pointing after a
predecessor merged.

This feature makes the **remote branch name the durable link** between a Planned PR, its
worktree/session, and its GitHub PR, and surfaces live status directly in the web view —
independent of whether the orchestrator agent is running:

1. **Definitive branch on materialization.** A Planned PR carries a canonical `branch_suggestion`
   from creation, and records it as its `branch` the moment a child worktree actually creates that
   branch. `branch` therefore means "a branch that exists", and is the single join key used for
   every downstream lookup; the suggestion is a planned name only.
2. **Branch → session resolution (in-progress).** The PR-Stack view resolves the child session
   for a node by matching the node's branch against each session's branch, and marks the node
   *in progress* when a live session owns that branch.
3. **Branch → GitHub PR status (number, link, state).** The view queries GitHub for the PR whose
   head is the node's branch and shows the PR number as a link plus its state
   (open / merged / closed / draft). Status is polled on an interval so it updates without user
   action.
4. **Repoint / restack control.** When a node's predecessor has already merged, the row offers a
   Repoint control that drops the merged parent, rebases the node's local branch onto the new
   effective base, and re-targets the open GitHub PR's base branch.
5. **Sequence-respecting base at spawn.** When a session is started for a planned node, its
   worktree is branched off the node's parent branch (the effective base, skipping merged
   ancestors) — not off the default branch. Starting a node whose non-merged parent owns no branch
   yet is refused, enforcing bottom-up ordering. The gate is the parent's *branch*, never its child
   session: a branch can be built on whether or not a session is still attached to it.

## Current behavior being fixed (capability 5)

When a session is started for a planned node today — from either the web **Start session** button
or the orchestrator agent's `spawn-child` — the child worktree is branched off the project's
**default branch** (`origin/master`/`main`), regardless of the node's DAG parents:

- The web (`PrStackScreen.handleStartSession`) passes `stackParent = <orchestrator session>`.
- `resolve_chain_base_ref` (`connection_service.rs`) sees a pr-stack orchestrator parent —
  `parent_is_pr_stack_orchestrator` returns `true` — and short-circuits to `Ok(None)` ("an
  orchestrator has no branch of its own").
- With no chain base, the worktree is created from `project.main_branch_ref` (the default branch).

`Stack::effective_base_refs(node_id)` — which returns `origin/<nearest-non-merged-ancestor.branch>`
— already exists but is only consulted by the orchestrator's assess/repoint logic, never at spawn.
The node's `parents` are therefore ignored when creating the branch, so the stack sequence is not
respected.

## Design decisions

| # | Decision | Rationale |
|---|----------|-----------|
| D1 | `StackNode.branch` is recorded when a child worktree creates the branch; planning only sets `branch_suggestion` | The branch is the link key *and* the spawn gate — descendants base onto `origin/<branch>`. Pre-filling it from the suggestion would unblock a spawn onto a ref nothing created. The suggestion is the derivation source and the name the child is asked to create. |
| D2 | The web resolves the in-progress session by matching `node.branch` against `SessionEntry.branch`; a new `SessionEntry.branch` proto field carries it | Keeps session resolution in the frontend (no new "which session owns this branch" backend signal), reusing the sessions list the drawer already loads. |
| D3 | GitHub PR status comes from a new `GetPrStatus(branch)` RPC, polled on an interval | Live status without requiring the orchestrator agent to run; polling keeps the number/link/state fresh. |
| D4 | Repoint performs DAG-parent update **and** local-branch rebase **and** GitHub base re-target | Matches the orchestrator's existing repoint semantics (`bridge::execute_stack_repoint`) so a web-triggered repoint and an agent-triggered one converge. |
| D5 | The spawn-time base is resolved in the daemon (`resolve_chain_base_ref`), the single point both the web and agent spawn paths funnel through | One source of truth; the fix lands for both `Start session` and `spawn-child` at once. |
| D6 | Starting a node whose non-merged parent owns **no branch** is refused | Enforces bottom-up ordering: the parent's branch must exist to base onto it. Keyed on the branch, never on the parent's session — a closed or cleaned-up child session must not wedge the nodes below it. A merged parent is skipped, not required. |

## API surface

### Proto (`packages/tddy-service/proto/connection.proto`)

**`SessionEntry` — new field**

```proto
// The session's git branch (from Changeset.branch). Empty when the session has no branch yet.
// Lets the PR-Stack view resolve the in-progress child session for a planned node by branch.
string branch = 28;
```

**New shared message**

```proto
// Live GitHub PR status for one head branch. Surfaced on the PR-Stack Chat Screen.
message PrStatusView {
  // False when no PR (open, merged, or closed) exists for the queried head branch.
  bool exists = 1;
  uint64 number = 2;
  string url = 3;
  // "open" | "merged" | "closed" | "draft". Empty when exists = false.
  string state = 4;
}
```

**New RPC — `GetPrStatus`**

```proto
rpc GetPrStatus(GetPrStatusRequest) returns (GetPrStatusResponse);

message GetPrStatusRequest {
  string session_token = 1;
  // The "pr-stack" orchestrator session — resolves the repo (owner/repo) to query.
  string session_id = 2;
  // Head branch to look up (the planned PR's branch).
  string branch = 3;
}
message GetPrStatusResponse {
  PrStatusView status = 1;
}
```

**New RPC — `RepointPlannedPr`**

```proto
rpc RepointPlannedPr(RepointPlannedPrRequest) returns (RepointPlannedPrResponse);

message RepointPlannedPrRequest {
  string session_token = 1;
  // The "pr-stack" orchestrator session whose Changeset.stack holds the node.
  string session_id = 2;
  // The planned node to repoint (drop merged parents, rebase, re-target PR base).
  string node_id = 3;
}
message RepointPlannedPrResponse {
  // Updated JSON-serialized Stack, same wire shape as SessionEntry.stack_plan_json (field 23).
  string stack_plan_json = 1;
}
```

**New RPC — `QueryBranch`** *(added 2026-07-25)*

Resolves, for one head branch, the in-progress child **session**, its on-disk **worktree**, and the
live GitHub **PR status** in a single call. Added **additively** — `GetPrStatus` (and `usePrStatus` /
`resolveNodeSession`) remain in place; `QueryBranch` reuses `PrStatusView` for its `pr` field.

```proto
rpc QueryBranch(QueryBranchRequest) returns (QueryBranchResponse);

message QueryBranchRequest {
  string session_token = 1;
  // The "pr-stack" orchestrator session — resolves the repo (owner/repo + repo_path) and the
  // sessions root to scan.
  string session_id = 2;
  // Head branch to resolve.
  string branch = 3;
}
message QueryBranchResponse {
  BranchResolution resolution = 1;
}

// Everything the PR-Stack row needs about one branch, resolved server-side by branch name.
message BranchResolution {
  string branch = 1;              // echoes the request; lets a response self-identify
  BranchSession session = 2;      // the in-progress child session working the branch
  BranchWorktree worktree = 3;    // the worktree checked out for the branch on disk
  PrStatusView pr = 4;            // live GitHub PR status (reuses PrStatusView)
}
message BranchSession {
  bool exists = 1;                // false when no session owns the branch
  string session_id = 2;
  bool is_active = 3;
  string status = 4;              // e.g. "active" | "idle"
}
message BranchWorktree {
  bool exists = 1;                // false when no worktree is checked out for the branch
  string path = 2;                // absolute worktree path when exists = true
}
```

The handler reuses the `get_pr_status` prologue (auth → os_user → sessions_base →
`require_pr_stack_orchestrator`) and composes: **PR** via `RealGithubPrApi::get_pr_by_head` (token-less
/ unresolvable repo → `exists = false`, never an error), **session** by scanning sessions whose
`Changeset.branch == branch` (prefers active, ties by most-recently-updated), and **worktree** via
`tddy_core::worktree::worktree_path_for_branch`.

### Rust (`tddy-core`, `tddy-workflow-recipes`, `tddy-daemon`)

- **`StackNode.branch` on materialization** — `pr_stack::add_planned_pr_node` and
  `plan_pr_stack::planned_prs_into_stack_nodes` leave `branch = None` and record the canonical name
  in `branch_suggestion`. `ConnectionServiceImpl::link_stack_node_to_spawned_branch` writes
  `branch` (plus `session_id`, as a fallback route back to the branch) once the child worktree has
  created it; a later session claiming the same branch repoints the fallback, last writer wins.
  `changeset::resolve_stack_node_branch` reads a node's branch, falling back to the branch recorded
  by its child session's changeset for a node linked before its branch was known.
- **`GithubPrApi::get_pr_by_head`** — new trait method returning the PR (open, merged, or closed)
  whose head matches a branch, with a derived `state`:

  ```rust
  pub struct PrView { pub number: u64, pub url: String, pub state: PrState }
  pub enum PrState { Open, Merged, Closed, Draft }
  fn get_pr_by_head(&self, head_branch: &str) -> Result<Option<PrView>, WorkflowError>;
  ```
- **`pr_stack::repoint_planned_pr_node`** — repoints a single node: drops merged parents from
  `node.parents` (persisted via `update_stack_atomic`), computes the effective base via
  `Stack::effective_base_refs`, rebases the node's local branch onto it, and calls
  `patch_pr_base` on the open PR. Reuses the `git_ops` + `github` primitives behind
  `bridge::execute_stack_repoint`.
- **Daemon handlers** — `get_pr_status` (resolve `owner/repo` from the orchestrator session's
  repo remote, call `get_pr_by_head`) and `repoint_planned_pr` (call
  `repoint_planned_pr_node`, return re-serialized `stack_plan_json`).
- **Enrichment** — `session_list_enrichment` populates `SessionEntry.branch` from
  `Changeset.branch`.
- **Sequence-respecting base (capability 5)** — `resolve_chain_base_ref` (renamed/extended to
  accept the new branch name) resolves, for a pr-stack orchestrator parent, the stack node that
  owns `new_branch_name` (by `branch`, else by `branch_suggestion` for a node not yet materialized)
  and returns `Stack::effective_base_refs(node_id)`'s nearest non-merged ancestor ref (or the stack
  default when the node is a root; only branch-bearing parents contribute a ref). It first enforces
  the ordering guard: if a non-merged parent owns no `branch`, it errors (`failed_precondition`)
  with a message naming that parent. The guard never consults a parent's `session_id` — a closed or
  never-linked child session must not wedge a stack whose branch exists. Both spawn paths reach
  this via `spawn_claude_cli_session_inner`.

### Web (`tddy-web`)

- `SessionEntry.branch`, `GetPrStatus*`, `RepointPlannedPr*`, `PrStatusView` regenerated into
  `gen/connection_pb.ts`.
- `resolveNodeSession(node, sessions)` — returns the live session whose `branch === node.branch`.
- `usePrStatus(client, sessionToken, orchestratorId, branches)` — polls `GetPrStatus` per branch
  on an interval, returns a `branch → PrStatusView` map.
- `PrStackScreen` gains a `sessions` prop (all sessions) threaded from `SessionsDrawerScreen`, and
  a `repointPlannedPr` handler.
- `PlannedPrRow` renders: an **in-progress** indicator (branch resolves to a live session), the
  **PR number as a link** + **PR state**, and a **Repoint** control when the node needs repoint
  (a predecessor merged).
- **`useQueryBranch(client, sessionToken, orchestratorId, branches)`** *(added 2026-07-25)* — sibling
  of `usePrStatus`, per-branch polled, returning a `branch → BranchResolution` map. `PrStackScreen`
  threads it through `PlannedPrList` into `PlannedPrRow`, which now sources the **worktree** indicator
  (`pr-stack-worktree-<nodeId>`), **in-progress** badge (`pr-stack-session-<nodeId>`), and **PR**
  link/state from the `QueryBranch` resolution. Additive alongside the existing `usePrStatus` /
  `resolveNodeSession` surfaces.

## Behavior and semantics

- **Branch as link key.** A node's `branch` is authoritative. Session resolution and PR lookup key
  off `node.branch`; `branch_suggestion` is only a derivation input, never the join key.
- **In-progress.** A node is *in progress* when some `SessionEntry.branch === node.branch` and that
  session is active. A node with no matching session shows its "Start session" CTA as today.
- **PR link/state.** When `GetPrStatus` returns `exists = true`, the row shows `#<number>` linking to
  `url` and the `state`. When `exists = false`, no PR chip is shown.
- **Repoint availability.** The Repoint control appears only when the node has at least one parent
  whose PR is merged (`StackNode::is_skipped`) — i.e. the derived `needs-repoint` condition.
- **Repoint effect.** Repoint drops merged parents, rebases the local branch onto the effective
  base, force-pushes, and re-targets the open PR's base. A rebase conflict marks the node
  `pr_status.phase = "error"` (existing `execute_stack_repoint` behavior) and surfaces as an error.
- **Spawn base.** A node with a single non-merged parent `n1` is branched off
  `origin/<n1.branch>`. A root node (no parents, or all parents merged) is branched off the stack
  default branch. Starting a node whose non-merged parent owns no branch is refused with a message
  naming the parent and its missing branch. Whether that parent still has a child session is
  irrelevant — a branch can be built on after its session is gone.

## Edge cases and constraints

- **Branch not yet on remote.** `GetPrStatus` returns `exists = false`; the row shows no PR chip and
  no in-progress indicator until a session claims the branch. Not an error.
- **No `GITHUB_TOKEN`/`GH_TOKEN`.** `GetPrStatus` returns `exists = false` (same as the orchestrator
  path, which is token-gated). No crash, no spurious error banner.
- **Branch resolves to more than one session.** Resolution prefers the active session; ties resolve to
  the most recently updated. (Should not happen for a well-formed stack.)
- **Repoint with no local branch.** Git rebase is skipped (remote-only branch); PR base is still
  re-targeted — mirrors `execute_stack_repoint`'s existing "branch not local; skipping rebase" path.
- **Polling churn.** The poll interval is fixed and shared per screen; only the branches currently
  rendered are queried.
- **Multi-parent DAG base.** A node with more than one non-merged parent uses the nearest
  ancestor ref (`effective_base_refs`' first entry) as its single base; a true octopus/merge base
  across multiple parents is out of scope for this changeset (documented non-goal).
- **Out-of-order start.** Starting a node whose non-merged parent owns no branch yet is refused
  (D6). A node whose parents are all merged is a root for base purposes and starts off the stack
  default.
- **Parent's child session closed or cleaned up.** Not an error and not a block: the parent's
  branch is what the child worktree bases onto, and it outlives the session that created it.
- **Parent's branch recorded only by its child session.** The node's `branch` resolves through
  that session's changeset (`resolve_stack_node_branch`), so the descendant still spawns. A missing
  session directory resolves to no branch, which is a refusal, not a crash.
```