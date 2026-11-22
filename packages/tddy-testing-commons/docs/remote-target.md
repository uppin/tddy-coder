# `remote_target` — the shared stand-in target kit

Test-only helpers for target-session tests (dev-dependency use only).

- `write_executable(path, body)` — writes an executable script and runs it once under the guarded
  warm-up (`TDDY_FIXTURE_WARMUP`), so a fresh script's slow first exec on macOS is paid before a
  test's timing starts.
- `StandInSshTarget` — a directory standing for a machine reached over ssh, holding the session's
  worktree, and the stand-in `ssh` that runs the remote command locally under that root.
  Its login shell answers `$SHELL -l` — a person's login shell, as a shell tab on a target runs it — by
  recording the directory it started in (`login_shell_started_in()`, written through a `.part` file and
  `mv` so a reader never sees a half-written record) and becoming an interactive `/bin/sh`.
  `behind_ssh()`'s login `PATH` is the machine's agents directory followed by this process's `PATH`;
  `behind_ssh_with_only_the_system_path()` follows it with `/usr/bin:/bin` only, so an agent not installed in
  the machine's agents directory is not found there whatever this host has installed.

Used by `tddy-agent-launch`, `tddy-session-lifecycle` and `tddy-session-driver` acceptance suites.
Copies that diverged (the stand-in coder `AWorkspace`, agent-launch's call-logging `ATarget`, the stub
agent families, `tddy-remote-shell`'s test support) are not here yet.
