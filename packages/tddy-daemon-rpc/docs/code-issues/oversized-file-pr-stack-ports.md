# oversized-file: pr_stack/ports.rs

**Location:** `packages/tddy-daemon-rpc/src/pr_stack/ports.rs`
**Category:** oversized-file
**Detected:** 2026-09-24 — `/pr-wrap` step 3.5 file-length gate on #509 (`#keyring` 2/9), production lines counted to the first `#[cfg(test)]`
**Metrics:** **775 production lines** of 775 total (no `#[cfg(test)]`; 0 test lines) · budget 500 · **1.6× over**
**Thresholds breached:** length 775 > 500
**Restructure:** not designed
**Status:** Open — regressed 2026-11-14 (818 → 868 in #886, `#orchestrators` 9/11; split deferred under the stack-branch rule — n10 also edits this file) — **unclaimed**
**Verified:** ⚠ **not hand-verified** — the line count is machine-measured; the seam table is a first reading of the item list

## Measurement history

| Run | Production lines | Note |
|---|---|---|
| 2026-09-24 | 775 | first detection. The size predates #509 (775 at merge-base `4e7157d2`). #509 changed 12 lines (6+/6−), all of them the `os_user_for_github` binding (`let os_user = self…` → `&self…`, for the account-identity return type). It did not grow the file |
| 2026-10-08 | 808 | 775 on `origin/master` → 808 after `#keyring` 9/9 (#516); split deferred with the developer's consent — see the backlog entry "`#keyring` 9/9 left files over the 500-production-line budget" (2026-10-08): +33, `repoint_planned_pr` resolving the project's account (`token_for_repoint`, `project_github_token`). File has no `#[cfg(test)]` module |
| 2026-10-24 | 819 | **Not touched** by `fix-main-agent-harness-se-e79c8de0` (`c1df8e142..4348f160a`): 819 production lines at both (counted to the first `#[cfg(test)]`; there is none) |
| 2026-11-12 | 819 → 818 | **Touched, not grown** by #879 (`#orchestrators` 2/11): four RPCs re-read `stack_plan_json` through one `stack_plan_json_of` helper, offsetting the store call-site swaps (`read_session_stack`). Not split: unchanged size, alert-only |
| 2026-11-14 | 818 → 868 | **Regressed** by #886 (`#orchestrators` 9/11): the six `session_id`-addressed handlers resolve a `StackAddress` (or forward it) beside the session path. Not split here: this file is stack-shared (n10 also edits it), so the split is deferred to the post-stack sweep per the stack-branch rule. The new RPCs themselves live in `pr_stack/project_stacks.rs` / `project_stack_queries.rs`, kept under budget |

## What would close it — candidate seams (not proven)

The file is one `impl PrStackHandler for PrStackRpcHandler` (L30–775) with eight RPC bodies. A trait impl cannot be split across files as-is. Each seam moves bodies into free functions in a sibling module and leaves delegating one-liners in the impl, the same way `branch_legs`, `guards` and `pr_status` already sit beside it.

| Seam | Items | Out |
|---|---|---|
| A | read side: `get_pr_status`, `query_branch`, `resolve_stack_base` (L95–365) | ~260 |
| B | planned-PR mutations: `add_planned_pr`, `link_stack_node`, `repoint_planned_pr`, `reorder_planned_pr` (L31–94, L366–611) | ~300 |
| C | `pull_base_into_branch` (L612–774) | ~155 |

From this first reading, Seam A plus Seam C bring the file to about 380 lines with stubs. Prove the seams with `restructure check --deep` before applying them.

**Stack constraint:** `#keyring` dependents #510–#516 are still open, so no split lands until that stack does. No dependent is known to touch this file.

## Measurement update 2026-10-15 — #622 (local gh)

**Regressed, split deferred with consent:** 808 → 819 production lines. `RepointPlannedPr` reads the sidecar/row and calls `local_gh_github_token` for a Local gh session. See [2026-10-15-local-gh-oversized-files-deferred.md](../../../../docs/dev/todo/2026-10-15-local-gh-oversized-files-deferred.md).

| 2026-11-14 | 868 → 868 | **attempted, engine-refused** in the `#orchestrators` sweep (B2). The changeset's seams (`extract_method` the four bodies into `serve_*` methods, then `move_impl_members` into `stack_reads` / `pull_base_leg`) were proven with `restructure check --deep` against the warm index: `extract_method` is refused (rust-analyzer offers no *extract into function* assist for these ranges — they hold the `forward_if_addressed … return` guard and `return Err(..)` guards), and `move_impl_members` / `move_item` in this crate fail the plan scan on the unrelated `#[path]` module (`mod project_stacks_unit_tests;`). On the rebased sweep base the file measures 876 total / 868 production. Under green-yolo consent an engine refusal is not done by hand, so B2 stops here with the record open |
