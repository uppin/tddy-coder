# Changeset: Atomic session-file writes (swap file + rename)

**Date**: 2026-08-16
**Status**: 🚧 In Progress
**Type**: Bug Fix

## Problem

When the disk fills up, session state files become **corrupted** rather than merely unwritten.

`std::fs::write` opens the target with `O_TRUNC`. The previous contents are discarded before the
first byte of the replacement is written, so a write that fails part-way — `ENOSPC` mid-write, or
at writeback after a short `write` — leaves a **truncated or empty** file where the session's state
used to be. A 0-byte `.session.yaml` does not read as "write failed"; it reads as "not a session",
so the daemon and `tddy-web` drop a session whose agent process is still running and healthy.
Same shape for `changeset.yaml` (every later goal in the session reads it), the workflow engine's
`*.session.json` snapshot, and the daemon's `projects.yaml`.

## Change

New `tddy_core::atomic_file`:

- `write_atomic(path, contents)` — write a per-call swap file next to the target
  (`.<basename>.<pid>.<uuid>.swap`), `write_all` + `sync_all`, carry over an existing target's
  permission bits, `rename` over the target, then best-effort `fsync` of the directory. Any
  failure is cleaned up and reported with the target untouched.
- `write_atomic_labelled(path, contents)` — same, with the target path folded into the error
  string (a bare `ENOSPC` names no file).

Everything that persists session or daemon state now goes through it. Three hand-rolled
temp-then-rename implementations were folded into it as well; two of them used a **fixed** temp
name (`.changeset.yaml.tmp`, `job.json.tmp`), so two concurrent writers shared one scratch file and
either could publish the other's half-written bytes.

Deliberately **not** converted:

- `/proc`, `/sys` and cgroup control-file writes (`tddy-sandbox-cgroups`, `tddy-supervisor`) —
  these are kernel interfaces where a rename is meaningless and a plain write is the contract.
- ready markers, empty completion markers and short-lived curl body files — nothing reads them
  after a failed write.
- `tddy-build`'s action cache — the crate is deliberately standalone (`no tddy-* deps`), and it
  already writes a uniquely-named temp file, syncs it and renames.
- `tddy-tool-engine`'s file writes — those target arbitrary repository files, where replacing a
  symlink with a regular file would be a behaviour change.

Follow-ups worth doing separately: the daemon's secret stores (`github_token_store.rs`,
`vnc_vault.rs`, `screen_sharing_vault.rs`) still truncate in place. They are correct about mode
`0600` on creation, so converting them needs a mode-aware variant of `write_atomic`.

## Safety notes

- Atomic replacement changes the file's **inode**. Nothing in the repo watches session files via
  `notify`/inotify — `usage_watcher.rs` documents polling as a deliberate choice — so no reader
  depends on the inode surviving.
- Swap files are dotfiles and end in `.swap`, so directory scans that filter on `.md`
  (`inject_cross_references`) or on hidden files never pick one up.

## Affected Packages

- **tddy-core**: `atomic_file.rs` (new, with unit tests), `session_metadata.rs`, `changeset.rs`,
  `output/writer.rs`, `workflow/session.rs`, `workflow/action_cache.rs`,
  `session_action_jobs/runner.rs`, `session_actions/runtime.rs`, `backend/codex.rs`,
  `backend/cursor.rs`, `presenter/presenter_impl.rs`
- **tddy-workflow-recipes**: `writer.rs`, `pr_stack/hooks.rs`, `plan_pr_stack/hooks.rs`,
  `orchestrate_pr_stack/{hooks,transient}.rs`, `tdd/{hooks,interview}.rs`, `tdd_small/hooks.rs`,
  `bugfix/interview.rs`, `review/persist.rs`
- **tddy-daemon**: `project_storage.rs`, `worktrees.rs`, `telegram_github_link.rs`,
  `connection_service.rs`, `cursor_cli_spawn.rs`
- **tddy-sandbox-recipes**: `claude_cli.rs`, `cursor_cli.rs` (+ `tddy-core` path dependency)
- **tddy-tools**: `session_context.rs`

## Tests

- `tddy_core::atomic_file` unit tests — new file, replacement without leftover bytes, parent
  creation, **failed write leaves the previous contents intact**, concurrent writers never publish
  a mixed file, existing `0600` mode preserved.
- `session_metadata::failed_metadata_rewrite_leaves_the_session_readable` — the regression itself,
  at the level the bug was reported: after a write that cannot complete, `.session.yaml` still
  parses and still carries the previous `activity_status`.

The failure cases use a read-only directory to stand in for a full filesystem (both make the swap
file impossible to create) and no-op when running as root, where permission bits do not apply.
