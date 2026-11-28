# 8 `tddy-agent-launch` tmux tests are red on macOS (pre-existing, not #1068)

**Found:** 2026-11-28 on PR #1068. **Proven pre-existing:** I restored the three files #1068 changes
to the merge-base with master (`8184b8124`) and ran `./test -p tddy-agent-launch -- tmux`. The same
8 tests failed and 48 tmux tests passed, exactly as on #1068's head. CI runs on Linux and is the
authority there.

## Failing

Unit tests, `src/remote_agent_launch/target_tmux_tab_tests.rs`:
- `an_agent_tab_command_attaches_when_its_session_already_lives`
- `an_agent_tab_command_enables_forwarding_then_attaches`
- `an_agent_tab_command_leaves_an_explicitly_disabled_server_clipboard_off`

Acceptance tests, `tests/remote_agent_launch_acceptance.rs` (each `AgentNeverStarted { after: 60s }`):
- `a_second_launch_in_tmux_re_attaches_to_the_running_agent`
- `a_tmux_run_enables_clipboard_forwarding_on_the_targets_tmux`
- `an_agent_run_in_tmux_outlives_its_dropped_connection`
- `an_agent_run_in_tmux_runs_in_a_tmux_session_named_for_the_session_on_the_target`
- `ending_a_tmux_run_kills_its_tmux_session_on_the_target`

## Causes

- **Unit tests: known.** `run_with_a_recording_tmux` (`target_tmux_tab_tests.rs:180`) runs the
  command with `/bin/sh`, an emptied environment and `PATH=<stand-in>:/usr/bin:/bin`. The command
  is `"${SHELL:-/bin/sh}" -lc '…'`, a login shell. On macOS `/etc/profile` runs `path_helper`,
  which rebuilds `PATH` from `/etc/paths` and `/etc/paths.d`, putting `/opt/homebrew/bin` ahead of
  the stand-in's directory. So the real tmux runs, the stand-in's log stays empty, and the test
  sees `[]` calls.
  Fix: give the child a `SHELL` stand-in that takes `-lc` without reading `/etc/profile`, as the
  acceptance tests' `login-shell` does (`remote_agent_launch_acceptance.rs:224`).
- **Acceptance tests: not yet known.** These tests already use a `login-shell` stand-in. One guess
  is ruled out: removing dev3's `~/.dev3.0/bin/tmux` shim from `PATH` (so `this_hosts_tmux()` finds
  `/opt/homebrew/bin/tmux`) changes nothing. Next step: run one test with `RUST_LOG=debug
  --nocapture` and read the remote command and the pane's output.

## Rule

Per "flaky/red is red", these are fixed in a separate PR (test harness only), not on #1068.
