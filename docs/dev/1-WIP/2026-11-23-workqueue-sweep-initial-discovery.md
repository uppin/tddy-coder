# Initial discovery — `#workqueue` sweep (n15)

**Date**: 2026-11-23
**Base**: `feature/workqueue/land-propagate` at `f53459523` (#1015, `#workqueue` 14/14), all 14 nodes green
**Measured with**: `awk '/#\[cfg\(test\)\]/{exit} {n++} END{print n+0}'` (production lines, the `/pr-wrap`
step 3.5 rule) and `wc -l` (whole file). No index daemon was started for this pass; every seam below is a
reading of the item outline, to be proven with `restructure check --deep` by `[G]`.

## 1. Files the stack left over 500 lines

Every `.rs`/`.ts`/`.tsx` source the stack created or grew, whose **whole file** is over 500 lines. Files the
stack touched only trivially (a rename pass, ±a few lines) and that were already over budget on `master`
(`tddy-coder/src/run.rs`, `tddy-worktree-service/src/worktrees.rs`, `tddy-toolcall/src/toolcall/listener.rs`,
`tddy-session-lifecycle/src/test_util.rs`, `tddy-github/src/pr_api.rs`, …) are out of scope: the stack shrank
or did not grow them, and their records already say so.

| File | Prod / total | Stack delta | Top-level shape | `#[path]` in crate |
|---|---|---|---|---|
| `tddy-daemon-rpc/src/work_queue/ports.rs` | 970 / 970 | 868 → 970 (record regressed) | **one** `#[async_trait] impl WorkQueueHandler for WorkQueueRpcHandler` (lines 39–970, 30 methods) | yes — `work_queue/project_stacks.rs:299` |
| `tddy-daemon-rpc/src/test_util.rs` | 706 / 706 | 500 → 706 | `TestDaemon` + builders (54–197), `impl ProjectService` (198–297), `impl CatalogService` (298–334), `impl ExecToolService` (335–399), **`impl WorkQueueService for TestDaemon` (400–706, one 9-line forwarder per RPC)** | yes |
| `tddy-daemon-rpc/src/exec_tool/work_queue_tools.rs` | 647 / 720 | 547 → 647 | traits + `RealWorkQueueGithubClients` (51–74), `impl ExecToolRpcHandler` (86–210), `struct DaemonStackTools` (222), **`impl DaemonStackTools` (251–588)**, `impl WorkQueueToolPort` (589–647), `mod tests` (648) | yes |
| `tddy-daemon-rpc/src/exec_tool/work_queue_tools/ops.rs` | 510 / 510 | 337 → 510, **no record** | `OpEnv` (34–78), `run` (79–264), annotation/link edits (265–339), status/refresh (340–419), `update_planned` / `set_status` (420–510) | yes |
| `tddy-daemon-rpc/src/exec_tool/work_item_git_tools.rs` | 529 / 529 | 499 → 529 | `impl ExecToolRpcHandler` (56–156), `response_of`, **`impl DaemonStackGitTools` (167–403)**, struct (404), `impl WorkItemGitToolPort` (420–528), `mod git_work` | yes |
| `tddy-daemon-rpc/src/work_queue/land.rs` | 439 / 654 | new | production + inline `mod tests` (215 lines) | yes |
| `tddy-work-queue/src/rpc.rs` | 514 / 523 | 483 → 514 | `WorkQueueStatusStream` (34–63), `WORK_QUEUE_SERVICE`, `trait WorkQueueHandler` (68–273), **`WorkQueueServiceImpl` + `impl WorkQueueService` (274–500)**, `build_pr_stack_entry` (501), `mod tests` | no |
| `tddy-work-queue/src/git_ops.rs` | 467 / 985 | new (moved in) | production + inline `mod tests` (518 lines) | no |
| `tddy-work-queue/src/item_ops/nodes.rs` | 383 / 890 | new | production + inline `mod tests` (507 lines) | no |
| `tddy-work-queue/src/assess.rs` | 178 / 604 | moved in | production + inline `mod tests` (426 lines) | no |
| `tddy-changeset/src/changeset/work_queue.rs` | 460 / 1004 | new (renamed from `changeset/stack.rs`) | types (17–145), `impl WorkQueue` (156–334), store fns (335–460), inline `mod stack_tests` (544 lines) | yes — `tab_roster.rs:204` |
| `tddy-tool-engine/src/work_queue_tools.rs` | 531 / 990 | new (renamed) | names + gating (50–132), op types and port (133–313), **argument parsing (`StackToolArgs` … `work_queue_op_of`, 314–465)**, `run` (466–531), inline `mod tests` (459 lines) | no |
| `tddy-tool-engine/src/work_item_git_tools.rs` | 328 / 717 | new (renamed) | production + inline `mod tests` (389 lines) | no |
| `tddy-tool-engine/src/lib.rs` | 430 / 577 | +10 | `pub mod` list, `ToolOutcome`, file tools, inline `pub mod dynamic_proxy` (419–477), two inline test modules (`tests`, `unbundle_exec_tool_entry_tests`) | no |
| `tddy-workqueue-web/src/WorkQueueView.tsx` | 504 (TS: whole file) | 484 → 504 | four pure helpers (67–88), one component `WorkQueueView` (99–504) holding 14 `useState`s | — (TypeScript) |
| `tddy-worktree-service/src/service.rs` | 897 / 897 | 831 → 897 (record regressed by #1005) | `mod inspector`, `struct WorktreeServiceImpl` (70), **inherent `impl WorktreeServiceImpl` (93–314)**, `project_main_repo_on_host` (315), `impl WorktreeService` (345–897) | no |

## 2. The engine's known refusals, and what they mean for each seam

- **`#[path]` modules** (`docs/dev/todo/2026-11-21-restructure-ignores-path-attribute-modules.md`, open): the
  `move_item` / `move_impl_members` module walk refuses *any* plan in a crate that declares a module with
  `#[path]`. `tddy-daemon-rpc` and `tddy-changeset` both do. Evidence from `#orchestrators` 12/12 (B6) is that
  **`extract_module` with `to_file` still lands** in such a crate (it does not take the walk), so every seam in
  those crates below is written as `extract_module` + `to_file`, never as a move.
- **Trait-impl method bodies** (`2026-11-14-restructure-engine-no-extract-for-guarded-ranges.md`,
  `…-value-returning-arms.md`): rust-analyzer offers no *extract into function* for a range that starts after a
  `forward_if_addressed … return Ok(..)` guard, nor for one holding a value `return`. Every long method of
  `ports.rs` has exactly that shape. A trait impl also cannot be split across files. **`ports.rs` therefore has
  no engine route today** — see the changeset's Decisions.
- **TypeScript** is outside the engine's v1 scope. `WorkQueueView.tsx` is split by hand under the Cypress
  component specs; that is the normal route for a TS file, not a refusal.

## 3. `ports.rs` method lengths (fn line to the next fn)

`pull_base_into_branch` 172 · `query_branch` 157 · `repoint_work_item` 147 · `link_work_item` 91 ·
`add_work_item` 79 · `resolve_work_item_base` 79 · `get_pr_status` 34 · the other 23 methods 6–14 lines each
(already one-line delegations to `work_queue_queries.rs`, `links.rs`, `annotations.rs`, `lineage.rs`,
`waves.rs`, `commit_flow.rs`, `schedules/`, `status_stream.rs`, `land.rs`). The seven long ones are 759 lines;
the established pattern in this module (`ReorderWorkItem` → `work_queue_queries.rs`, #1007) is a body on an
inherent `impl WorkQueueRpcHandler` in a sibling module, and the trait method a one-line `.await`.
`WorkQueueRpcHandler`'s private helpers (`authenticated_user`, `forward_if_addressed`,
`session_dir_for_target`) are defined in `work_queue/mod.rs`, so a sibling module sees them without widening.

## 4. Code issues the stack added or regressed

| Record | State at the tip | Sweep action |
|---|---|---|
| `tddy-daemon-rpc/…/oversized-file-work-queue-ports.md` | regressed to 970; accepted debt 2026-11-14 ("engine refused") | close by seam P (needs consent, see changeset) |
| `tddy-daemon-rpc/…/oversized-file-test-util.md` | added, 706 | close by seam T |
| `tddy-daemon-rpc/…/oversized-file-exec-tool-work-queue-tools.md` | added, 647 | close by seam Q |
| `tddy-daemon-rpc/…/oversized-file-exec-tool-work-item-git-tools.md` | added, 529 | close by seam G |
| `tddy-work-queue/…/oversized-file-rpc.md` | added, 514 | close by seam R |
| `tddy-workqueue-web/…/oversized-file-work-queue-view.md` | added, 504 | close by seam V |
| *(none)* `exec_tool/work_queue_tools/ops.rs` | 510, unrecorded — the gate on #1015 missed it | seam O; no record to write if it lands |
| `tddy-worktree-service/…/oversized-file-service.md` | pre-existing, regressed +66 by #1005; engine-deferred 2026-11-21 | seam W narrows it (attempt) |
| `tddy-work-queue/…/complexity-pull-base-…`, `complexity-repoint-repoint-work-item.md` | moved in by #1004, body unchanged | not stack-added; out of scope |

## 5. Backlog entries the stack added (`docs/dev/todo/`)

| Entry | Closable without an operator decision? |
|---|---|
| `2026-11-23-count-only-shape-tests.md` | **yes** — the three tests it lists were already deleted in-stack (`b7966a8eb`, `rpc_handlers_shape.rs` at n11); a re-run of its audit over `packages/*/tests/*{shape,split,budget,vocabulary}*.rs` at the tip finds no remaining count-only test, only an empty `// AC11, AC12 — the size budget` banner at the end of `rpc_handlers_shape.rs` |
| `2026-11-22-stack-helpers-with-no-production-caller.md` | **yes** — `docs::{StackDocsOutput, NodeDocs, validate_stack_docs, write_stack_docs}` and the private `carries_heading` / `REQUIRED_CHANGESET_HEADINGS` have no caller outside `docs.rs`'s tests. `node_doc_paths`, `NodeDocPaths` and the four basename constants **stay**: `tddy-session-files/src/stack_doc_attachments.rs` reads them |
| `2026-11-22-github-inbox-reads-a-repository-with-one-projects-credential.md` | no — the access rule (per-credential rows vs a per-caller check) is a design call |
| `2026-11-22-target-session-inspector-has-no-sync-state-or-pr.md` | no — git on a target runs with that machine's credentials; a product call |
| `2026-11-22-work-item-git-tools-reach-only-the-queue-owners-host.md` | no — a cross-host feature; the entry itself recommends waiting |
| `2026-11-22-work-queue-status-stream-is-local-only.md` | no — two fixes offered (peer-forwarded stream vs browser dials the holder) |
| `2026-11-23-work-queue-graph-is-not-live.md` | no — two fixes offered (edges on the status stream vs re-read per snapshot); a behaviour change either way |
| narrowed pre-existing entries (polling hygiene, full-control follow-ups, `QueryBranch` legs, new-session form, node-session listing, trunk watch) | no — each remainder is a feature or a behaviour change (e.g. routing the web's `RepointWorkItem` through `tddy_work_queue::engine` changes its refusals) |

## 6. Callers of the seams (blast radius)

- Seams T, Q, G, O, P, W move impl blocks or private free functions into **child** modules of the file they
  leave; nothing outside the file names an impl, so no caller changes. Private methods the parent still
  calls are widened to `pub(super)` by the assist and reported.
- Seam R moves `WorkQueueServiceImpl` and `build_pr_stack_entry`, which
  `tddy-session-lifecycle/src/work_queue_rpc.rs:7` re-exports as `tddy_work_queue::rpc::{…}`:
  `reexport: "named"` keeps that path. `WorkQueueHandler` and `WorkQueueStatusStream` stay in `rpc.rs`
  (named by `tddy-agent-launch`, `tddy-daemon-rpc`).
- The test-module extractions change no path any caller names.
- The `docs.rs` deletion removes four public items with no caller outside the crate (grep over `packages/`).
