# The crate's test suites

`tests/` holds **56 integration suites**. Together they are this crate's acceptance coverage:
session start, resume, split and deletion; the sandboxed Claude and Cursor CLI paths; terminal
control and replay; staging, uploads and session files; peer and cross-host forwarding; and the
`DaemonRpcFamilies` port (`rpc_families_port_acceptance.rs`: an unwired host reports the missing
wiring, a wired one hands back what it was given). The Telegram control surface and its notifier are
tested where they live, in [`tddy-telegram-control`](../../tddy-telegram-control/README.md), and the
Project, Catalog, ExecTool and PR-stack families' suites where those families live, in
[`tddy-daemon-rpc`](../../tddy-daemon-rpc/docs/architecture.md#tests). The same goes for the modules
that live below this crate: `action_service_acceptance` and `action_sandbox_acceptance` are
`tddy-daemon-sandbox`'s, and `worktree_removal_eligibility` is `tddy-session-activity`'s.
`task_service_acceptance` stays here, because three of its tests drive this crate's
`ClaudeCliSessionManager`.

`./test -p tddy-session-lifecycle` therefore exercises the crate. That is the point of them being
here: the same suites, reaching the same code through `tddy-daemon`'s re-export facade, left the
crate looking untested from inside it and untestable on its own — the worst property a coverage gap
can have, because nothing in the crate shows it.

### Tab jails (`#session-liveness` 5/8, 7/8)

`sandbox_shell_tab_acceptance`, `sandbox_agent_tab_acceptance` (a jailed agent tab: its own jail and conversation, liveness, the main's records untouched, teardown), `sandbox_shutdown_acceptance` and the jailed-tab cases of
`resume_tab_roster_acceptance` drive a sandboxed session's shell tabs through the daemon's own terminal RPCs,
roster and store, with an in-memory `TabJailSpawn` injected by `DaemonSessionHost::with_tab_jail_spawn`
(`tests/sandbox_tab_support/`): each "jail" is a `sleep` standing in for its runner, whose terminal has already
printed a banner naming its tab, so lifetime, death, teardown and routing are observed on real processes. The
kernel sandbox itself is `sandbox_shell_tab_jail_linux`'s, which self-skips off Linux and needs a VM
(`docs/dev/todo/2026-11-02-ci-runs-no-linux-jail.md`).

### Target tabs (`#session-liveness` 6/8)

`target_shell_tab_acceptance` and the target-tab cases of `resume_tab_roster_acceptance` drive a target session's
shell tabs through the daemon's own terminal RPCs and roster, with an in-memory `TabTargetShell` injected by
`DaemonSessionHost::with_tab_target_shell` (`tests/target_tab_support/`): the "target" is a directory on this host
behind a stand-in ssh whose login shell records where it started, so the worktree a tab runs in, the recorded
chain, the owner the target was resolved as and Resume's reopen-or-drop are observed on real processes. The
transport itself (`ssh`, `coder ssh`) is `tddy-agent-launch`'s `target_shell_tab_acceptance`.
`target_agent_tab_acceptance` (`#session-liveness` 8/8) opens an agent "On target" through the same roster and
asserts the `[target, agent]` record; the target-agent case of `resume_tab_roster_acceptance` reopens a recorded
claude tab on the target resuming its conversation. The agent's own run on the target (worktree, no host copy of
the agent, the chat minted there, no host path handed over) is `tddy-agent-launch`'s `target_agent_tab_acceptance`.
`#tab-hardening` 1/4 extends it with the error contract: an unreachable target is `UNAVAILABLE`, an agent the target
lacks is `FAILED_PRECONDITION` naming it (the fake's login `PATH` is its agents directory plus `/usr/bin:/bin`, so this
host's copy never answers), and a terminal this host cannot spawn is `INTERNAL`
(`target_shells_whose_client_this_host_lacks`); `resume_tab_roster_acceptance` drops such an agent tab on Resume with that reason.

## Writing one

- A suite goes in the crate whose production code it exercises. `tddy-daemon` keeps only the suites
  that mount the composition root — see [its rule](../../tddy-daemon/docs/test-placement.md).
- Import what the suite exercises **from the crate that defines it**. `tddy_session_lifecycle::…`
  for this crate's own modules; the owning crate directly for anything this crate re-exports.
- A suite that opens a session room without exercising the four families served above this crate
  installs `test_util::RpcFamiliesNotUnderTest` with `with_rpc_families`. A suite that asserts one
  of those families — through a room or a stack link — belongs in `tddy-daemon-rpc`, whose
  `test_util::TestDaemon` installs the real `RpcHandlers`.
- There is no `tests/common/` module. The two tmux tab suites (`tmux_tabs_acceptance`,
  `tmux_new_session_acceptance`) share a private tmux server and a one-session daemon through
  `tests/support/mod.rs`. The PTY wait the CLI suites share is
  `tddy_testing_commons::wait::a_capture_showing`, with its `PTY_STUB_OUTPUT` ceiling, so the
  Telegram start suites in `tddy-telegram-control` use the same one.
- Crates a suite needs go in `[dev-dependencies]`, never `[dependencies]` — a crate only a test
  needs is not one the library needs, and declaring it in `[dependencies]` makes every consumer
  rebuild it.

## What a local run shows

`./test -p tddy-session-lifecycle` stops at the first red suite, and `./test` passes its arguments
before its own `-- --test-threads=1`, so a libtest `--skip` becomes a filter. The full local run is
therefore cargo directly, in the dev shell:

```bash
cargo test -p tddy-session-lifecycle --no-fail-fast -- --test-threads=1
```

On a macOS developer host this gives 59 targets (58 test binaries plus doctests), **575 passed,
22 failed, 1 ignored**. The 22 are environmental, not defects in the code they cover:

| Suite | Red | Why |
|---|---:|---|
| `sandbox_behavior_acceptance` | 5 of 5 | `sandbox RPC bridge not installed — runtime must call install_sandbox_rpc_bridge`: the harness never installs the bridge, so every sandboxed start panics |
| `sandboxed_claude_cli_acceptance` | 5 of 5 | the same |
| `sandboxed_cursor_cli_acceptance` | 4 of 4 | the same |
| `sandboxed_session_lifecycle_acceptance` | 2 (`delete_sandbox_session_stops_child_and_removes_directory`, `resume_sandbox_session_respawns_and_updates_pid`) | the same |
| `session_sync_livekit_acceptance` | 6 of 6 | `tddy-remote-git-repo is not built`: `./test`'s prebuild does not build it |

`sandboxed_cli_harness_acceptance` (OpenCode and Muse Code, #670) installs the bridge with
`install_sandbox_rpc_bridge` and passes against a real Seatbelt jail, and so do the suites that pin
the sandboxed Claude start and relaunch on the shared jail launcher (`LaunchSessions::launch_jailed`,
[`tddy-agent-launch` module layout](../../tddy-agent-launch/docs/module-layout.md#the-sandboxed-starts-and-relaunch)).
All three are `#![cfg(target_os = "macos")]` — no CI host runs a Linux jail
([todo](../../../docs/dev/todo/2026-11-02-ci-runs-no-linux-jail.md)):

| Suite | What it pins |
|---|---|
| `sandboxed_claude_launch_baseline` | the start and the relaunch through a recording stub runner: argv, env, mounts and files; the jail home at `<tddyhome>/homes/claude` |
| `claude_cli_jail_from_spec_acceptance` (fixture `claude_jail_support/`) | a stub `claude` in a real jail sees the spec's argv, `HOME`, `PATH`, proxy env, MCP config and credentials; no `TDDY_*` variable on the agent and the ones it needs on its `tddy-tools` server; the relaunch renders the same spec; a declared host reached through the shim and an undeclared one refused; a routed session reaches its base-URL host and carries no OAuth token secret; `sandbox-claude-home` migrated to `homes/claude` once; no Claude flag on the runner's argv |
| `ensure_project_home_session_acceptance::a_project_home_main_set_up_in_a_sandbox_is_listed_with_session_role_project_home` | a jailed main keeps `session_role: project-home` |

`sandboxed_claude_cli_acceptance`, `sandboxed_cursor_cli_acceptance`, `sandbox_behavior_acceptance`
and the two `sandboxed_session_lifecycle_acceptance` tests above stay red on the bridge: their
fixtures do not install it.

## Session liveness

A session is active while the daemon holds a live agent tab for it ([Session
liveness](../../../docs/ft/daemon/session-liveness.md)); a pid written into `.session.yaml` makes it
active no longer. `TestDaemon::holds_a_main_agent(session_id)` (`src/test_util.rs`) makes a session live
the way the daemon defines it — it holds a main agent terminal, a stub that records the first `INT` or
`TERM` it receives (`HeldMain::signal_received`) and is ready once its traps are installed;
`serves_a_main_for` and `stops_holding` read and end that hold.

| Suite | What it pins |
|---|---|
| `session_liveness_acceptance` | a recorded pid belonging to an unrelated live process lists inactive; a running main lists active with no recorded pid; a main that exited while an agent tab runs lists active; closed agent tabs, a shell-only session and a daemon restart list inactive; delete leaves the process the recorded pid names running and stops the session's agent tabs; `SignalSession` reaches the held main, not the recorded pid; a session with no running agent is refused and nothing is signalled |
| `tab_roster_acceptance` | an agent tab, a shell and a tmux-attach tab are each recorded in `tabs.yaml` with their launch chain; closing a tab removes it; a tab that exits on its own stays |
| `tmux_wrapped_tab_acceptance` | a tmux-wrapped agent tab runs in a tmux session of its own and lists its chain; it keeps its session live after main exits and while detached, until its tmux session ends; closing it ends its tmux session and leaves the roster; a wrapper other than tmux is refused naming it; a fresh daemon re-attaches live wrapped tabs, and a pre-roster tmux main, at startup without relaunching the agent |
| `resume_tab_roster_acceptance` | Resume restores every roster tab under its recorded id (stub claude, cursor with `create-chat`, opencode): a claude tab resumes its own conversation, a cursor tab its minted chat, an opencode tab reopens fresh and is reported `FRESH`; a live tmux-wrapped tab is re-attached, not relaunched, and a gone one is relaunched and resumed; a `tmux-attach` tab whose session is gone is dropped and reported; a tab live at Resume is not restarted |
| `claude_cli_tmux_reattach_acceptance` | a tmux-run agent that outlived its daemon lists inactive until its main is re-attached, and a detached one lists active while the daemon watches it; each re-attach attempt is awaited through `reattach_finished()` rather than polled |
| `ensure_project_home_session_acceptance::ensuring_a_main_whose_recorded_pid_names_an_unrelated_process_resumes_it` | a main whose recorded pid is a running unrelated process is resumed, not reported running |
