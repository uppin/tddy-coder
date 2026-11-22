# 2026-11-22 — The trunk watch reads through the GitHub inbox

**Type:** Feature

`svc_trunk_watch_wiring.rs`: the host owns one `GithubInbox`, steps it in the trunk-watch task, and hands each
project's `TrunkPoller` an `InboxTrunkFeed`. `HostRepoReaders` registers, per repository, the reader of the
project that subscribed last — `GhCliConditionalRead` for a Local gh project, `RestConditionalRead` with the
project's vault token otherwise (backlog: `docs/dev/todo/2026-11-22-github-inbox-reads-a-repository-with-one-projects-credential.md`).
Full record: [`docs/dev/changesets/2026-11-22-github-event-inbox.md`](../../../../docs/dev/changesets/2026-11-22-github-event-inbox.md) (#1003, `#workqueue` 2/14).
