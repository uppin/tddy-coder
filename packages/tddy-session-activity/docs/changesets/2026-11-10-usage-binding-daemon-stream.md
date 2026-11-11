# 2026-11-10 — tddy-session-activity: StreamSessionUsage

`ActivityService.StreamSessionUsage`: one poller per watched session (`service/usage_pollers.rs`)
evaluates the spec's binding against the session's usage context every 2 s and streams each changed
snapshot; a dropped stream leaves the poller at once and the last one stops it. New port
`ActivityPorts::usage_bindings`. `#usage-binding 2/5`, PR [#891](https://github.com/uppin/tddy-coder/pull/891). See
[activity-service.md](../activity-service.md).
