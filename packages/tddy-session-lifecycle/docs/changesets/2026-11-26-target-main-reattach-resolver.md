# 2026-11-26 — The lazy main re-attach resolves a target session's location (#target-reconnect 2/3)

**Type:** Feature

`TmuxMainReattach` now holds the daemon's `TabTargetShell` and hands it to `reattach_recorded_main`, which resolves a target record's `TmuxLocation` from its `owner_subject` and attaches a transport client there. Startup re-attach of target chains is unchanged (node 3). Part of #1041.
