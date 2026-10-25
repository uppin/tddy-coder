# 2026-10-25 — Coding subagents in remote-codebase mode: the two-daemon LiveKit suite is not run

**Category:** Missing test
**Source:** `#coagents` 6/8 (#725, `feature/coagents/remote-codebase`)

Coding subagents on the codebase host ([remote-codebase-mode.md § Coding subagents on the codebase
host](../../ft/daemon/remote-codebase-mode.md#coding-subagents-on-the-codebase-host)) are verified
in-process only. `packages/tddy-daemon-rpc/tests/remote_codebase_clone_placement_acceptance.rs` puts
the relay and the codebase host in one process, with an in-process forwarder standing in for the
LiveKit hop. Unverified:

- the real LiveKit hop and the relay ↔ codebase-daemon handshake;
- prompt, await, permission resume and cancel of a conversation opened through the relay, end to end;
- the `subagent_*` tool advertisement by `tddy-tools --mcp` over the HTTP relay (the
  roster-before-tool-list test uses `TDDY_REMOTE_DAEMON_SOCKET`, the UDS transport);
- a conversation on the workspace worktree (non-`clone` placement) through the relay.

**Why deferred:** the suite needs the LiveKit testkit (`LIVEKIT_TESTKIT_WS_URL`), which was unset in
the environment that built #725, so it was not written and not run; nothing was faked. It also needs a
workspace project and repo fixture (today defined only inside
`packages/tddy-session-lifecycle/tests/workspace_session_acceptance.rs`), a coding-agent profile on the
codebase daemon, and the `tddy-acp-stub` binary.

**What closing it takes:** `packages/tddy-daemon-rpc/tests/remote_codebase_subagent_acceptance.rs` —
a relay daemon and a codebase daemon joined through the LiveKit testkit; `tddy-tools --mcp` with
`TDDY_REMOTE_DAEMON_URL` pointed at the relay advertises `subagent_*` for a workspace session started
with `--agent`; a conversation opened through it runs on the codebase host as the session user, and
prompt, await, permission resume and cancel reach it.
