# 2026-11-19 — A hook-log follow ends once its log is removed

**Type:** Fix

`follow_command` runs its `tail -f` beside a background watch that kills it within a second of the
log's removal, so a session leaves no `tail` on its target after its end — though ending the
client never ended the remote command. See [module-layout.md](../module-layout.md). Full record:
[`docs/dev/changesets/2026-11-19-session-driver-hooks.md`](../../../../docs/dev/changesets/2026-11-19-session-driver-hooks.md).
