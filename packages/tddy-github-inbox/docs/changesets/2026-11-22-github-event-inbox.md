# 2026-11-22 — One conditional GitHub feed per repository

**Type:** Feature

New crate. `GithubInbox`: a row cache with per-request ETags, single-flight reads, a per-repository events
feed with a pending-check poll and an updated-pulls sweep, rate-limit pacing, kind-filtered subscriptions,
and `InboxTrunkFeed` for the trunk watch. Conversation rows carry GitHub's id on every review and comment.
See [architecture.md](../architecture.md). Full record:
[`docs/dev/changesets/2026-11-22-github-event-inbox.md`](../../../../docs/dev/changesets/2026-11-22-github-event-inbox.md) (#1003, `#workqueue` 2/14).
