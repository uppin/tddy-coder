# 2026-11-20 — A session is active only while the daemon holds a live agent tab for it

A session lists as active when the daemon holds at least one live agent tab for it — its main agent
or an agent terminal tab — instead of when the pid recorded in `.session.yaml` names a running
process, which read a recycled pid as a live session. Delete, `SignalSession` and the project-home
idle reaper act on the processes the daemon holds and signal no recorded pid; a signal to a session
with no running agent is refused. After a daemon restart sessions list inactive until started,
resumed or re-attached. See [session-liveness.md](../session-liveness.md).
