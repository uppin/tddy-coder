# 2026-11-16 — A target session's checkout lives under the target user's home and is cut like a local one

**Type:** Fix · `#remote-shell` 10/11, PR [#926](https://github.com/uppin/tddy-coder/pull/926), base `feature/remote-shell/derived-host-identity`, wave 2 of 2

A session whose agent runs on a target — an SSH host or a Coder workspace — gets its checkout under the target user's own home, `$HOME/.tddy/repos/<repository>/.worktrees/<session>` on the target, resolved through its shell; the worktree is cut with the same branch intent, base ref and new-branch name a local start honours; a clone, fetch or cut that fails refuses the start with the target's own words; deleting the session removes the worktree on the target, for a Coder target too, and unregisters it from the clone. The fixture default `/home/dev/<repo>` leaves production. A remote pack for a session whose record names no path packs locally rather than against a guessed path.

## What changed

- **tddy-git**: `remote_checkout.rs` — `REMOTE_REPOS_DIR`, `RemoteBranch::{NewFrom, Existing}`, `RemoteWorktreeRequest`, `RemoteCheckout`, `RemoteCheckoutError`, `repository_name_of`, `remote_checkout_root`, `setup_remote_worktree`, re-exported at the crate root and through `tddy_core`. Deletions: `src/ssh_worktree.rs` (replaced by `remote_checkout.rs`) and `default_remote_repo_root` in `ssh_exec.rs` (replaced by target-resolved `remote_checkout_root`).
- **tddy-agent-launch**: the remote cut in `cli_session_worktree.rs` passes the start's branch intent; `svc_signal_delete_session.rs` resolves the shell for every target kind and runs `worktree remove --force` plus `worktree prune`.
- **tddy-session-activity**: `remote_repo_path_for_session` returns the recorded path only; no fallback root.
- **tddy-session-worktree**: deletion of `tests/ssh_exec_worktree.rs` — replaced by `tddy-git/tests/remote_checkout_acceptance.rs` over `InMemoryShell` and the `tddy-session-lifecycle` target suites.
- **Docs**: `packages/tddy-git/docs/architecture.md`, `packages/tddy-git/README.md`, `packages/tddy-remote-shell/docs/architecture.md`, `packages/tddy-session-worktree/docs/architecture.md` and `docs/ft/daemon/remote-managed-worktree.md` state the target checkout root above.

## Verification

Scoped gates at wrap: `CARGO_INCREMENTAL=0 ./dev cargo test -p tddy-git -p tddy-agent-launch -p tddy-session-activity -p tddy-session-worktree` green; lifecycle suites `coder_target_session_acceptance` 5/5, `remote_target_session_acceptance` 9/9, `derived_host_start_acceptance` 1/1, `remote_hooks_session_acceptance` 2/2; clippy `-D warnings` for the touched packages and `cargo fmt --check` clean. Whole-workspace health is CI's.

`/validate-changes` (stack gate: base `feature/remote-shell/derived-host-identity`, already current and leak-free) found one gap: permanent docs still described the removed `default_remote_repo_root` and the old SSH worktree signature. All five documents were rewritten to State B and the run re-validated — the single /green re-entry of this wrap.

## File length

No touched non-test source file is at or over 500 production lines over the whole PR range (largest: `svc_signal_delete_session.rs` 414 → 446; new `remote_checkout.rs` 0 → 278). No split was required and no stack-shared file was split.

## Code issues

No code issue is claimed by this PR, none in a touched package names code this PR touches, and none carries another PR's claim on that code. Pre-existing records in the touched packages stay exactly as they stood.

## Backlog

No `docs/dev/todo/` entry is added, edited or resolved here. The changeset's scanned entry `2026-11-16-coder-hosts-never-run-against-a-real-deployment.md` stays open. This PR is not the stack's top node, so the stack's backlog delta is the top node's sweep, not this one's.
