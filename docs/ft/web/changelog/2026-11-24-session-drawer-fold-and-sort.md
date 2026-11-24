# 2026-11-24 — Ended sessions always fold under More; rows ordered by start and by stop

In each project group of the sessions drawer, ended sessions now always fold under the collapsed
**More**, including when nothing besides the pinned main is live. Previously, a project with a live
main above grey rows, or with only ended sessions, listed every ended row bare.

Active rows are listed newest-started first. A session started by another session sits right after
its starter, nested to any depth, and siblings are newest first too. Rows under **More** are listed
most recently touched first, by the later of the agent's last activity and the session's last update.

The daemon does not yet record the moment a session stops. A session that crashed or was killed is
placed by its last hook report (see `docs/dev/todo/2026-11-24-session-stop-time-not-recorded.md`).

See [Session drawer § Row order](../session-drawer.md#row-order).
