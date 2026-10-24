# 2026-10-24 — A stalled or impossible spawn says why

A child the daemon starts as another OS user — a session, a clone, a host probe — no longer hangs before
it starts. A daemon that cannot switch users refuses at once, naming both users; while a child starts,
the log records each step with its pid and repeats a step that does not finish every five seconds. A
session-start clone that fails or times out is logged on the cloning host. See
[project concept § Cloning at session start](../project-concept.md#cloning-at-session-start).
