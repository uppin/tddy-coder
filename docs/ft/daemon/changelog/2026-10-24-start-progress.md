# 2026-10-24 — Session creation you can watch and cancel

The `#start-progress` stack (#734–#741) makes a session start, local or split, show what its host is
doing while it does it: cloning (git's stage, percent, bytes and throughput), cutting the worktree,
building the index, starting the agent. Nothing on that path gives up on a timer. A start ends when it
succeeds, fails with its cause in the error and the daemon log, has a host go silent, or the operator
presses **Cancel**.

This answers the 2026-10-24 split-start incident. A pre-supervisor `daemon.yaml` on a supervised host
cloned through an unprivileged forked worker. The clone child deadlocked before `exec`, the peer's
30 s clone timeout was misread as the forward's own, the worker stayed wedged, and the create pane
showed nothing. The stack's slices:
[spawn diagnostics and the pre-exec deadlock](../systemd-install.md), the concurrent forked worker,
[the supervisor handoff](../../supervisor/tddy-supervisor.md), the streamed
[split start](../remote-managed-worktree.md#session-start), clone progress with no deadline
([project concept](../project-concept.md#cloning-at-session-start)) on both clone backends,
[the step list](../../web/session-drawer.md#start-progress) and
[Cancel](../background-tasks.md#session-starts-run-as-tasks).

Unchanged: a start's result (one session, or an error); the 30 s relay idle timeout on a forwarded
stream, which every hop's keep-alives keep from firing on a live start;
`spawn_worker_request_timeout_secs` for everything other than session creation; CreateProject and
AddProjectToHost stay unary.
