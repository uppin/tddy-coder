# 2026-11-03 — A detached tmux-run agent's output is not recorded

**Category:** Deferred from "A tmux-run agent survives the daemon and is re-attached" (#815, `#tmux-agent 2/3`)
**Source:** `packages/tddy-cli-sessions/src/cli_session_manager/tmux_reattach.rs` (`DetachWatch`), `packages/tddy-cli-sessions/src/cli_session_manager/capture_recorder.rs`

## What happens

A tmux-run session's `main` recording is written by the tmux *client* — the PTY the daemon attaches
to the agent's tmux session. When that client ends while the agent lives (a daemon shutdown, a client
crash), the recording is finished at that moment. Everything the agent prints afterwards exists only
in tmux's scrollback.

- If a client re-attaches, tmux redraws the screen and the recording resumes from there: the gap is
  whatever scrolled past while nobody was attached.
- If the agent ends while detached (the watcher notices and cleans up), `GetTerminalHistory` serves
  the recording as of the client's exit, never the agent's final output.

## Why deferred

#815 is the survive-and-re-attach node; recording while detached needs a reader of the tmux server
that is not a client, which is a new capture path, and `tmux.rs` is shared with #816
(`#tmux-agent 3/3`) — a change there now conflicts with it.

## What would close it

**Recommendation — follow-up PR after the `#tmux-agent` stack lands.** On each poll of the detach
watcher (every `DETACH_WATCH_INTERVAL`, while the tmux session is alive), snapshot the pane with
`tmux capture-pane -p -e -J -S - -t <session>` as the session's OS user (a `tmux::capture_pane` op
beside `pane_pid`) and rewrite the main recording with it. The last snapshot before the session ends
is then the history. Acceptance case in `claude_cli_tmux_reattach_acceptance.rs`: detach, have the
stub agent print a marker and exit, assert `GetTerminalHistory` for `main` contains the marker.
