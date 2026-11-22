# 2026-11-21 — A dead main jail ends its terminal

**Type:** Bug Fix · **Stack:** `#tab-hardening` 3/4 (PR #991)

The launch paths (`jail_launch`, `tab_jail_launch`) attach the `JailBridge` that `dial_and_bridge_with_egress` returns to the jail's state.

Full record: [`docs/dev/changesets/2026-11-21-jail-death-watch.md`](../../../../docs/dev/changesets/2026-11-21-jail-death-watch.md); feature: [`session-liveness.md`](../../../../docs/ft/daemon/session-liveness.md#a-dead-main-jail).
