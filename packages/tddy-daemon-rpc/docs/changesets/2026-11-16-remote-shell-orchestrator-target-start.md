# 2026-11-16 — The orchestrator's start resolves its target on the daemon

**Type:** Feature · `#remote-shell` 11/11, PR [#927](https://github.com/uppin/tddy-coder/pull/927)

`DescribeSessionStartOptions` lists the described host's derived hosts, and the project-home start resolves `StartSessionArgs.target` for the caller before building the request, stamping the resolved parent and target. The exec-tool handler takes its host service through `attach_host_service`, attached by the daemon's composition root from the wired service; an unattached handler refuses naming the missing wiring rather than answering from a bare service. See the cross-package entry in `docs/dev/changesets/2026-11-16-remote-shell-orchestrator-target-start.md`.
