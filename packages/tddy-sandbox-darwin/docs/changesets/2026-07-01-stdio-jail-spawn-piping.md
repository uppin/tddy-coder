# 2026-07-01 — `--stdio` jail-spawn piping

**Type:** Feature

`spawn_plan` pipes the jailed process's stdin/stdout (instead of redirecting stdout to an egress log) when `--stdio` is present in the command, so `tddy-sandbox-runner --stdio`'s RPC framing can ride the real jail-spawned process's own stdio; proven end-to-end through a real Seatbelt jail (Echo/SessionChannel/tool-exec round trips). Non-`--stdio` spawns unchanged. Cross-package: [docs/dev/changesets/](../../../../docs/dev/changesets/). PR [#253](https://github.com/uppin/tddy-coder/pull/253). (tddy-sandbox-darwin, tddy-sandbox, tddy-daemon, tddy-sandbox-runner)
