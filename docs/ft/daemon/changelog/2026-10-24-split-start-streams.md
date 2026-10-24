# 2026-10-24 — A split start shows the codebase host's progress, with no deadline

When a session's codebase lives on another daemon, that daemon's start progress — its worktree step and
keep-alives — now reaches the create pane as it happens, and the agent step follows from the daemon
running the agent. The 330-second split-forward budget is gone: a start ends with a result, an error from
either host, or after 30 s of silence from the codebase host. Every host keeps a start alive every 5 s for
its whole duration, so a slow clone or worktree cut is never mistaken for silence. A codebase host's own
timeout (for example its clone deadline) is reported as that host's answer, not as the forward's. The
same applies to a forwarded unary `StartSession`. See
[remote-managed-worktree.md § Failure is atomic](../remote-managed-worktree.md) and
[cloning over SSH](../clone-ssh-passphrase.md).
