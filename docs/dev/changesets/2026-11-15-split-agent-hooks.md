# 2026-11-15 — A split agent loads its hooks file and carries its spec's launch variables

**Type:** Fix

A split session's agent loads the hooks file the daemon writes into its context dir:
the agent spec's `host_agent` variant declares the flag that loads a hooks file
(`hooks_flag: --settings` for Claude, rendered by `CodingAgentSpec::variant_hooks_args`),
and every split launch — start and resume, both through `split_claude_extra_args` — passes
it with `split_hooks_file(session_dir)` (`<context>/.claude/settings.local.json`).
`--setting-sources user` and `--strict-mcp-config` stay, so the checkout's own settings
still configure nothing. The split agent's process env also carries the spec's `run.env`
(`CLAUDE_CODE_NO_FLICKER=1` for Claude) on the start path (`split_agent_pty.rs`) and on
resume (`SplitAgentWiring.env` from `prepare_split_agent_wiring`). Stack `#split-alive`
2/4 ([#906](https://github.com/uppin/tddy-coder/pull/906)); the product statement is in
[remote-managed-worktree.md § Session start](../../ft/daemon/remote-managed-worktree.md#session-start).

This closes fix 3 of the backlog entry `2026-11-15-split-session-agent-not-running`
(deleted at the #905 wrap), together with the `run.env` divergence noted there. What
remains of that entry is recorded in
`docs/dev/todo/2026-11-15-split-agent-exit-cause-unknown.md`, updated at this wrap to
mark fix 3 done in #906: why the agent exits (fix 4) is still unknown, and fixes 5 and
6–7 land in #907 and #908.

Validation at this wrap: `./test -p tddy-coding-agent-specs -p tddy-session-split`
green, `cargo fmt --check` clean, scoped `cargo clippy -p tddy-coding-agent-specs
-p tddy-session-split -- -D warnings` clean (via `./dev`, `CARGO_INCREMENTAL=0`).
File-length gate over the PR range: no touched non-test source file at or over 500
production lines (`spec.rs` 433 → 461, `split_session.rs` 419 → 424,
`agent_argv.rs` 263 → 275). Code issues re-measured: `complexity-slots-mod-render`
(65 lines, `slots::render`) untouched by this PR and left open as written;
`complexity-svc-spawn-split-agent-spawn-split-agent` untouched by this PR (152 lines,
recorded in #905) and left open — `svc_spawn_split_agent.rs`, `split_session.rs` and
`agent_argv.rs` are also modified by later stack nodes, so any split is deferred to a
follow-up branch after stack `#split-alive` lands.
