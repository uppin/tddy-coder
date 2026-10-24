# 2026-10-24 — A split start streams the codebase host's progress, with no deadline

**Type:** Feature
**Stack:** `#start-progress` 4/8 ([#737](https://github.com/uppin/tddy-coder/pull/737)).
**Packages:** tddy-daemon-kernel, tddy-session-split, tddy-session-agents, tddy-agent-launch, tddy-session-files, tddy-daemon-livekit, tddy-session-lifecycle (tests)

Every start forwarded to a codebase host — the split start, the agent-clone provision and the plain
`StartSession` forward — is a relayed `StreamStartSession` (`peer_calls::forward_start_session_relaying`)
bounded only by the relay's per-frame idle timeout. The peer's events reach the caller's sink in order;
every streamed start emits keep-alives for its whole life. A peer's status (its own `DEADLINE_EXCEEDED`
included) is `PeerAnswered`; only idle expiry is `PeerSilent`, which alone keeps the orphan warning.
`split_forward_deadline`, `start_forward_deadline` and the worktree-cut `request_timeout` on the session
path are gone. Split starts report their phases (worktree from the codebase host, agent locally).

Docs: [remote-managed-worktree.md § Failure is atomic](../../ft/daemon/remote-managed-worktree.md),
[clone-ssh-passphrase.md](../../ft/daemon/clone-ssh-passphrase.md),
[daemon-kernel.md](../../../packages/tddy-daemon-kernel/docs/daemon-kernel.md).

Backlog: narrowed [start phases](../todo/2026-10-03-start-phases-and-code-index-warm-skip-sandboxed-tool-and-split-starts.md)
(split part resolved) and the first bullet of [clone passphrase gaps](../todo/2026-10-17-clone-passphrase-prompt-verification-gaps.md);
added [in-memory peer-forwarding testkit](../todo/2026-10-24-peer-forwarding-tests-use-livekit-below-the-binary-boundary.md)
and [keep-alive through a slow worktree cut](../todo/2026-10-24-streamed-start-keepalive-through-a-slow-worktree-step-untested.md).
Not written: the planned cross-host LiveKit case `a_split_start_streams_the_codebase_hosts_progress_across_livekit`
and `a_streamed_start_sends_keep_alives_through_a_slow_worktree_step` (both covered by those TODOs).
Code issues deferred with the developer's consent: `start_split_claude_cli_session` 149 → 153;
`provision_agent_clone` 86 (new record).
