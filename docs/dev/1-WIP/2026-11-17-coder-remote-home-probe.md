# A Coder workspace's home is read through its pty without the pty's words

**Type:** Fix
**Packages:** `tddy-remote-shell`, `tddy-git`, `tddy-agent-launch`, `tddy-session-lifecycle` (test fakes only)

## Problem

A session start whose remote-worktree target is a Coder workspace
(`mantasi/remote-dev`) always failed with
`remote worktree setup failed: coder target mantasi/remote-dev is unreachable: … Process exited with status 1`
(diagnostics `coder-remote-dev-diagnostics.md` §8, verified root cause).

`coder ssh` runs the remote command in a pty, which merges the command's stderr
into its stdout. The daemon's home probe —
`coder ssh --disable-autostart <ws> -- sh -c 'printf … marker >&2; printf "%s" "$HOME"'` —
therefore captured `tddy-coder-shell: the command is running\r\n/home/coder` as
the home. The next command, `test -d '<marker>\r\n/home/coder/.tddy/repos/qape-hq/.git'`,
exited 1, and because the start marker was looked for on stderr alone, that
exit was misreported as the workspace being unreachable.

The separate `/home/dev/qape-hq` path-guess report (§1 of the diagnostics) is
the same root: the `ssh_config_host` route resolves its checkout root through
the same `$HOME` probe in `remote_checkout_root`, and no production source
names `/home/dev` (pinned by `tddy-git/tests/no_fixture_root_in_production.rs`).
A polluted probe answer was the only way a guessed-looking path could reach
the cut, so this fix covers that route too; no separate TODO is written.

## Change

- **`CoderShell` (`tddy-remote-shell`)**: the one-shot start marker is now
  recognised on whichever stream it arrives on. Present on stdout (the
  pty-merged case, with the `\r\n` a pty writes) it is stripped from stdout and
  the command's status is its own; only a non-zero exit with the marker on
  neither stream is `Unreachable`, and its detail falls back to stdout when
  stderr is empty. The marker is kept: without it a stopped workspace and a
  command exiting 1 are indistinguishable, since `coder ssh` exits 1 for both.
- **Home probe (`tddy-git`)**: `remote_checkout_root` frames the value with
  unique sentinels (`TDDY_REMOTE_HOME_BEGIN:` / `:TDDY_REMOTE_HOME_END`) and
  parses the value out from between them, so transport noise before or after
  the value — merged marker, banner, carriage returns — cannot enter the
  path. An answer without the sentinels is `HomeUnresolved` carrying the
  target's words, never a guessed home.
- **Error taxonomy**: a remote command that runs and exits non-zero is a
  command failure naming its step (`HomeUnresolved`, `Clone`, `Fetch`,
  `Worktree` — the fetch case is pinned by a new test), carrying the target's
  stderr or output. `Unreachable` is kept for connection failures only: no
  marker, non-zero exit.
- **Test fakes**: a pty-merging stand-in `coder`
  (`a_coder_whose_workspaces_merge_stderr_into_stdout`) joins the existing
  stand-ins; the in-memory and stub home answers in `tddy-git`,
  `tddy-agent-launch` and `tddy-session-lifecycle` now answer the probe the
  way a real shell answers the new command.

## Evidence

Scoped only, per repo rules (no bare `./test`, no workspace build):

- **Red first**: the new tests failed on the old code — 3 in `coder_shell_acceptance`
  (pty-merged status, value and failure) and 6 in `remote_checkout_acceptance` (sentinels, CRLF,
  fetch-failure taxonomy).
- **`./test -p tddy-remote-shell -p tddy-git -p tddy-agent-launch`** — every target green
  (unit + acceptance + doc-tests), including `tddy-git/tests/no_fixture_root_in_production.rs`.
- **`tddy-session-lifecycle`** — 104 of its 105 test binaries run green (`--test-threads=1`, the
  ssh-exec fixture sourced, as `./test` runs them), including the touched
  `cli_spec_features_acceptance`. The exceptions are both pre-existing on this machine, each
  verified against clean HEAD with these changes stashed:
  - `derived_host_start_acceptance` hangs (its `git`/`gh` children exit un-reaped and the binary
    blocks) — identically with the changes stashed, so not caused by this fix;
  - `cursor_cli_session_acceptance`, `sandboxed_claude_cli_acceptance`,
    `sandboxed_cli_harness_acceptance` and `sandboxed_cursor_cli_acceptance` fail only on
    `sandbox unsupported on linux: cgroup v2 delegation unavailable … Permission denied` — this
    machine grants no writable cgroup v2 subtree; no sandbox code is touched here.
  (`session_sync_livekit_acceptance` failed in the sweep only because the sweep had not built
  `tddy-remote-git-repo`, which the suite asserts is in `target/debug`; after building it the
  suite is green.)
- **`./dev cargo clippy -p tddy-remote-shell -p tddy-git -p tddy-agent-launch -p
  tddy-session-lifecycle -- -D warnings`** — clean.
- **`./dev cargo fmt --check`** on the four packages — clean.

## Documentation delta

- `packages/tddy-remote-shell/docs/architecture.md`: the Coder transport's
  marker is described as recognised on either stream, because `coder ssh`
  merges stderr into stdout through its pty.
- `docs/dev/todo/2026-11-16-coder-hosts-never-run-against-a-real-deployment.md`
  stays open: the pty merge is now pinned by a stand-in and by the real
  deployment's diagnostics, but the four facts it lists still have no
  real-deployment run behind them.
