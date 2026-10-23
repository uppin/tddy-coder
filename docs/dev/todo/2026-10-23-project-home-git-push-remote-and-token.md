# 2026-10-23 — Project-home `PushSessionBranch` pushes to `origin` with the checkout's credentials

**Category:** Gap (medium)
**Source:** `#project-home` 7/7 (`feature/project-home/home-git-tools`, PR #702)

- `session_git_ops/run.rs::push` (`packages/tddy-worktree-service`) always pushes to the remote
  named `origin`. A project whose `ProjectData.remote_name` names another remote is pushed to the
  wrong one, or fails when it has no `origin`.
- The push identity (`ProjectHomePort::push_identity`, from
  `SessionAccountAccess::session_identity_in(..).git_environment`) carries only `GIT_*` pairs. A
  session's own HTTPS push gets the account's token over its toolcall socket; a daemon-run push has
  no such channel, so it uses whatever credentials the checkout has, and with
  `GIT_TERMINAL_PROMPT=0` fails rather than prompting.
- **Why kept:** both need surface outside this node — the remote name is not on node 5's
  `SessionTarget`, and a token channel for daemon-run git is the askpass/credential-helper design
  `tddy-askpass` serves for clones, not a tool-engine change.
- **What would close it:** carry the project's remote (`remote_name`, default `origin`) on the
  target or the push identity and push to it; give daemon-run pushes the account token through
  `tddy-askpass` (`GIT_ASKPASS` + the vault lookup a daemon-run clone uses). Acceptance case in
  `packages/tddy-daemon-rpc/tests/project_home_git_tools_acceptance.rs` with a project whose
  `remote_name` is `upstream` and no `origin`.
