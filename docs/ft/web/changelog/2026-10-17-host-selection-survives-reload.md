# 2026-10-17 — The selected host survives a reload

- **A reload keeps the host the URL names.** While the common room is still joining, only the daemon
  that served the page is listed. The selection no longer snaps to that daemon, and `?host=` is no
  longer rewritten to it. A deep-linked session stays on its own host.
- **A chosen host is never replaced by the app.** `?host=` and the tab's stored selection win whether
  or not that daemon is listed. That includes a daemon that leaves the room mid-session. Only
  navigation by the operator changes it. An empty `?host=` is still filled in with the default.
- **The selector shows a waiting host.** Its last-seen label (or its instance id) is shown beside a
  pulsing amber dot with the tooltip "Waiting for daemon". The dot goes away by itself when the
  daemon is listed again.

See [URL state routing § Host in the URL](../url-state-routing.md#host-in-the-url) and
[Daemon selector](../daemon-selector-livekit-rpc.md).
