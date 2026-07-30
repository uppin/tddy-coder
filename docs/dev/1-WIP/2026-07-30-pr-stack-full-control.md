# Changeset: PR-Stack full control — update, delete, search and read PRs from the orchestrator

**Date**: 2026-07-30
**Status**: 🚧 In Progress
**Type**: Feature (amendment to an existing feature)

## Affected Packages

- **tddy-workflow-recipes**: [README.md](../../packages/tddy-workflow-recipes/README.md)
  - `src/pr_stack/mod.rs` — four new stack primitives, seven new names in `PR_STACK_TOOL_NAMES`, extended `PR_STACK_ORCHESTRATE_PROMPT`
  - `src/orchestrate_pr_stack/github.rs` — new `GithubPrInsightApi` trait + `RealGithubPrApi` impl + new data types
  - `src/orchestrate_pr_stack/pr_insight.rs` — **new module**: read/search/comment shaping over the trait
  - `src/orchestrate_pr_stack/bridge.rs` — `pr_number_from_status_url` promoted to `pub`
  - `src/github_rest_common.rs` — new helper for non-`/repos/` API paths (`/search/issues`)
  - [changesets.md](../../packages/tddy-workflow-recipes/docs/changesets.md) — changeset index entry
- **tddy-tools**: [README.md](../../packages/tddy-tools/README.md)
  - `src/server.rs` — seven new `#[tool]` methods, seven new `call_tool_by_name` arms, seven new input structs
  - [changesets.md](../../packages/tddy-tools/docs/changesets.md) — changeset index entry

No `tddy-core` change: `StackNode` and `Stack` are untouched, and `changeset::update_stack_atomic` is reused as-is.
No `tddy-daemon`, `tddy-service` or `tddy-web` change: the surface is agent-side only.

## Related Feature Documentation

- [PRD: PR-Stack full control](../ft/coder/1-WIP/PRD-2026-07-30-pr-stack-full-control.md)
- [pr-stacking.md](../ft/coder/pr-stacking.md) — feature being amended (§ PR-management tools, § GitHub API surface)
- [pr-stack-live-status.md](../ft/coder/pr-stack-live-status.md) — internal-status derivation now also covers adopted nodes
- [github-pr-tools-mcp.md](../ft/coder/github-pr-tools-mcp.md) — the `tddy-tools` GitHub tool surface

## Summary

Add seven `mcp__tddy-tools__pr_*` tools to the `pr-stack` orchestrator's `orchestrate` goal: `pr_update_planned`, `pr_delete_planned`, `pr_set_parents`, `pr_read`, `pr_search`, `pr_comments`, `pr_adopt`. The stack mutations land as pure primitives in `tddy_workflow_recipes::pr_stack`; the GitHub reads land on a new `GithubPrInsightApi` trait so they are covered by hand-written fakes exactly like `GithubPrApi` is today.

## Background

The orchestrator can grow a stack and act on PR lifecycle, but cannot edit a node, delete a node, move a node in the DAG, read a PR in any depth, search the repository's PRs, or adopt an existing PR. Because whole-plan rewrite (`reseed_stack_from_plan_if_unspawned`) refuses once any node owns a `branch` or `session_id`, the plan becomes immutable the moment the stack becomes real. Full gap analysis in the PRD.

## Scope

- [ ] **Package Documentation**: `changesets.md` index entries for tddy-workflow-recipes and tddy-tools
- [x] **Implementation**: four stack primitives, one insight trait + real impl, one REST helper, seven MCP tools
- [x] **Testing**: 20 unit + 35 acceptance test instances, all passing
- [x] **Integration**: `PR_STACK_TOOL_NAMES` allowlist ↔ `#[tool]` router ↔ `call_tool_by_name` mirror ↔ orchestrate prompt all agree
- [x] **Technical Debt**: extract the shared git+GitHub realignment tail out of `repoint_planned_pr_node` instead of duplicating it
- [x] **Code Quality**: `clippy -p tddy-workflow-recipes -p tddy-tools --all-targets -D warnings` + `fmt --check` clean

## Technical Changes

### State A (Current)

**Allowlist and prompt** — `packages/tddy-workflow-recipes/src/pr_stack/mod.rs`
- `PR_STACK_TOOL_NAMES` (line 40) holds eight names; consumed once at line 145 as `GoalHints.allowed_tools` for `orchestrate`, plus `"Agent"` and permission `AcceptEdits`. Reaches the backend as `--allowedTools` (`packages/tddy-core/src/backend/claude.rs:257-262`, built at `:339`).
- `PR_STACK_ORCHESTRATE_PROMPT` (lines 53-72) documents those eight tools to the agent.

**Stack primitives** — same file
- `add_planned_pr_node` (line 397, input struct line 372): validates parents exist, assigns `next_free_node_id` (line 614, max-based), cycle-checks a candidate `Stack`, appends via `update_stack_atomic`. Appends only — never touches existing nodes. `AddPlannedPrInput.child_recipe` is accepted and discarded (`StackNode` has no field for it).
- `repoint_planned_pr_node` (line 480): rewrites `parents` by a keep/drop rule derived from `target_base_branch`, then — only if the node owns a `branch` — rebases onto the new effective base, `--force-with-lease` pushes, and `gh.patch_pr_base`. Force-push failure is `log::warn!` only.
- `reseed_stack_from_plan_if_unspawned` (line 339): whole-list replace; refuses when any node owns a `branch` or `session_id`.
- No delete. No metadata update. No arbitrary-parents rewrite.

**Data model** — `packages/tddy-core/src/changeset.rs`
- `Stack { version, nodes: Vec<StackNode> }` (line 41); `StackNode` (line 51) carries `node_id`, `title`, `description`, `branch_suggestion`, `branch`, `session_id`, `parents`, `pr_status`, `child_state`, `internal_status`. **No PR-number field.**
- `Stack::node` (line 95) is the only lookup; there is no `node_mut` — every writer open-codes `nodes.iter_mut().find(...)`.
- `Stack::topo_order` (line 101) counts in-degree only over parents that resolve to a node, so **unknown parent ids are silently ignored, not rejected**. Nothing re-validates `Changeset.stack` on read.
- `update_stack_atomic` (line 791) is read-modify-write + `write_changeset_atomic`; **no locking** — concurrent writers are last-writer-wins on the whole changeset.
- A node's PR number is recovered by string-parsing `pr_status.url`: `pr_number_from_status_url` (`orchestrate_pr_stack/bridge.rs:225`), currently **private**.

**GitHub layer** — `packages/tddy-workflow-recipes/src/orchestrate_pr_stack/github.rs`
- `GithubPrApi` trait (lines 78-110): `get_open_pr`, `get_pr_by_head`, `merge_pr`, `patch_pr_base`, `create_pr`, `disable_auto_merge`, `close_pr`. Consumers take `&dyn GithubPrApi`, and eight hand-written fakes implement it (five in `tests/`, three in-source).
- `RealGithubPrApi` (line 125): `Command::new("curl")` against a hardcoded `https://api.github.com`, token from `GITHUB_TOKEN` then `GH_TOKEN` (`github_rest_common.rs:19`). Repo slug from `TDDY_REPO_DIR` + `git remote get-url origin`.
- `github_api_url` (`github_rest_common.rs:36`) can only build `/repos/{repo}/{path}`; there is no helper for `/search/issues`. No pagination anywhere.
- **Nothing** in the repository calls `GET /pulls/{n}`, `/pulls/{n}/files`, `/pulls/{n}/reviews`, `/pulls/{n}/comments`, `/issues/{n}/comments`, `/commits/{sha}/check-runs`, or `/search/issues`.

**MCP layer** — `packages/tddy-tools/src/server.rs`
- Eight `pr_*` `#[tool]` methods (lines 619-736) in the `#[tool_router]` block (line 522), each returning a JSON `String` and flattening errors to `{"error": …}`.
- `call_tool_by_name` (lines 271-307) is a second, non-MCP dispatch mirror used by `tddy-tools call-tool` and the web Inspector; `advertised_tool_defs()` at line 1884.
- Every tool body calls `real_gh()` / `orchestrator_dir()` inline (env-driven), so the tool methods themselves have no injection seam and no unit tests. The one dispatch test is `call_tool_by_name_dispatches_mcp_tool_and_rejects_unknown` (line 1926).
- `PermissionServer::decide` (lines 422-424) auto-allows every `mcp__tddy-tools__*` call, so the allowlist governs advertisement, not permission.

### State B (Target)

**`tddy_workflow_recipes::pr_stack` gains four primitives**

```rust
pub struct UpdatePlannedPrInput {
    pub node_id: String,
    pub title: Option<String>,
    pub description: Option<String>,
    pub branch_suggestion: Option<String>,
}
pub fn update_planned_pr_node(session_dir: &Path, input: UpdatePlannedPrInput)
    -> Result<StackNode, String>;

pub struct DeletedNode {
    pub node: StackNode,
    pub reparented_children: Vec<String>,
    pub orphaned_branch: Option<String>,
    pub orphaned_session_id: Option<String>,
}
pub fn delete_planned_pr_node(session_dir: &Path, node_id: &str)
    -> Result<DeletedNode, String>;

pub fn set_stack_node_parents(
    session_dir: &Path,
    repo_root: &Path,
    node_id: &str,
    parents: &[String],
    default_branch: &str,
    gh: &dyn GithubPrApi,
) -> Result<StackNode, String>;

pub struct AdoptedPrFacts {
    pub pull_number: u64,
    pub title: String,
    pub body: String,
    pub head_branch: String,
    pub url: String,
    pub phase: String,
}
/// Pure: takes already-fetched PR facts so it is testable without GitHub.
pub fn adopt_pr_as_stack_node(session_dir: &Path, facts: AdoptedPrFacts, parents: Vec<String>)
    -> Result<StackNode, String>;
/// Composes the fetch with the append.
pub fn adopt_pr_into_stack(
    session_dir: &Path,
    pull_number: u64,
    parents: Vec<String>,
    gh: &dyn GithubPrInsightApi,
) -> Result<StackNode, String>;

/// Local-only edit + explicit GitHub push are separate functions; the tool composes them.
pub fn sync_node_to_github_pr(
    session_dir: &Path,
    node_id: &str,
    gh: &dyn GithubPrInsightApi,
) -> Result<u64, String>;
```

Rules, per the PRD:
- `update_planned_pr_node` — rejects an input naming no field; rejects a `branch_suggestion` edit once `branch.is_some()`; rejects an unknown `node_id`; never touches `parents` / `branch` / `session_id` / `pr_status` / `internal_status`.
- `delete_planned_pr_node` — rejects when `pr_status.phase == "open"`; reparents every child onto the removed node's `parents` without introducing duplicates; validates the candidate stack with `topo_order` before writing; leaves branch/worktree/session alone and reports them.
- `set_stack_node_parents` — rejects unknown parent, self-parent, duplicate entry, and cycle, writing nothing; empty list means root; runs the git+GitHub realignment only when the node owns a `branch`.
- `adopt_pr_as_stack_node` — validates parents like `add_planned_pr_node`; rejects a head branch already bound to a node; sets `branch` + `pr_status { phase, url }`, leaves `session_id` and `internal_status` `None`.

**A shared realignment helper.** The git+GitHub tail of `repoint_planned_pr_node` (effective base → rebase → force-push-with-lease → `patch_pr_base`, plus the `phase = "error"` path) is extracted into a private `realign_node_to_effective_base(session_dir, repo_root, node_id, default_branch, gh)` called by both `repoint_planned_pr_node` and `set_stack_node_parents`. `repoint_planned_pr_node`'s observable behaviour is unchanged.

**`GithubPrInsightApi` — a new sibling trait** in `orchestrate_pr_stack/github.rs`, so no existing fake changes:

```rust
pub trait GithubPrInsightApi {
    fn get_pr(&self, number: u64) -> Result<PrDetail, WorkflowError>;
    fn list_pr_files(&self, number: u64) -> Result<Vec<PrFile>, WorkflowError>;
    fn list_check_runs(&self, head_sha: &str) -> Result<Vec<CheckRun>, WorkflowError>;
    fn list_reviews(&self, number: u64) -> Result<Vec<PrReview>, WorkflowError>;
    fn list_review_comments(&self, number: u64) -> Result<Vec<PrReviewComment>, WorkflowError>;
    fn list_issue_comments(&self, number: u64) -> Result<Vec<PrIssueComment>, WorkflowError>;
    fn search_prs(&self, query: &PrSearchQuery) -> Result<Vec<PrSearchHit>, WorkflowError>;
    fn patch_pr_title_body(&self, number: u64, title: Option<&str>, body: Option<&str>)
        -> Result<(), WorkflowError>;
}
```

`RealGithubPrApi` implements it alongside `GithubPrApi`, reusing `curl_github_get_json_with_token` / `curl_github_patch_json_with_token`. `PrReviewComment` carries `id` and `in_reply_to_id` so threads can be reconstructed.

`patch_pr_title_body` refuses two payloads before spending a round trip: one naming neither field, and one naming a field whose value is blank or whitespace — GitHub answers `{"title": ""}` with a 422, and a blank body would publish an empty description over whatever the PR says today. A refusal rather than a trim-and-skip: dropping the field would apply half of a call and report the whole of it as success. `FakeInsightGithub` mirrors both refusals, so the rejection is catchable in the test suite rather than only against `api.github.com`.

**`orchestrate_pr_stack::pr_insight` — a new shaping module**, so every response shape is testable against a fake:

```rust
pub fn read_pr(gh: &dyn GithubPrInsightApi, number: u64, include_files: bool)
    -> Result<PrReadView, WorkflowError>;
pub fn read_pr_comments(gh: &dyn GithubPrInsightApi, number: u64)
    -> Result<PrCommentsView, WorkflowError>;
pub fn search_prs(gh: &dyn GithubPrInsightApi, input: PrSearchInput)
    -> Result<Vec<PrSearchHit>, WorkflowError>;
/// node_id -> pull number, via the node's recorded `pr_status.url`.
pub fn pull_number_for_node(stack: &Stack, node_id: &str) -> Result<u64, String>;
```

`PrReadView` folds `/reviews` into one latest state per reviewer and `/check-runs` into `name` + `conclusion`; `files` is `None` unless requested. `PrCommentsView` holds `reviews`, `threads` (grouped by reply chain, ordered by `created_at`), and `conversation` — and carries **no** `resolved` field, since REST does not expose it.

**`github_rest_common.rs`** gains `curl_github_get_json_absolute_path(path, query, token)` for `/search/issues`, next to the existing `/repos/{repo}/…` helpers and using the same transport.

**`bridge.rs`** — `pr_number_from_status_url` becomes `pub` and is re-exported from `orchestrate_pr_stack`.

**`pr_stack/mod.rs`** — `PR_STACK_TOOL_NAMES` becomes fifteen names; `PR_STACK_ORCHESTRATE_PROMPT` documents each new tool, including the `pr_repoint` vs `pr_set_parents` distinction and the two documented read limitations.

**`tddy-tools/src/server.rs`** — seven `#[tool]` methods with input structs `PrUpdatePlannedInput`, `PrDeletePlannedInput`, `PrSetParentsInput`, `PrReadInput`, `PrSearchInput`, `PrCommentsInput`, `PrAdoptInput`; seven matching `call_tool_by_name` arms. Each body resolves env (`orchestrator_dir`, `real_gh`, `default_branch`, repo root) and delegates — no logic in the tool bodies.

### Delta

#### tddy-workflow-recipes

- **`src/pr_stack/mod.rs`**: `+UpdatePlannedPrInput`, `+update_planned_pr_node`, `+DeletedNode`, `+delete_planned_pr_node`, `+set_stack_node_parents`, `+AdoptedPrFacts`, `+adopt_pr_as_stack_node`, `+adopt_pr_into_stack`, `+sync_node_to_github_pr`, `+realign_node_to_effective_base` (private, extracted from `repoint_planned_pr_node`). `PR_STACK_TOOL_NAMES` 8 → 15 names. `PR_STACK_ORCHESTRATE_PROMPT` extended.
- **`src/orchestrate_pr_stack/github.rs`**: `+GithubPrInsightApi` trait, `+PrDetail`, `+PrFile`, `+CheckRun`, `+PrReview`, `+PrReviewComment`, `+PrIssueComment`, `+PrSearchQuery`, `+PrSearchHit`, `+impl GithubPrInsightApi for RealGithubPrApi`. `GithubPrApi` unchanged.
- **`src/orchestrate_pr_stack/pr_insight.rs`**: new module — `read_pr`, `read_pr_comments`, `search_prs`, `pull_number_for_node`, `+PrReadView`, `+PrCommentsView`, `+PrReviewThread`, `+PrSearchInput`.
- **`src/orchestrate_pr_stack/mod.rs`**: re-export the new module and types.
- **`src/orchestrate_pr_stack/bridge.rs`**: `pr_number_from_status_url` private → `pub`.
- **`src/github_rest_common.rs`**: `+curl_github_get_json_absolute_path`.

#### tddy-tools

- **`src/server.rs`**: `+7` `#[tool]` methods and their `Parameters` input structs; `+7` arms in `call_tool_by_name` (lines 289-307). No change to `PermissionServer::decide` — `mcp__tddy-tools__*` is already auto-allowed.

## Implementation Milestones

- [x] Extract `realign_node_to_effective_base` from `repoint_planned_pr_node`; existing repoint tests still green (9 green, behaviour-neutral)
- [x] `update_planned_pr_node` + `UpdatePlannedPrInput`
- [x] `delete_planned_pr_node` + `DeletedNode` (reparenting + open-PR refusal + candidate-stack validation)
- [x] `set_stack_node_parents` (validation + realignment via the shared helper)
- [x] `pr_number_from_status_url` → `pub`; `pull_number_for_node`
- [x] `GithubPrInsightApi` trait + data types; `RealGithubPrApi` impl
- [x] `curl_github_get_json_absolute_path` for `/search/issues`
- [x] `pr_insight` module: `read_pr`, `read_pr_comments`, `search_prs` + view types
- [x] `adopt_pr_as_stack_node` + `adopt_pr_into_stack`
- [x] `sync_node_to_github_pr`
- [x] Seven `#[tool]` methods + input structs in `tddy-tools/src/server.rs`
- [x] Seven `call_tool_by_name` arms
- [x] `PR_STACK_TOOL_NAMES` + `PR_STACK_ORCHESTRATE_PROMPT` updated
- [x] `clippy -p tddy-workflow-recipes -p tddy-tools --all-targets -D warnings` + `fmt --check` clean

### Tool response shapes

The view types carry no `Serialize` derive; each tool builds its own JSON with `serde_json::json!`, the way `pr_stack_status_impl` already does — the wire shape is the tool's contract, not a Rust type's projection.

| Tool | Success shape |
|---|---|
| `pr_update_planned` | `{node_id, title, description, branch_suggestion, branch, session_id, parents}` + `pr_synced: <number>` only when a sync happened |
| `pr_delete_planned` | `{deleted, reparented_children, orphaned_branch, orphaned_session_id}` |
| `pr_set_parents` | `{node_id, parents}` |
| `pr_read` | `{number, url, title, body, state, base, head, head_sha, mergeable, mergeable_state, additions, deletions, changed_files, reviews:[{author,state}], checks:[{name,conclusion}]}`, plus `files:[{path,status}]` only when requested — the key is absent, never `null`. `state` is `open`/`merged`/`closed`/`draft` |
| `pr_search` | `{hits:[{number, title, state, draft, author, url, updated_at}]}` |
| `pr_comments` | `{reviews:[{author,state,body,submitted_at}], threads:[{path,line,diff_hunk,comments:[{author,body,created_at}]}], conversation:[{author,body,created_at}]}` — no `resolved` on a thread |
| `pr_adopt` | `{node_id, branch, parents, pr_status}` |

`pr_read` uses the wire keys `base` / `head` from the PRD rather than the view's `base_branch` / `head_branch`. `pr_read` keeps `draft` distinct as a *state*, while `pr_adopt` folds draft into the `open` *phase* — the phase vocabulary on `StackNode` is the documented `planned|open|merged|closed|error`, and the two are different things.

## Testing Plan

### Testing Strategy

**Primary level: Unit**, for every pure local stack mutation. `update_planned_pr_node`, `delete_planned_pr_node` and `adopt_pr_as_stack_node` touch only a `changeset.yaml` in a `tempfile::tempdir()`. They belong in the inline `#[cfg(test)] mod tests` of `packages/tddy-workflow-recipes/src/pr_stack/mod.rs` (lines 628-1332), reusing that module's existing `a_changeset_with_stack(nodes)` and `a_node(node_id, title, parents)` fixtures — the same home and fixtures `add_planned_pr_node`'s seven tests already use.

**Secondary level: Integration (acceptance)**, for anything crossing the GitHub boundary or the git boundary. These go in new `packages/tddy-workflow-recipes/tests/*_acceptance.rs` files following `pr_stack_repoint_dead_end_acceptance.rs` exactly: a module doc-header citing this PRD and changeset, `a_planned_node` / `an_open_node` / `a_merged_node` / `write_stack` / `parents_of` fixtures, a hand-written fake with `Mutex` call recorders, and `tempfile::tempdir()` as both `session_dir` and `repo_root` so `local_branch_exists` is false and the git rebase is deterministically skipped — the assertions are then about `Changeset.stack` and about which GitHub calls were made.

**One exception, deliberately**: `pr_stack_realign_failure_acceptance.rs` builds a *real* repository with two branches that rewrite the same line, because a `repo_root` where `local_branch_exists` is false skips the whole git half of `realign_node_to_effective_base` — including the arm that records `pr_status.phase = "error"` and refuses to re-target the PR. Claiming the extraction of that helper is behaviour-neutral while testing only its skipped half would be claiming it untested.

**Tertiary level: Integration (tool surface)**, to pin the three-way agreement that has no single owner today: allowlist ↔ MCP router ↔ `call_tool_by_name` mirror. A tool added to only two of the three is silently unreachable, which is exactly the class of bug this changeset could introduce seven times.

**Not tested, deliberately**: the `curl` bodies inside `RealGithubPrApi`. `github_api_url` hardcodes `https://api.github.com` and the transport is `Command::new("curl")`, so `wiremock` cannot intercept it — the same gap the existing seven `GithubPrApi` methods have. Everything above the trait is fake-covered. Adding a base-URL seam is scoped out (see [Decisions](#decisions-and-trade-offs)).

### Testing options analysis

**Option 1 — Unit tests against a temp `changeset.yaml` (chosen for local mutations)**
Real `read_changeset` / `update_stack_atomic` against a real file in a temp dir. Fast (no I/O beyond one small file), exercises the real serde round-trip, and asserts the on-disk result — which is what the "rejected calls write nothing" cases need. Trade-off: filesystem I/O in a unit test. Accepted, because it is one tiny file in a tempdir and it is the established pattern for all seven existing `add_planned_pr_node` tests.

**Option 2 — Acceptance tests with hand-written trait fakes (chosen for GitHub/git flows)**
A fake implementing `GithubPrInsightApi` (or `GithubPrApi`) with `Mutex<Vec<_>>` recorders, so absence of a call is assertable (`assert_eq!(gh.patched_bases(), vec![])`). Trade-off: the fake can drift from real GitHub semantics. Accepted, because it is the established pattern across five existing test files and because the alternative — a real HTTP boundary — is unreachable through `curl`.

**Option 3 — `wiremock` against a base-URL seam (rejected)**
Would give genuine coverage of request shapes and JSON parsing. Rejected for this changeset: it requires replacing the `curl` transport in `github_rest_common.rs`, which every existing GitHub call path shares, turning a feature change into a transport migration. Recorded as a future enhancement.

**Option 4 — End-to-end through a live orchestrator session (rejected as an automated gate)**
There is no fixture repository with PRs, reviews and check runs. Kept as manual verification.

### Coverage requirements

Every acceptance criterion in the PRD maps to at least one named test below. Rejection cases additionally assert the stack **on disk** is unchanged — a validation that writes a partial mutation before failing is the specific risk in a read-modify-write model with no locking.

### Acceptance tests

#### Unit — `packages/tddy-workflow-recipes/src/pr_stack/mod.rs` (inline `mod tests`, 20 instances)

Fixtures added next to the existing `a_changeset_with_stack` / `a_node`: `a_started_node(node_id, branch, phase, parents)`, `write_stack`, `stack_on_disk`, `parents_of`, `node_ids`, `an_update_of(node_id)`, and the same `assert_rejected(…).with_reason_containing(…)` assertion the acceptance files use.

`update_planned_pr_node` (5):
1. `updating_a_nodes_title_and_description_persists_both_and_leaves_every_other_field_untouched`
2. `an_update_that_names_no_field_is_rejected_and_the_stack_on_disk_is_unchanged`
3. `a_branch_suggestion_edit_is_accepted_while_the_node_is_still_planned`
4. `a_branch_suggestion_edit_is_rejected_once_the_node_owns_a_branch`
5. `updating_an_unknown_node_id_is_rejected`

`delete_planned_pr_node` (10 instances):
6. `deleting_a_middle_node_reparents_its_children_onto_that_nodes_parents`
7. `deleting_a_root_node_leaves_its_children_as_roots`
8. `a_child_that_already_lists_the_inherited_parent_does_not_gain_a_duplicate` — the diamond case
9. `no_reference_to_the_deleted_node_survives_in_any_nodes_parents` — two children, scans the whole stack
10. `deleting_a_node_whose_pr_is_open_is_rejected_and_the_stack_on_disk_is_unchanged`
11. `deleting_a_node_whose_pr_is_no_longer_open_is_allowed` — `#[rstest]` over `merged` / `closed` / `error`
12. `deleting_a_started_node_reports_its_orphaned_branch_and_session_id`
13. `deleting_an_unknown_node_id_is_rejected`

`adopt_pr_as_stack_node` (4):
14. `adopting_a_pr_creates_a_node_carrying_its_head_branch_title_body_and_url`
15. `an_adopted_node_starts_with_no_child_session_and_no_internal_status`
16. `adopting_a_pr_whose_head_branch_is_already_bound_to_a_node_is_rejected`
17. `adopting_a_pr_with_a_dangling_parent_ref_is_rejected_and_the_stack_on_disk_is_unchanged`

Tool surface (1):
18. `the_orchestrate_goal_allows_every_pr_management_tool_its_prompt_documents` — parses the `- pr_*` lines out of `PR_STACK_ORCHESTRATE_PROMPT` and compares them to `PR_STACK_TOOL_NAMES`

Shared fixtures live in `packages/tddy-workflow-recipes/tests/common/mod.rs`: the stack builders (`an_open_node` / `a_merged_node` / `a_planned_node` / `write_stack` / `parents_of` / `node_ids` / `stack_of`), the `assert_rejected(…).with_reason_containing(…)` assertion, the PR value builders, `FakeInsightGithub` — a stateful in-memory `GithubPrInsightApi` with `searched()`, `files_requested_for()` and `patched_title_bodies()` recorders — and `FakeStackGithub`, the lifecycle sibling implementing `GithubPrApi` with `looked_up()` / `patched_bases()` recorders. Several files need the same fakes seeded differently, which is why they are shared rather than hand-rolled per file.

`FakeInsightGithub`'s four PR-scoped list reads (`list_pr_files`, `list_reviews`, `list_review_comments`, `list_issue_comments`) **fail** for a pull request it was never seeded with, because the real implementation does: GitHub answers an unknown number with a 404 object, which `json_array` rejects. Serving an unknown PR an empty list would have let `read_pr_comments` report "this PR has no feedback" in the suite and error in production. `list_check_runs` keeps its empty `Ok`, being keyed by head SHA — a commit with no check runs is a real, successful, empty response.

#### Acceptance — `packages/tddy-workflow-recipes/tests/pr_stack_set_parents_acceptance.rs` (7)

Every rejection test additionally asserts `looked_up()` and `patched_bases()` are both empty (`assert_github_untouched`): a validation that rejected *after* re-targeting the PR would otherwise pass all four.

17. `setting_parents_on_a_plan_only_node_rewrites_the_dag_without_calling_github`
18. `setting_parents_on_a_branch_owning_node_patches_its_open_prs_base_to_the_new_effective_base`
19. `emptying_a_nodes_parents_makes_it_base_off_the_stack_bottom`
20. `naming_an_unknown_parent_is_rejected_and_the_dag_on_disk_is_unchanged`
21. `naming_the_node_itself_as_its_own_parent_is_rejected`
22. `a_parent_change_that_would_close_a_cycle_is_rejected_and_the_dag_on_disk_is_unchanged`
23. `a_repeated_parent_id_is_rejected`

#### Acceptance — `packages/tddy-workflow-recipes/tests/pr_stack_realign_failure_acceptance.rs` (1)

Builds a real git repository whose two branches rewrite the same line, so the rebase conflicts for the one reason git always conflicts.

24. `a_move_whose_rebase_conflicts_records_the_git_failure_on_the_node_and_leaves_its_pr_alone` — asserts the `Err` names the operation and the branch, that the node carries `phase = "error"` with git's own message, and that `patched_bases()` is empty: a failed rebase must not re-target the PR onto a base its branch does not sit on

#### Acceptance — `packages/tddy-workflow-recipes/tests/pr_stack_pr_sync_acceptance.rs` (6)

25. `syncing_a_node_patches_its_prs_title_and_body`
26. `syncing_only_a_title_patches_the_pull_request_the_node_records_and_leaves_its_body_alone` — seeds PR #1234, so a number taken from anywhere but the node's recorded url would show
27. `syncing_a_node_that_records_no_pr_is_rejected`
28. `syncing_an_unknown_node_is_rejected`
29. `syncing_a_node_without_naming_a_title_or_a_body_is_rejected`
30. `syncing_a_blank_title_is_rejected_instead_of_sent_for_github_to_refuse`

#### Acceptance — `packages/tddy-workflow-recipes/tests/pr_stack_pr_insight_acceptance.rs` (16)

31. `reading_a_pr_returns_its_body_state_base_and_head`
32. `reading_a_pr_reports_one_latest_review_state_per_reviewer` — the two reviews from one reviewer are seeded newest-first, so only a `submitted_at` comparison produces the expected rows
33. `two_reviews_submitted_at_the_same_instant_report_the_one_listed_last` — the deliberate tie-break, with nothing left to order them by
34. `reading_a_pr_summarises_each_check_run_on_its_head_commit`
35. `changed_files_are_omitted_and_never_fetched_unless_they_are_requested`
36. `changed_files_are_returned_with_a_path_and_a_status_when_requested`
37. `a_node_id_resolves_to_the_pull_number_recorded_in_its_pr_status_url`
38. `a_node_that_records_no_pr_url_cannot_be_addressed_and_says_so` — no `pr_status` at all
39. `a_node_whose_recorded_pr_status_carries_no_url_cannot_be_addressed_and_says_so` — a `pr_status` that names no pull request, a different path
40. `an_unknown_node_id_cannot_be_addressed_and_says_so`
41. `a_search_asks_github_for_the_callers_repository_and_the_agents_own_text_state_and_limit` — asserts the whole recorded `PrSearchQuery`, which also pins `limit` propagation
42. `a_search_returns_at_most_the_requested_number_of_hits`
43. `review_comments_are_grouped_into_one_thread_per_root_comment`
44. `a_threads_replies_are_ordered_by_when_they_were_written_not_by_their_id` — one reply's `created_at` disagrees with its id, so the sort is load-bearing
45. `reviews_and_conversation_comments_are_returned_as_separate_sections`
46. `reading_the_comments_of_a_pull_request_that_does_not_exist_fails_rather_than_reporting_none`

#### Acceptance — `packages/tddy-workflow-recipes/tests/pr_stack_pr_adoption_acceptance.rs` (8)

47. `adopting_a_pr_reads_its_title_body_and_head_branch_from_github`
48. `an_adopted_node_records_the_prs_live_state_as_its_phase` — `#[rstest]` over four cases: open → `open`, draft → `open`, merged → `merged`, closed → `closed`
49. `an_adopted_pr_can_be_stacked_onto_existing_nodes`
50. `adopting_a_pr_whose_head_branch_is_already_bound_to_a_node_is_rejected`
51. `adopting_a_pr_with_a_dangling_parent_ref_is_rejected_and_nothing_is_appended`

The no-session/no-internal-status invariant is *not* repeated here: `adopt_pr_into_stack` delegates it verbatim to `adopt_pr_as_stack_node`, whose unit test owns it and asserts it against the node read back from disk.

#### Unit — `packages/tddy-tools/src/server.rs` (inline `mod tests`, 8 instances)

The three helpers behind the read tools are pure and touch no environment, so the "every tool body reads `TDDY_SESSION_DIR`" limitation does not reach them:

52. `naming_neither_a_node_nor_a_pull_number_leaves_no_pull_request_to_read`
53. `naming_both_a_node_and_a_pull_number_is_rejected_rather_than_settled_by_precedence`
54. `a_pr_read_without_files_leaves_the_files_key_out_altogether` — the key is **absent**, not `null`
55. `a_pr_read_puts_the_prs_branches_on_the_wire_as_base_and_head` — the whole JSON, so the wire names stay `base` / `head`
56. `a_prs_live_state_reaches_the_agent_in_the_lowercase_phase_vocabulary` — `#[rstest]` over all four `PrState` variants

#### Acceptance — `packages/tddy-tools/tests/pr_stack_tool_dispatch_acceptance.rs` (2)

Both derive their expectations from `PR_STACK_TOOL_NAMES` rather than a literal list, so a tool added to the allowlist and left unwired fails here instead of at an operator's prompt. Both express that as "nothing is missing", which is empty by construction if the allowlist stops carrying the `mcp__tddy-tools__` prefix — so `allowlisted_tool_names` asserts it derived exactly as many names as the allowlist holds, and the dispatch test asserts it gathered one outcome per name.

57. `every_allowlisted_pr_stack_tool_is_advertised_in_the_mcp_tool_definitions`
58. `every_allowlisted_pr_stack_tool_is_dispatchable_by_name` — tells an unregistered name apart from a tool's own refusal without `TDDY_SESSION_DIR` by matching the public `server::UNKNOWN_TOOL_REJECTION` constant rather than a free-text substring, so rewording the message cannot silently turn this into a test that always passes

## Technical Debt & Production Readiness

**The `sync_pr` path of `pr_update_planned` has no automated test.** `sync_node_to_github_pr` is covered directly (6 acceptance tests) and the tool layer's *reachability* is covered (2 tests), but nothing exercises the tool's composition of local edit + GitHub push. That gap let a partial-write slip through the first implementation: the local edit was written and *then* the no-PR refusal fired, so a call the caller was told had failed had already rewritten the plan. Fixed by resolving both the PR reference (`addressed_pull_number`) and the credential (`real_gh`) before the local write, so a refused call writes nothing — but the fix is held by reasoning, not by a test. Worth a test in `tddy-tools` once that crate has an injection seam for `orchestrator_dir` / `real_gh`; today every tool *body* reads them from the environment directly, which is why no `pr_*` tool method itself has a unit test. The pure helpers those bodies delegate to — `addressed_pull_number`, `pr_read_json`, `pr_state_name` — do, since they touch no environment.

A GitHub failure *after* the preconditions hold is genuinely non-atomic and reported as such: the error carries `(the node was updated; its pull request was not)`. Making that atomic would need the primitive to stage the local write until the push succeeds — a `tddy-workflow-recipes` change, deliberately not attempted here.

**The eight new REST bodies have no automated coverage**, as planned — `github_api_url` hardcodes `https://api.github.com` and the transport is `Command::new("curl")`, so nothing can intercept them. Everything above the `GithubPrInsightApi` trait is fake-covered. Recorded under *Future Enhancements → PR-Stack full control follow-ups* in [docs/dev/TODO.md](../TODO.md).

Carried in, not introduced here — `set_stack_node_parents` reuses `repoint_planned_pr_node`'s git tail and therefore inherits its three known silent-failure paths (a failed `git rev-parse` collapsing to an empty `expected_sha` and guaranteeing a `--force-with-lease` rejection; `merge_base` failure inventing `effective_base`; a force-push failure logged as `warn!` while the call returns success and the PR is re-targeted anyway). The **conflict** arm of that tail is now covered by `pr_stack_realign_failure_acceptance.rs`; the three above remain uncovered, since each needs git itself to fail in a specific way rather than merely to conflict. Already recorded under *Future Enhancements → PR-Stack* in [docs/dev/TODO.md](../TODO.md); extraction into `realign_node_to_effective_base` makes them fixable in one place instead of one.

## Decisions & Trade-offs

**No `pull_number` field on `StackNode`.** A node's PR keeps being resolved from `pr_status.url` via `pr_number_from_status_url`. Adding a field means a second source of truth every existing writer must maintain plus a precedence rule for changesets already on disk. Cost: a node with no `pr_status.url` cannot be addressed by `node_id` in `pr_read` / `pr_comments`, and fails with an explicit message rather than guessing.

**A sibling `GithubPrInsightApi` trait instead of extending `GithubPrApi`.** Extending the existing trait would ripple stub methods into eight fakes with no interest in reads. Cost: two traits, both implemented by `RealGithubPrApi`.

**`pr_set_parents` is a new tool, not a widened `pr_repoint`.** Widening `pr_repoint` would silently change the meaning of a tool the agent already calls and put its seven existing tests at risk. Cost: two neighbouring concepts the prompt must distinguish.

**Delete reparents rather than cascading, and refuses on an open PR.** Reparenting preserves descendant work; a cascade would silently orphan branches and sessions. Closing a PR is externally visible, so the agent must call `pr_close` explicitly. Cost: a descendant may end up on a base further back than intended, and abandoning a node takes two steps.

**`sync_pr` defaults to `false` on `pr_update_planned`.** Editing a node is a plan operation; pushing to GitHub is externally visible and opted into per call. When `sync_pr` is `true` and the node has no PR the call is **rejected** rather than skipped — silently doing nothing would be a fallback.

**Shaping lives in `pr_insight`, not in the tool bodies.** The tool methods in `server.rs` read env and delegate. This is the split `add_planned_pr_node` vs `pr_add_planned` already established, and it is the only reason the response shapes are testable at all — the tool bodies have no injection seam.

**No `wiremock` seam for `RealGithubPrApi` in this changeset.** Replacing the shared `curl` transport would turn a feature change into a transport migration across every existing GitHub call path. Cost: the new REST request/response bodies ship with the same zero coverage the existing seven have.

## TODO

- [x] Create/update PRD documentation
- [x] Create changeset (this document)
- [x] Create failing acceptance tests
- [x] Run acceptance tests (verify they fail)
- [x] USER REVIEW — acceptance tests
- [x] TDD Red — write failing unit/integration tests
- [x] TDD Green — implement with quality code
- [ ] Update documentation with progress
- [ ] Repeat Red→Green→Update cycle until feature complete
- [ ] Run all tests — verify 100% pass
- [ ] Validate changes
- [ ] USER REVIEW — development complete
- [ ] Linting and type checking
- [ ] Wrap documentation
- [ ] USER REVIEW — work complete, decide next steps
