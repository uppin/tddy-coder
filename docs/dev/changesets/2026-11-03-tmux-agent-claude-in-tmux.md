# 2026-11-03 — `#tmux-agent` 1/3: a Claude CLI session can run its agent inside tmux

**Type:** Feature

`StartSessionRequest.run_in_tmux` (field 47) and `SessionMetadata.tmux_session`. The tmux helpers moved from `tddy-session-lifecycle`'s `tmux_host.rs` into `tddy_cli_sessions::tmux` (a move; the tab code imports them from there), which adds the agent's tmux session operations. `tddy-agent-launch` starts and resumes a claude-cli agent in tmux (`claude_cli_tmux`), refuses `run_in_tmux` with `sandbox`, `sandboxed_codebase` or a split placement before routing, and kills the tmux session on Delete (by the derived name when the metadata is unreadable). `CliSessionManager::kill_all` kills main terminals' tmux sessions — never a tab's. The StartSession tool offers `run_in_tmux`; the web Create pane has the checkbox.

Decisions (developer, 2026-11-03): the name is `tddy-<full session id>`; the tool offers `run_in_tmux`; Delete with unreadable metadata kills the derived name; the three refusals; the pane environment handed over in a `0600` file under the session directory (`tmux-env/`) that the pane sources, deletes and `exec`s from — no value on any argv (developer, 2026-11-03, replacing `new-session -e`, which put the provider token on tmux's argv).

Code issues: `complexity-svc-resume-claude-cli-session` re-measured 142 → 142, unchanged (green had grown it to 147; deferred by the developer, still open); `oversized-file-create-session-pane` regressed 502 → 509 (stack-shared with #815/#816, split after the stack lands). `svc_start_session_core.rs` unchanged.

Backlog: *A tmux-run agent's environment travels on tmux's argv* (`2026-11-03-tmux-agent-env-on-the-tmux-argv`) was found by this PR's validation and resolved here (the env file), so the entry is deleted. Touched, still open — `2026-11-02-tmux-listing-and-tab-use-two-privilege-drops` (same split, not widened) and `2026-09-06-unprivileged-daemon-cannot-probe-another-os-user`. Part of [#814](https://github.com/uppin/tddy-coder/pull/814).
