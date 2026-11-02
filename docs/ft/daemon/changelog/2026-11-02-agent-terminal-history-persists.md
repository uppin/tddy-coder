# 2026-11-02 — A stopped agent terminal keeps its history

What a session's agent printed outlives its process. The daemon records the agent terminal (`main`)
to `<data dir>/sessions/<id>/terminals/main.capture` every 30 seconds while it runs and once more when
it exits — for a CLI session on start and on resume, and for a jailed session until its sandbox state
is removed. `GetTerminalHistory` for `main` of a stopped session answers from that recording with the
offsets the live terminal had, so scroll-up history pages exactly as before, also after a daemon
restart. A running agent terminal still answers from its live capture; a stopped one stays read-only
(streaming and input remain `NOT_FOUND`), and a session that never recorded answers `NOT_FOUND`. See
[Terminal sessions § History of a stopped agent terminal](../terminal-sessions.md#history-of-a-stopped-agent-terminal).
