# tddy-session-worktree architecture

## Overview

A session's git worktree: creating and reusing it, the integration base it is cut from, the chain
base a stacked child inherits from its parent, and base sync. This is the **session-aware** layer:
it reads and writes a session's `changeset.yaml`. The git plumbing underneath is
[`tddy-git`](../../tddy-git/docs/architecture.md)'s.

Workspace dependencies: `tddy-changeset`, `tddy-session-store`, `tddy-git`.

`tddy-core` re-exports this crate whole (`pub use tddy_session_worktree::*;`), so every `tddy_core::{worktree, base_sync, session_chain, git_head}::…` path consumers name resolves unchanged. New code should name `tddy_session_worktree` directly.

## Worktree (`worktree`)

The **session-aware** worktree layer: the functions that read and write a session's
`changeset.yaml` to decide which worktree a session gets. Every `git` operation they perform is
[`tddy-git`](../../tddy-git/docs/architecture.md)'s — this module re-exports that crate with
`pub use tddy_git::*;`, so `tddy_session_worktree::worktree::<any git helper>` (and `tddy_core::worktree::…`) resolves, but the helpers are
documented where they live.

- **setup_worktree_for_session_with_integration_base**: Validates and fetches the given integration
  base ref, then honours the changeset's `workflow.branch_worktree_intent` —
  `NewBranchFromBase` creates `workflow.new_branch_name` from `workflow.selected_integration_base_ref`
  (or the given ref); `WorkOnSelectedBranch` puts the worktree on the **local** form of
  `workflow.selected_branch_to_work_on`, reusing an existing worktree for that branch. With no intent
  it derives the branch from `branch_suggestion` → `branch` → `feature/<slug of name>`, reuses an
  existing worktree for it, else creates one from the ref via the retry helper. Records `worktree`,
  `branch` and `repo_path` on the changeset.
- **setup_worktree_for_session_with_optional_chain_base**: Optional chain-PR base: with `None`,
  resolves the default base, fetches, creates the worktree, sets
  **effective_worktree_integration_base_ref** on the changeset; with `Some(ref)`, validates and
  fetches the multi-segment ref, creates the worktree from that tip, and sets both
  **effective_worktree_integration_base_ref** and **worktree_integration_base_ref**.
- **resolve_persisted_worktree_integration_base_for_session**: Reads **changeset.yaml** and returns
  the persisted effective ref, else the user chain ref, else
  `tddy_git::resolve_default_integration_base_ref`.
- **setup_worktree_for_session**: Resolves the default integration base ref, then calls
  **setup_worktree_for_session_with_integration_base**. Used by TUI and daemon after plan approval
  when no explicit ref is passed at this API layer.

`tddy_core::ssh_exec` is likewise a facade over `tddy_git::ssh_exec`, and the remote-host variant
`setup_worktree_for_session_over_ssh` lives in `tddy-git`: it takes a session id as a string and
never reads a changeset.

**Repo root resolution** (where `output_dir`/`repo_path` comes from):

| Entry point | Repo root source |
|-------------|-------------------|
| TUI `run_plan_without_output_dir` | `current_dir()` when `output_dir == "."`; otherwise `output_dir` param. Stored in changeset at plan start. |
| CLI `run_plan_with_session_dir` | `current_dir()` |
| CLI `build_goal_context` (plan_dir set) | `read_changeset(plan_dir).repo_path` with fallback to `current_dir()` |

## Base sync (`base_sync`)

How a branch stands against its base — behind/ahead counts and whether taking the base would conflict — computed **without touching the repository's state**, because it runs on a status poll against worktrees that may have a child session's agent working in them.

- **branch_base_sync(repo_root, branch, base_branch) -> Result\<BranchBaseSync, String\>**, split into **resolve_base_sync_refs** (cheap: resolve both refs to commits) and **compare_base_sync_refs** (expensive: the counts and the conflict probe) so a polling caller can cache on the two SHAs.
- `git rev-list --left-right --count` for the counts; `git merge-tree --write-tree --name-only -z` for conflicts, which merges in memory and writes only the resulting tree — **no index, no working tree, no `HEAD`, no ref**. `behind_count == 0` short-circuits the probe entirely. `orchestrate_pr_stack::pr_actions::pr_resolve_conflicts_action` is **not** reusable here: it runs a real `git merge --no-commit` and would corrupt a concurrent agent's turn.
- A `<remote>/` prefix on the requested base is normalised off before probing — callers pass `ProjectEntry.main_branch_ref`, usually already `origin/master`.
- **Nothing here fetches**, so the base is read as of the last fetch. Every failure is an `Err`, never a zeroed success: a comparison that could not be made arrives byte-identical to a healthy one, so collapsing it to a default would render "could not tell" as "clean".

## Session chain (`session_chain`)

Resolves the `origin/...` chain integration base from a parent workflow session, and
`spawn_chain_child_worktree` integrates it into a child session's worktree bootstrap. See
[pr-stacking.md](../../../docs/ft/coder/pr-stacking.md).

## Worktree `HEAD` (`git_head`)

`read_head_commit` reads the worktree's `HEAD` commit **from the filesystem** rather than by
spawning git. Both sides of a session stamp records with it — the daemon for claude-cli and sandbox
sessions, the coder's presenter for tool and cursor-cli ones — so it sits in a crate both reach.
Product contract: [session-worktree-sync.md](../../../docs/ft/daemon/session-worktree-sync.md) AC1.
