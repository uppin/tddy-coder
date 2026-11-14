# Initial discovery: the `#orchestrators` sweep (node 12/12)

**Date**: 2026-11-14
**Changeset**: [2026-11-14-orchestrators-sweep.md](./2026-11-14-orchestrators-sweep.md)

All of this was gathered from git and documents. Per the developer's instruction for this node,
**nothing was built, tested or indexed**: no cargo, no `./test`, no Cypress, no `tddy-tools analyze`,
no `restructure` command that needs rust-analyzer. Every number below is a line count read from a git
object. A function-level or CRAP figure is quoted from a record, never re-derived.

## 1. The stack and its base

| | |
|---|---|
| Stack | `#orchestrators`, gh stack #889, trunk `master` |
| Nodes | #878 → #888 (n1–n11), all open, all non-draft |
| Top node | #888 `feature/orchestrators/project-settings-pane`, head `2c87c1006959e4620c429f30b1851f2267e97b31` |
| Merge-base with `origin/master` | `4cf668a4a24cd7bb85c52ec6e64389518b90c7a5` |
| This branch | `feature/orchestrators/sweep`, cut from `2c87c1006` |

## 2. The backlog delta, from git

```bash
mb=$(git merge-base origin/master origin/feature/orchestrators/project-settings-pane)   # 4cf668a4a
git diff --name-status $mb..origin/feature/orchestrators/project-settings-pane \
  -- docs/dev/todo/ packages/*/docs/code-issues/
```

The quoted pathspec `'packages/*/docs/code-issues/'` from the sweep recipe matched nothing in this
shell, so the code-issue half was read with an unquoted glob and a `grep` over the full name-status
diff instead.

### 2a. Code-issue records

| Change | Record | Stack's effect on the measurement |
|---|---|---|
| A | `packages/tddy-daemon-rpc/docs/code-issues/oversized-file-stack-git-tools.md` | new: 0 → 999 (#887) |
| A | `packages/tddy-session-web/docs/code-issues/oversized-file-sessions-drawer-screen.md` | new: 487 → 581 (#886). **Stale**: #888 took it to 625 and did not update the record |
| M | `packages/tddy-daemon-rpc/docs/code-issues/oversized-file-pr-stack-ports.md` | 819 → 868 (#879 −1, #886 +50) |
| M | `packages/tddy-tool-engine/docs/code-issues/oversized-file-lib.md` | 889 → 893 (+1 each in #881, #882, #884, #887) |
| M | `packages/tddy-tools/docs/code-issues/oversized-file-session-hook.md` | 535 → 547 (#883) |
| M | `packages/tddy-prstack-web/docs/code-issues/oversized-file-pr-stack-screen.md` | 701 → 731 (#886) |
| M | `packages/tddy-telegram-control/docs/code-issues/oversized-file-telegram-session-control-callbacks.md` | 686 → 687 (#886) |
| M | `packages/tddy-agent-launch/docs/code-issues/complexity-svc-start-session-core-start-session-core.md` | +1 line, +1 early exit in `start_session_on_request` (#880) |
| M | `packages/tddy-tools/docs/code-issues/oversized-file-server.md` | **improved**: 3,080 → 2,289 (#884 −527, #887 −264). Stack-shared, [W] deferred its split to the sweep |
| M | `packages/tddy-session-create-web/docs/code-issues/oversized-file-create-session-pane.md` | **improved**: 595 → 591 (#878, #880). Stack-shared, [W] deferred its split to the sweep |
| M | `packages/tddy-daemon/docs/code-issues/complexity-mount-host-build-session-host.md` | improved 74 → 73 (#879) |
| M | `packages/tddy-pr-stack/docs/code-issues/complexity-pull-base-pull-base-into-node-branch.md` | improved 158 → 157 (#879) |
| M | `packages/tddy-pr-stack/docs/code-issues/complexity-repoint-repoint-planned-pr-node.md` | improved 79 → 78 (#879) |
| M | `packages/tddy-workflow-recipes/docs/code-issues/complexity-bridge-execute-stack-repoint.md` | improved 121 → 120 (#879) |
| D | `packages/tddy-session-lifecycle/docs/code-issues/oversized-file-test-util.md` | closed by the stack |
| D | `packages/tddy-session-split/docs/code-issues/oversized-file-svc-spawn-split-agent.md` | closed by #878 (618 → 422) |

### 2b. Oversized files the stack touched that the record diff does not show

Every non-test `.rs` / `.ts` / `.tsx` file the stack changed, production lines at `$mb` and at the top
node's head. For Rust, production lines are the lines before the first column-0 `#[cfg(test)]`. For
TypeScript, it is the whole file. Generated `*_pb.ts` files and files named `*tests.rs` are skipped.

| File | `$mb` → top | Record |
|---|---|---|
| `packages/tddy-daemon-rpc/src/project/coordinate_handlers.rs` | 564 → **566** | `oversized-file-project-coordinate-handlers.md`, **not updated by the stack**. Two `orchestrator_defaults: None` field lines (#888) |
| `packages/tddy-daemon-livekit/src/livekit_peer_discovery.rs` | 1,402 → 1,402 | touched (+1 test line), not grown: out of scope |
| `packages/tddy-session-files/src/host_documents.rs` | 827 → 831 | all four lines are in its indented `mod tests` (fixture fields), so production did not grow. No record exists, and the file's real production count was not derived: see §5 |
| `packages/tddy-changeset/src/session_metadata.rs` | 372 → **409** | under budget. The file is 1,087 lines, but 678 of them are its test module. **Not a code issue.** It was named in the wrap reports, and no record is due |

The brief named `coordinate_handlers.rs` under `tddy-agent-launch`. It lives in
`packages/tddy-daemon-rpc/src/project/`.

### 2c. TODO records

| Change | Record | Added by |
|---|---|---|
| A | `docs/dev/todo/2026-11-13-acp-turn-input-is-not-delivered.md` | #881 (n4) |
| A | `docs/dev/todo/2026-11-13-spawner-scope-check-reads-changeset-with-ok.md` | #881 (n4) `/fix-pr` |
| A | `docs/dev/todo/2026-11-14-conversation-rotation-residual-drift.md` | #883 (n6), commit `ee38c25e5` |
| A | `docs/dev/todo/2026-11-14-jail-list-exec-tools-misses-stack-node-reads.md` | #884 (n7) |
| A | `docs/dev/todo/2026-11-14-retire-stack-child-spawn-handler.md` | #887 (n10) |
| M | `docs/dev/todo/2026-10-26-forwarded-conversation-spawner-is-refused-the-start-tools.md` | #884 (n7) added a section: a stack-node session's read tools are refused across hosts |
| M | `docs/dev/todo/2026-11-10-sweep-wrap-deferred-splits-and-extracted-helpers.md` | #878 struck the row it closed. Pre-existing, so out of scope |
| D | five 2026-10-2x project-home and drawer TODOs | closed by the stack |

### 2d. Not from the records

- **Developer decision, 2026-11-14:** remove n9's first-write-create path on the
  `session_id`-addressed PR-stack RPCs (§4).
- `.agents/skills/muse-subagent/scripts/muse-agent.sh`: the `health --watch` race fix, carried
  uncommitted in this worktree.

## 3. Seams, read from each file's item list

These come from `grep` over column-0 and member declarations. None is proven: `check --deep` is
deferred to `[G]`.

### `stack_git_tools.rs` (999)

| Range | Items |
|---|---|
| 1–427 | the module doc, imports, `impl ExecToolRpcHandler` (`run_stack_git_tool`, `serve_stack_git_tool_for_caller_held_elsewhere`, `stack_git_tool_port`), `response_of`, `impl DaemonStackGitTools` (`run_blocking`, `delegate_conflicts`, `spawn_child`), `struct DaemonStackGitTools`, `impl StackGitToolPort for DaemonStackGitTools` |
| 428–805 | `GitWork`, `GitEnv`, `git`, `project_token`, `impl GitEnv` (14 methods), `node_of`, `run_git_work`, `journal_path`, `write_journal` |
| 806–999 | `merge_node`, `close_node`, `repoint_node`, `set_parents`, `resolve_conflicts`, `merge_tree_conflicts` |

The record notes that #887's `move_item` deep check was refused on an **unrelated** `#[path]` module in
this crate (`pr_stack/project_stacks.rs`). An `extract_module` plan over 429–999 passed `check --deep`
with `pub(crate)` widenings. `[G]` should try `move_item` first and fall back to
`extract_module` + `extract_module_to_file` if the `#[path]` refusal recurs.

### `pr_stack/ports.rs` (868)

The file is one `#[async_trait] impl PrStackHandler for PrStackRpcHandler` (34–862) and
`stack_plan_json_of` (863). A trait impl cannot be split across files, so the bodies must leave as
inherent methods. Each body starts with `forward_if_addressed … return Ok(…)`, so an `extract_method`
range must start **after** that early return. Ranges:

| RPC | Lines | Destination |
|---|---|---|
| `get_pr_status` | 114–146 | `pr_stack::stack_reads` |
| `query_branch` | 147–301 | `pr_stack::stack_reads` |
| `resolve_stack_base` | 302–380 | `pr_stack::stack_reads` |
| `pull_base_into_branch` | 677–846 | `pr_stack::pull_base_leg` |

That is about 420 lines out, leaving about 450.

### `project/coordinate_handlers.rs` (566)

One inherent `impl ProjectRpcHandler`. Seam A from the record is `add_project_to_host_at_project_coordinate`
(204–324, about 120 lines), leaving about 446.

### `session_hook.rs` (547)

The transport (`REPORT_TIMEOUT`, `deliver`, `HookTransport`, `hook_transport`, `exchange`,
`call_over_socket`, `post_proto`, 342–467) is about 125 lines, leaving about 422. The file's own unit
tests name `hook_transport` through `super::*`, so the move uses `reexport: "outside"`.

### `tddy-tool-engine/src/lib.rs` (893 by the record's count; 949 to the first column-0 `#[cfg(test)]`)

| Seam | Items | Lines |
|---|---|---|
| `shell_tool` | `ShellTaskBody` and its `TaskBody` impl, `register_sync_task`, `tool_shell`, `tool_await` | 163–225, 631–784 (≈215) |
| `search_tools` | `GREP_BUDGET`, `GREP_CONTEXT_LINE_CEILING`, `tool_grep`, `fold_context_events`, `context_line`, `tool_glob` | 484–630 (≈147) |
| `lsp_tools` | `tool_lsp`, `parse_lsp_query`, `tool_read_lints`, `tool_semantic_search` | 294–345, 785–889 (≈157) |

That is about 520 lines out, leaving about 375 by the record's count. The crate root also declares 29
`mod`s, and they stay where they are.

### `tddy-tools/src/server.rs` (2,289)

| Seam | Items | Lines |
|---|---|---|
| `server::subagent_tools` | `SUBAGENT_NEW_SESSION` … `subagent_resume_tool`, then `subagent_await_tool` … `close_conversation` | 1068–1343, 1477–1608 |
| `subagent_tools::turns` | `replacement_of`, `turn_budget`, `take_a_turn` | 1344–1476 |
| `subagent_tools::status` | `subagent_list_tool` … `wait_until_readiness_is_settled` | 1609–1871 |
| `subagent_tools::schemas` | `subagent_new_session_schema` … `subagent_tool_router` | 1872–2289 |
| `server::approval` | `PermissionServer::{allowed_dirs … relay_approve}` (members of the first inherent impl) | 377–665 |
| `server::dynamic_tools` | `static_tool_names` … `dynamic_tool_router` | 902–1067 |
| `server::tool_inputs` | the `#[tool]` input structs, `SpawnConversationInput` + its `JsonSchema` impl, the two request-json helpers | 66–149 |

That takes out about 2,060 lines. What stays is `PermissionServer`, its constructor and catalogue
methods, the `#[tool_router] impl` (672–820, which a macro owns and nothing splits), `relay_to_daemon`,
`github_token_for_agent` and `impl ServerHandler`: about 480 lines. `relay_approve` is a
`#[cfg]`-paired item (601 / 657). The `#agent-tools` sweep found the engine refuses those, so `[G]`
may have to merge the pair into one item first (a hand edit, like `#agent-tools` B2).

### `telegram_session_control/callbacks.rs` (687)

| Seam | Items | Lines |
|---|---|---|
| `session_list_entries` | `resolve_child_grpc_port`, `format_session_list_entry`, `telegram_label_for_session_id`, `session_list_status_or_placeholders` | 299–363 |
| `callback_data` | `RECIPE_MORE_PAGE` … `parse_callback_payload` (16 items) | 364–584 |

That is about 285 out, leaving about 400. The siblings reach these items through `use super::*`, so the
moves take `reexport: "glob"` in `callbacks` to keep every sibling compiling unchanged. The record's
alternative was a rename to `vocabulary.rs` and a test move. It is not needed for the budget, so it is
left to the record's "What would close it" and not planned.

### `svc_start_session_core.rs` (`start_session_on_request` 83, `start_here_body` 273)

`start_here_body` (177–~450) branches as `TddyCoder` (190), the placement `match` (215), the
`pr_stack_base` check (254), the branch-conflict refusal (276), the `NoAgent` arm (293–423, about 130
lines), `AgentCli` (424) and the project fallback (432). The early-return guards are the E4 refusals the
record names. The intent is to extract each arm's **body without its trailing `return`**, so that the arm
becomes `return self.<arm>(…).await;`. Any `return` inside an arm body blocks that cut, and `[G]` has to
read the arms to find out. This is the item most likely to stop 🔴.

## 4. The developer's behaviour change: no first-write create over the RPCs

**Today.** `StackTarget::Session(id)` is served through `require_orchestrator_unless_stacked`
(`pr_stack/project_stacks.rs:115`). A session that names a `stack_id` passes. A session that names
none passes only if its recipe resolves to `pr-stack` (`guards.rs::require_pr_stack_orchestrator`),
because "its first write creates the stack". The create is `update_stack_atomic`'s first-write branch
in `tddy-changeset/src/changeset/stack.rs:338` (`docs/ft/coder/pr-stacking.md:97`).

Call sites of the recipe guard:
- `ports.rs:76` (`add_planned_pr`)
- `ports.rs:431` (`link_stack_node`, directly on `orchestrator_session_id`)
- `ports.rs:655` (`reorder_planned_pr`)
- `project_stacks.rs:232` (`session_dir_for_target`, behind the repoint, pull and read legs)

**After.** A `session_id` whose changeset names no `stack_id` is refused, whatever its recipe, with
`FailedPrecondition`. The message says the session has no stack and that one is made with
`pr_stack_create`. `require_pr_stack_orchestrator` is deleted, and `require_orchestrator_unless_stacked`
becomes "require a stack" (renamed to match).

**Tests that pin today's behaviour:**
- `packages/tddy-daemon-rpc/src/pr_stack/add_planned_pr_unit_tests.rs`: `…rejects_a_session_whose_recipe_is_not_pr_stack`, `…rejects_a_session_with_no_recipe_set`.
- `packages/tddy-daemon-rpc/tests/planned_pr_mutation_rpc_acceptance.rs`: `reordering_a_session_that_is_not_a_pr_stack_orchestrator_is_refused`, `pulling_into_a_session_that_is_not_a_pr_stack_orchestrator_is_refused`. Its fixture `record_the_plan` writes `recipe: pr-stack` with an inline plan.
- `packages/tddy-daemon-rpc/tests/link_stack_node_rpc_acceptance.rs`.

**Docs that state it:** `docs/ft/coder/pr-stacking.md:421` ("only a session naming no stack still needs
the `pr-stack` recipe, because its first write creates the stack") and
`packages/tddy-daemon-rpc/docs/architecture.md`.

**Open (§ Decisions in the changeset):**
- Does a fresh `pr-stack`-recipe session still get a stack anywhere before its first `AddPlannedPr`?
  If not, the web Add-planned-PR flow on such a session is refused.
- `update_stack_atomic`'s first-write create stays for its in-process callers in `tddy-pr-stack` and
  `tddy-workflow-recipes`.

## 5. What this discovery could not establish

- **Function-level findings.** `/analyze-code-issues` was not run, so a function the stack pushed over
  60 lines, or over a nesting or CRAP budget, without a record is invisible here. The sweep recipe's
  "diff the analyzer's findings against `$mb`" step is the `[W]` re-measure's job on this node.
- **The baseline.** CI has **not** run on #888's head `2c87c1006`: `scripts/ci-status.sh 888` reports
  no checks, and `gh run list --commit 2c87c1006…` is empty. The baseline is named, not recorded.
- **Every seam.** No `restructure anchors`, `snapshot` or `check --deep` was run.
- **`host_documents.rs`'s production count.** Its test module is indented, so the column-0 rule
  overcounts it. The stack's four lines are all test lines.
