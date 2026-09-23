# tddy-git architecture

## Overview

Pure `git` plumbing. Every function shells out to the `git` command line (or to `ssh` for the
remote-host variant) and returns `Result<_, String>` carrying the command's own stderr. Nothing here
knows what a session, changeset or workflow is — the session-aware layer built on top of it is
[`tddy_session_worktree::worktree`](../../tddy-session-worktree/docs/architecture.md) (also
`tddy_core::worktree`), which re-exports this crate with `pub use tddy_git::*;`.

The crate depends on no workspace crate, only `log`. All modules are private and glob re-exported
at the root, so every item is `tddy_git::<item>` regardless of which module holds it.

## Modules

### Refs (`refs.rs`) — branch and ref names

Pure string rules; no `git` probe unless stated.

- **local_branch_name_for_remote(reference, remote) -> &str**: Strips one leading `<remote>/`. The
  legacy **local_branch_name** strips `origin/`. Branch pickers deal in remote-tracking names; a
  worktree must be put on the local one.
- **validate_integration_base_ref**: Accepts any `<remote>/<single-branch-segment>` ref (the remote
  is not required to be `origin`); rejects empty, multi-segment paths, whitespace, and characters
  that could widen a `git` invocation beyond a single branch argument.
- **validate_chain_pr_integration_base_ref**: Accepts `<remote>/<path>` where `path` may contain `/`
  (multi-segment); rejects `..`, `--`, empty segments, whitespace, and shell-oriented metacharacters
  in the path. The remote is not required to be `origin`.
- **slugify_for_branch**: The branch-safe slug used to derive `feature/<slug>` from a session name.
- **first_free_suffixed_branch_name(repo_root, branch)**: The first `<branch>-<n>` (`n` from 1) no
  local branch holds — the name **create_worktree_with_retry**'s suffixing loop would land on,
  computed **without creating** a branch or worktree. Backs the `suggested_branch_name` a refused
  session creation reports (see
  [session-branch-conflict.md](../../../docs/ft/daemon/session-branch-conflict.md)); a repo whose
  refs cannot be listed reports nothing as taken and so suggests `<branch>-1`.

### Rev (`rev.rs`) — `git rev-parse` wrappers

- **checked_out_branch_name(worktree_dir) -> Option<String>**: The branch a worktree actually has
  checked out, or `None` on a detached `HEAD`. **find_existing_worktree_for_branch_ref** may answer
  with a worktree that merely *shares* a branch's tip commit, which is fine for a caller that only
  displays an indicator and wrong for one about to write — a mutation asks this first and refuses on
  a mismatch.
- `git_rev_parse`, `git_rev_parse_abbrev_ref`, `git_head_branch_name` are crate-internal, shared with
  the worktree lookups.

### Remote (`remote.rs`) — fetch, push and remote resolution

- **set_git_ssh_command**: Sets, once, the `GIT_SSH_COMMAND` applied to every `git` subprocess that
  contacts a remote. Set at daemon startup from `DaemonConfig::git.ssh_command`; `None` (the default)
  inherits the ambient environment.
- **FALLBACK_DEFAULT_INTEGRATION_BASE_REF**: `origin/master` — the last-resort default ref used when
  no remote can be detected (used by **fetch_origin_master**). Project registry rows without
  `main_branch_ref` are resolved **live** by the daemon via
  **resolve_default_integration_base_ref_with_remote**, not this constant.
- **detect_default_remote_name(repo_root) -> Option<String>**: Runs
  `git rev-parse --abbrev-ref @{upstream}` and returns the segment before the first `/` as the
  tracked remote. `None` on a detached `HEAD`, a branch with no upstream, or any `git` error — the
  probe never errors the caller.
- **resolve_default_integration_base_ref_with_remote(repo_root, preferred_remote)**: Chooses
  `preferred_remote` → **detect_default_remote_name** → `origin` (last resort), runs
  `git fetch <remote>`, then prefers `<remote>/master` if present, else `<remote>/main`, else follows
  `refs/remotes/<remote>/HEAD` when it resolves to a valid `<remote>/<branch>`. The bare
  **resolve_default_integration_base_ref** delegates with `None`.
- **fetch_integration_base**: Splits the validated ref on the first `/` into `(remote, branch)` and
  runs `git fetch <remote> <branch>` (single-segment). **fetch_origin_master** is the same with
  **FALLBACK_DEFAULT_INTEGRATION_BASE_REF**.
- **fetch_chain_pr_integration_base**: Validates with **validate_chain_pr_integration_base_ref**,
  splits on the first `/` into `(remote, path)`, then runs `git fetch <remote> <path>`.
- **push_new_branch_to_remote(worktree_dir, branch, remote)**: Runs `git push -u <remote> <branch>`.
  The legacy **push_new_branch_to_origin** wraps it with `"origin"`.
- **remote_branch_ref_sha(repo_root, branch)**: `git rev-parse --verify --quiet
  refs/remotes/<remote>/<branch>` against the detected default remote (else `origin`); `None` when
  the ref is absent or `git` fails.
- **list_recent_remote_branches / list_recent_remote_branches_skip**: Take a `remote` and list lines
  starting with `<remote>/`, skipping `<remote>/HEAD`.

### Worktree (`worktree.rs`) — linked worktrees

Worktrees live in `.worktrees/` relative to the repository root (**worktree_dir**).

- **create_worktree**: Creates a worktree with an optional `start_point` (a remote-tracking ref);
  without one it branches from `HEAD`. When the target path already exists as a linked worktree of
  this repository whose `HEAD` matches the branch, it is **reused** rather than re-added — the case of
  a changeset that lost its `worktree` field while the directory stayed registered with `git`.
- **create_worktree_with_retry**: **create_worktree**, retrying with `-1`, `-2`, … when the branch
  already exists. Returns `(worktree_path, actual_branch_name)`.
- **add_worktree_for_existing_branch**: Adds a linked worktree at `.worktrees/<name>` on an
  **existing** local branch. When `git` refuses because the branch is checked out elsewhere (the
  primary checkout on `main`, typically), it adds a detached worktree at the branch tip and then
  `git switch --ignore-other-worktrees <branch>` inside it.
- **find_existing_worktree_for_branch_ref**: Finds a worktree to reuse for a branch ref, in two
  tiers: (1) a registered worktree whose current branch equals the ref's abbreviated name; (2) else a
  worktree **under `.worktrees/`** whose `HEAD` equals the ref's resolved commit — which covers
  `origin/feature/x` against a local `feature/x`. **try_find_existing_worktree_for_branch_ref** is the
  same but returns `Ok(None)` when the ref does not resolve yet; **worktree_path_for_branch** is the
  non-erroring display wrapper, collapsing every error to `None`.
- Tier 1 spans **every** registered worktree, wherever it lives — a worktree outside `.worktrees/`
  that `git worktree list` reports is found by branch name. Tier 2, the tip-commit match, looks only
  under `.worktrees/`.
- **remove_worktree**: `git worktree remove --force`.
- **list_worktrees**: The main worktree and every linked one, as **WorktreeInfo**.

### SSH (`ssh_worktree.rs`, `ssh_exec.rs`)

- **setup_worktree_for_session_over_ssh(ssh_config_host, git_url, session_id)**: Clones (or
  initialises) the repository under the remote's default root if it is absent, then adds
  `.worktrees/<session_id>` on branch `tddy-<session_id>`, creating the branch from `HEAD` when it
  does not exist — all as one `ssh -o BatchMode=yes` batch. Idempotent: a worktree already listed at
  that path is left alone. The session id is only a string; nothing here reads a changeset.
- **ssh_exec**: `run_ssh_batch`, `shell_single_quote`, `default_remote_repo_root`,
  `contain_remote_path`. There is no fallback to the local filesystem when the remote command fails.
