# A target session's checkout lives under the target user's home and is cut like a local one - PRD

**Date**: 2026-11-16
**PRD Type**: Bug Fix
**Stack**: `#remote-shell 10/11` — branch `feature/remote-shell/remote-checkout-root`, base `feature/remote-shell/derived-host-identity`

## Affected Features

- **Remote managed worktree**: [remote-managed-worktree.md](../remote-managed-worktree.md) — where a target session's checkout lives and how it is cut
- **Claude CLI session**: [claude-cli-session.md](../claude-cli-session.md) — the target-session rules name the checkout root

## Summary

A session whose agent runs on a target — an SSH host or a Coder workspace — gets its checkout under the target user's own home, `$HOME/.tddy/repos/<project>`, resolved on the target through the remote shell; the worktree `.worktrees/<session>` is cut with the same branch intent, base ref and new-branch name a local start honours; a clone, fetch or cut that fails refuses the start with the target's own words; deleting the session removes the worktree on the target, for a Coder target too, and unregisters it from the clone. The fixture default `/home/dev/<repo>` leaves production.

## Background

`tddy-git/src/ssh_exec.rs:40` `default_remote_repo_root` returns `/home/dev/<repo-name>` for every
git URL — its own doc comment calls it the default "for the fixture alias `buildbox` and example URLs".
`setup_worktree_for_session_over_ssh` clones there and cuts `.worktrees/<session>` with
`git worktree add -b tddy-<session> HEAD`, whatever the start asked for. Every SSH checkout since
2026-06 and every target session since `#remote-shell 3/11` went through it. The first start on a real
Coder workspace failed with `mkdir: cannot create directory '/home/dev': Permission denied`.

Two more gaps sit beside it: `remove_target_worktree` resolves its shell with `shell_for_target`, which
refuses a Coder target, so deleting a Coder session leaves its worktree behind; and it `rm -rf`s the
worktree without `git worktree prune`, so the clone keeps a dangling registration.
`remote_repo_path_for_session` (`tddy-session-activity`) falls back to the same fixture root when a
record has no `repo_path`.

## Proposed Changes

### What's Changing

- `tddy-git`: `remote_checkout_root(shell, project_name)` resolves `$HOME` on the target and returns `<home>/.tddy/repos/<project_name>`; `setup_worktree_for_session_over_ssh` takes a `RemoteWorktreeRequest` (git URL, project name, session id, `RemoteBranch`) and returns a `RemoteCheckout` (root, worktree, branch); its errors are typed (`RemoteCheckoutError`) and carry the target's stderr
- `default_remote_repo_root` is deleted from production; the fixtures that used it move to `tddy-git`'s tests
- `tddy-agent-launch`: the remote cut receives the branch intent, base ref and new-branch name; `remove_target_worktree` resolves the shell as the start does (`shell_for_session_target`, Coder included) and runs `git -C <root> worktree remove --force <wt>` then `worktree prune`
- `tddy-session-activity`: `remote_repo_path_for_session` uses the record's `repo_path` only; a record without one is a refused pack, not a guessed path

### What's Staying the Same

- the legacy `ssh_config_host` mode (agent local, tools remote) uses the same cut, so it gets the same root
- the session record's `repo_path` is the remote path, as before
- the launch on the target (`remote_agent_launch`), hooks, pointer and resume

## Impact Analysis

### Technical Impact

- `tddy-git` gains `remote_checkout.rs`; `ssh_worktree.rs` becomes its caller
- `cli_session_worktree.rs` passes `start.intent`, `start.new_branch_name`, `start.selected_integration_base_ref`, `start.selected_branch_to_work_on` into the remote cut
- `svc_signal_delete_session.rs` takes the credential source it needs for a Coder target's shell

### User Impact

- a session on a Coder workspace or SSH host starts on any machine, in the target user's home
- the branch the operator asked for is the branch on the target
- a failed clone names the target's error instead of `remote worktree setup failed`
- deleting the session leaves nothing on the target

## Implementation Plan

1. `tddy-git`: `remote_checkout.rs` with the plan, the root resolution and the typed errors; tests over `InMemoryShell`
2. `tddy-agent-launch`: the cut honours the intent; the delete resolves a Coder target's shell and prunes
3. `tddy-session-activity`: no fallback root
4. feature docs: remote-managed-worktree.md § Agent on the target, claude-cli-session.md

Tests are written first and run per touched package; the whole-workspace result is read from CI.

## Acceptance Criteria

- [ ] The checkout root on a target is `<$HOME on the target>/.tddy/repos/<project name>`; the worktree is `<root>/.worktrees/<session id>`
- [ ] A target whose `$HOME` cannot be read refuses the start with the target's words
- [ ] `new_branch_from_base` cuts the new branch from the requested base ref on the target; `existing_branch` checks the requested branch out; no `tddy-<session>` branch is created
- [ ] A clone or fetch that fails refuses the start naming the git URL and the target's stderr; no session is left
- [ ] Deleting a session on a Coder target removes its worktree on the workspace and prunes the clone's registration
- [ ] A remote pack for a session whose record has no `repo_path` is refused, never run against a guessed path
- [ ] No production source names `/home/dev`

## References

### Affected Features (Complete List)

- **Remote managed worktree**: [remote-managed-worktree.md](../remote-managed-worktree.md) — where a target session's checkout lives and how it is cut
- **Claude CLI session**: [claude-cli-session.md](../claude-cli-session.md) — the target-session rules name the checkout root

### Related Documentation

- Changeset: `docs/dev/1-WIP/2026-11-16-remote-shell-remote-checkout-root.md` on this branch
- Other PRs of this stack are named by branch in the changeset's `## Dependencies` and `## Successor PRs`
