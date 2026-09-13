# 2026-08-14 — A split session's room is not re-opened when the daemon restarts

**Category:** Future enhancement
**Source:** session-room changeset, 2026-08-14

`SessionRoomRegistry` is built empty in `ConnectionServiceImpl::new`. A co-located session recovers
on its own — `ConnectSession` opens the room, which is the same path that opened it the first time —
but a split session's room is opened only by its start
(`SessionRoomRegistry::open_measured_by` on the split path) and its checkout is on another host, so
`ConnectSession` there finds no local `repo_path` and opens nothing. A split agent resumed against a
restarted daemon then finds no `daemon-{instance_id}` to address and its `connect_livekit_client`
wait times out after 10 s (`packages/tddy-tools/src/session_tool_client.rs:448`).

The fix is a startup sweep that re-opens a room for each split session whose `.session.yaml` names a
codebase daemon — the same shape as the existing startup reconciliation in
`packages/tddy-daemon/src/startup.rs`.

**Moved, not fixed, 2026-09-10.** `#unbundle` node 4
([#473](https://github.com/uppin/tddy-coder/pull/473)) moved `session_room` to
`packages/tddy-daemon-livekit`; `SessionRoomRegistry` is now
`tddy_daemon_livekit::session_room::SessionRoomRegistry`. The defect crossed unchanged, and it is
recorded here so a reader who meets it in the new crate knows it is inherited rather than
introduced. Note that the sweep would live in `tddy-daemon`'s `startup.rs` and call **into** this
crate — the direction the split already allows.
