# 2026-11-15 — A PTY child reports its real exit status

**Type:** Fix

The exit monitor maps `portable_pty::ExitStatus` through `exit_status::task_status_of`: a
normal exit keeps its code and an exit by signal is a failure naming the signal, instead of
every exit reading as code 0.

Cross-package record: [docs/dev/changesets/2026-11-15-split-agent-liveness.md](../../../../docs/dev/changesets/2026-11-15-split-agent-liveness.md).
