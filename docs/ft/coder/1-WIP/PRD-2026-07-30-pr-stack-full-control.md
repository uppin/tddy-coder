# PRD: PR-Stack full control — update, delete, search and read PRs from the orchestrator

**Status:** 🚧 In Progress
**Date:** 2026-07-30
**Type:** Amendment (extends an existing feature)

## Summary

The `pr-stack` orchestrator agent can currently only *grow* its stack and act on PR lifecycle. It can add a planned node, read a status roll-up, override a node's internal status, spawn a child session, and merge / close / conflict-probe / re-base a PR. It cannot **edit** a node, **delete** a node, **change a node's place in the DAG**, **read a PR in any depth**, **search** the repository's PRs, or **bring an existing PR into the stack**.

This amendment closes those gaps with **seven new `mcp__tddy-tools__pr_*` tools** exposed to the `orchestrate` goal:

| New tool | Capability |
|---|---|
| `pr_update_planned` | Edit a node's `title` / `description` / `branch_suggestion`, optionally pushing title+body to the node's open PR |
| `pr_delete_planned` | Remove a node, reparenting its children onto the removed node's parents |
| `pr_set_parents` | Rewrite a node's `parents` (true DAG reparent), realigning git + the PR base when the node owns a branch |
| `pr_read` | Full detail for one PR: title, body, state, base/head, mergeability, review states, check runs, optionally changed files |
| `pr_search` | Search the repository's PRs (query, state, author, base) — including PRs not in the stack |
| `pr_comments` | Read a PR's review summaries, review-comment threads, and conversation comments |
| `pr_adopt` | Create a stack node from an existing GitHub PR, binding its branch and PR reference |

Scope is deliberately **agent-side only**. No web UI, no new gRPC methods — see [Out of scope](#out-of-scope).

## Background

### What the orchestrator has today

`PR_STACK_TOOL_NAMES` (`packages/tddy-workflow-recipes/src/pr_stack/mod.rs:40`) is the authoritative allowlist fed to `--allowedTools` for the `orchestrate` goal. It holds exactly eight names: `pr_stack_status`, `pr_merge`, `pr_repoint`, `pr_close`, `pr_resolve_conflicts`, `pr_set_status`, `pr_add_planned`, `pr_spawn_child`. See [PR stacking § PR-management tools](../pr-stacking.md#pr-management-tools).

### The seven concrete gaps

1. **No delete.** No `pr_remove_planned`, and no equivalent anywhere else in the system (not in the gRPC surface either). A mis-planned node is permanent.
2. **No edit.** `title`, `description` and `branch_suggestion` are write-once at add time. [PR stacking § PR-management tools](../pr-stacking.md#pr-management-tools) describes `pr_add_planned` as "Add/**amend** a planned PR node" — `add_planned_pr_node` only ever appends and explicitly "never touches existing nodes". The documented amend capability does not exist.
3. **No DAG reparent.** The existing `pr_repoint` calls only `GithubPrApi::patch_pr_base` — it never touches `StackNode.parents` and never rebases. The real reparenting primitive `pr_stack::repoint_planned_pr_node` (rebase, `--force-with-lease`, PR re-target, parent keep/drop rules) is reachable **only** over the gRPC `RepointPlannedPr` used by the web. The agent cannot change a node's ancestry at all after creation, and because DAG order is *derived* from `parents`, it cannot reorder either.
4. **No PR read.** The only read is `pr_stack_status`, which is a per-node roll-up (phase + internal status) and is side-effecting: it queries GitHub for every node and persists derived statuses back to the changeset. There is no way to read one PR's body, review state, checks, or changed files.
5. **No PR search.** `GET /search/issues` is never called anywhere in the repository. The agent has no way to discover a PR it doesn't already hold a reference to.
6. **No review-feedback read.** `/pulls/{n}/comments`, `/pulls/{n}/reviews` and `/issues/{n}/comments` are never called anywhere in the repository. The agent cannot see review feedback it is asked to address.
7. **No adoption.** A PR created outside the stack (by hand, by another session, by a teammate) can never join the stack. Combined with gap 2, the only whole-plan rewrite path — re-emitting `stack-plan.yaml` through the `write-stack-plan` goal — **refuses once any node owns a `branch` or a `session_id`**. In other words: the moment the stack becomes real, the plan becomes immutable.

### Why now

The orchestrator is an interactive, operator-driven chat ([PR stacking § Loop shape](../pr-stacking.md#loop-shape-free-prompting)). An operator saying "drop n4, we folded it into n3", "retitle n2", "n5 should sit on n3 not n1", "what did the reviewer ask for on n2?", or "there's already a PR for this — track it" currently gets a refusal from the agent, not because the operation is unsafe but because no tool exists.

## Proposed Changes

### 1. `pr_update_planned` — edit node metadata

```
{ node_id, title?, description?, branch_suggestion?, sync_pr? }
```

- At least one of `title` / `description` / `branch_suggestion` must be present; an all-empty call is rejected rather than silently no-op'ing.
- `title` and `description` are editable at **any** time, including on a node that owns a branch, a session, or an open PR.
- `branch_suggestion` is editable **only while the node owns no `branch`**. Once a worktree exists the suggestion is history, and rewriting it would desynchronise it from reality — rejected with an explicit message.
- `sync_pr` (default `false`): when `true` and the node resolves to a PR, also `PATCH /pulls/{n}` with the new title and/or body. When `true` and the node has no PR, the call is **rejected** — the agent asked for something that cannot happen, and silently skipping it would be a fallback.
- Never touches `parents`, `branch`, `session_id`, `pr_status` or `internal_status`.

### 2. `pr_delete_planned` — remove a node, reparent its children

```
{ node_id }
```

- **Refuses when the node has an open PR** (`pr_status.phase == "open"`), directing the agent to `pr_merge` or `pr_close` first. A node whose PR is merged, closed, errored, or absent is deletable.
- **Children are reparented onto the removed node's parents**, so the DAG stays connected and no dangling parent reference is left behind. Children that already list one of those parents don't gain a duplicate.
- Deleting a root node makes its children roots (empty `parents`), i.e. they base off the stack bottom.
- The node's `branch`, worktree and child session are **left untouched** — deletion is a plan operation, not a destructive git/session operation. The response reports the orphaned `branch` and `session_id` so the agent can tell the operator what is now unowned.
- Validates the resulting DAG with `Stack::topo_order` before writing.

### 3. `pr_set_parents` — true DAG reparent

```
{ node_id, parents: [node_id, ...] }
```

- Rejects an unknown parent id, self-parenthood, duplicate entries, and any change that would create a cycle. Nothing is written when validation fails.
- An empty `parents` list is legal and means "make this a root", basing off the stack bottom.
- **Plan-only node** (no `branch`): rewrites `parents` and stops. No git, no GitHub.
- **Node that owns a branch**: rewrites `parents`, then realigns reality exactly as the existing repoint does — rebase the branch onto the new effective base, `--force-with-lease` push, and `PATCH` the open PR's base. A rebase conflict aborts with `pr_status.phase = "error"` carrying the message, matching current repoint behaviour.

This is deliberately a *different* tool from the existing `pr_repoint`, which stays as-is: `pr_repoint` is "the GitHub base drifted, PATCH it", `pr_set_parents` is "the plan changed, move this node".

### 4. `pr_read` — one PR in full

```
{ node_id? | pull_number?, include_files? }
```

Exactly one of `node_id` / `pull_number` is required. Returns:

- `number`, `url`, `title`, `body`, `state` (`open` / `closed` / `merged` / `draft`), `base`, `head`, `head_sha`
- `mergeable`, `mergeable_state`
- `additions`, `deletions`, `changed_files`
- `reviews`: latest state per reviewer (`APPROVED` / `CHANGES_REQUESTED` / `COMMENTED`)
- `checks`: each check run's `name` and `conclusion`, from the head SHA
- `files`: `path` + `status` per changed file, **only when `include_files` is `true`** (a large PR's file list would dominate the agent's context)

### 5. `pr_search` — discover PRs in the repository

```
{ query?, state?, author?, base?, limit? }
```

- Always scoped to the orchestrator's own repository — `repo:{owner}/{repo}` and `is:pr` are injected, not caller-supplied. Cross-repository and organisation-wide search are out of scope.
- `state`: `open` / `closed` / `merged` / `all` (default `open`).
- `limit`: default 20, hard cap 100 (one page — no pagination).
- Each hit carries `number`, `title`, `state`, `draft`, `author`, `url`, `updated_at`.
- **Known limitation, documented not hidden:** `GET /search/issues` does not return a PR's head or base branch. `base:` works as a *search qualifier*, but the branch names are not in the result; the agent follows up with `pr_read` when it needs them.

### 6. `pr_comments` — read review feedback

```
{ node_id? | pull_number? }
```

Exactly one of `node_id` / `pull_number` is required. Returns three sections:

- `reviews`: `author`, `state`, `body`, `submitted_at`
- `threads`: review-comment threads grouped by reply chain — `path`, `line`, `diff_hunk`, and the ordered `comments` (`author`, `body`, `created_at`)
- `conversation`: issue-level comments — `author`, `body`, `created_at`

**Known limitation, documented not hidden:** a thread's *resolved* state is not exposed by the REST API (it is GraphQL-only). No `resolved` field is emitted rather than guessing one. Adding it is listed as a future enhancement.

### 7. `pr_adopt` — create a node from an existing PR

```
{ pull_number, parents: [node_id, ...] }
```

- Reads the PR, then appends a node carrying `title` = PR title, `description` = PR body, `branch` = PR head branch, and `pr_status` derived from the PR's live state (with `url` set to the PR's URL, which is how a node's PR number is resolved everywhere in the system today).
- `parents` are validated exactly as `pr_add_planned` validates them (must exist, no cycle). An empty list makes it a root.
- **Refuses when the PR's head branch is already bound to a node**, so a PR can't be adopted twice.
- `session_id` stays `None` — the adopted PR has a branch and a PR but no child session in this orchestrator. `internal_status` is left for `pr_stack_status` to derive.

### 8. Enabling changes

- **Allowlist + prompt.** The seven names are added to `PR_STACK_TOOL_NAMES`, and `PR_STACK_ORCHESTRATE_PROMPT` gains a description of each so the agent knows they exist.
- **A sibling GitHub trait.** The reads and the title/body patch land on a **new** `GithubPrInsightApi` trait alongside the existing `GithubPrApi`, in the same module. Adding methods to `GithubPrApi` would force every existing hand-written fake (five test files plus three in-source ones) to grow stubs for methods it does not care about; a sibling trait leaves them untouched. `RealGithubPrApi` implements both.
- **One new REST helper.** `GET /search/issues` is not under `/repos/{owner}/{repo}/`, which is the only shape `github_rest_common.rs` can currently build. A sibling helper for absolute API paths is added next to the existing ones, using the same curl + `GITHUB_TOKEN` transport.

## Impact Analysis

### Technical

- **`StackNode` is unchanged.** A node's PR number continues to be resolved from `pr_status.url` via the existing `pr_number_from_status_url` mechanism (promoted from private to public). Adding a `pull_number` field was considered and rejected — see [Decisions](#decisions-and-trade-offs).
- **`Changeset.stack` gains its first deleting writer.** `Stack::topo_order` silently ignores parent ids that don't resolve to a node, so a careless delete would leave dangling references that no existing validation catches. Every mutating tool here validates the *candidate* stack before writing, and delete's reparenting exists precisely so no dangling reference can be produced.
- **Read-modify-write races are unchanged, not worsened.** `update_stack_atomic` is atomic per write but takes no lock, so concurrent writers are last-writer-wins on the whole changeset. The new tools follow the established shape and compute their result inside the closure where correctness depends on freshness — the same discipline `repoint_planned_pr_node` already documents.
- **New network surface has no automated coverage,** consistent with the existing state: `RealGithubPrApi` shells out to `curl` against a hardcoded `https://api.github.com`, so it cannot be intercepted by `wiremock`. Everything above the trait is covered by fakes; the curl bodies are not. Introducing a base-URL seam is a real improvement but is a separate change, listed as a future enhancement.
- **`pr_repoint` keeps its current behaviour** and its seven existing tests. `pr_set_parents` reuses the git+GitHub realignment tail of `repoint_planned_pr_node` through a shared helper rather than duplicating it.

### User-facing

- The orchestrator chat gains deterministic answers to "delete that node", "retitle it", "move it under n3", "what does the review say?", "find the PR for X", "track this existing PR".
- The web PR-Stack screen is unaffected — same data, same shape. A node edited or deleted by the agent shows up on the next `SessionEntry.stack_plan_json` refresh, because the web already re-parses the whole stack.
- No change to any existing tool's inputs or outputs, so an in-flight orchestrator session keeps working.

## Out of scope

- **Web UI.** No new row actions, forms, or `PrStackScreen` changes.
- **New gRPC methods.** The asymmetry stays: the web has `AddPlannedPr` / `RepointPlannedPr` / `GetPrStatus` / `QueryBranch`; the agent now has strictly more. Exposing update/delete/adopt over gRPC is a follow-up.
- **Deleting remote branches, worktrees, or child sessions.** `pr_delete_planned` is a plan operation only.
- **An explicit reorder operation.** Order is derived from `parents`; `pr_set_parents` is the reorder primitive.
- **Review-thread resolution state** (GraphQL-only) and **resolving/replying to** review comments — read-only here.
- **Search pagination** and **cross-repository / org-wide search.**
- **A testable base-URL seam for `RealGithubPrApi`.**
- **Reviving `AddPlannedPrInput.child_recipe`,** which is inert today (`StackNode` has no field to carry it).

## Acceptance Criteria

- [ ] `PR_STACK_TOOL_NAMES` holds the seven new names, and `PR_STACK_ORCHESTRATE_PROMPT` documents each one
- [ ] All seven tools are registered in the `tddy-tools` MCP router *and* in the `call_tool_by_name` dispatch mirror
- [ ] `pr_update_planned` edits title/description on a spawned node, rejects a `branch_suggestion` edit once a branch exists, rejects an all-empty call, and rejects `sync_pr` when the node has no PR
- [ ] `pr_update_planned` with `sync_pr` patches the PR's title and body
- [ ] `pr_delete_planned` removes a node and reparents its children onto that node's parents, leaving no dangling parent reference
- [ ] `pr_delete_planned` makes children of a deleted root into roots
- [ ] `pr_delete_planned` refuses a node with an open PR and leaves the stack on disk unchanged
- [ ] `pr_delete_planned` reports the orphaned branch and session id of a deleted spawned node
- [ ] `pr_set_parents` rewrites a plan-only node's parents without touching git or GitHub
- [ ] `pr_set_parents` on a branch-owning node rebases, force-pushes with lease, and patches the PR base
- [ ] `pr_set_parents` rejects an unknown parent, self-parenthood, and a cycle — writing nothing
- [ ] `pr_read` returns title, body, state, base/head, mergeability, per-reviewer review state and check-run conclusions; changed files appear only when `include_files` is set
- [ ] `pr_search` scopes every query to the orchestrator's own repository and caps results at the requested limit
- [ ] `pr_comments` groups review comments into reply-ordered threads and returns reviews and conversation comments separately
- [ ] `pr_adopt` creates a node bound to the PR's head branch and PR reference, with validated parents
- [ ] `pr_adopt` refuses a PR whose head branch is already bound to a node
- [ ] `cargo test --workspace` shows no new failures; `clippy --workspace --all-targets -D warnings` and `fmt --check` are clean

## Affected Features

- [pr-stacking.md](../pr-stacking.md) — § PR-management tools (table + the incorrect "Add/amend" claim), § GitHub API surface, § Stack data model
- [pr-stack-live-status.md](../pr-stack-live-status.md) — internal status derivation now also runs over adopted nodes
- [github-pr-tools-mcp.md](../github-pr-tools-mcp.md) — the `tddy-tools` GitHub tool surface grows

## Decisions and trade-offs

**No `pull_number` field on `StackNode`.** Adding one would be cleaner than parsing `pr_status.url`, but it introduces a second source of truth for "which PR is this node" that every existing writer would have to maintain, plus a precedence rule for the many changesets already on disk without it. Reusing the established URL-parsing path keeps one source of truth and requires no migration. Cost: a node whose `pr_status.url` is absent cannot be addressed by `node_id` in `pr_read` / `pr_comments`, which fail with an explicit message rather than guessing.

**A sibling `GithubPrInsightApi` trait rather than extending `GithubPrApi`.** Extending the existing trait would ripple stub methods into eight fakes that have no interest in reads. Cost: two traits to hold in mind, and `RealGithubPrApi` implements both.

**`pr_set_parents` is a new tool, not a widened `pr_repoint`.** Widening `pr_repoint` would silently change the meaning of a tool the agent already uses and put its seven existing tests at risk. Cost: two neighbouring concepts the prompt must distinguish clearly.

**Delete reparents rather than cascading.** Reparenting preserves work already done on descendants; a cascade would silently orphan branches and sessions. Cost: after deleting a middle node, a descendant may sit on a base further back than intended, so the operator may need a follow-up `pr_set_parents`.

**Delete refuses on an open PR instead of closing it.** Closing a PR is externally visible and irreversible-ish; the agent must ask for it explicitly via `pr_close`. Cost: two steps for the common "abandon this node" case.

**`sync_pr` defaults to `false`.** Editing a node's title is a plan operation; pushing it to GitHub is externally visible and should be opted into per call. Cost: the agent must remember the flag when the operator means "and update the PR".
