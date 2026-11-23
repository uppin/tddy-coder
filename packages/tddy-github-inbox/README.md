# tddy-github-inbox

The daemon's GitHub inbox: **one conditional feed per repository** and a cache of rows — a PR, the
PR for a head branch, the checks at a commit, a PR's conversation, a PR's commits, a branch's newest
commits — each revalidated with its own ETag. Events invalidate exactly the rows they affect,
concurrent reads of one row make one request, and in-process subscribers hear about the kinds of
change they asked for. It is the daemon's shared, quota-aware path for reading GitHub PR state.

| Module | Owns |
|---|---|
| `lib.rs` | `GithubInbox` (typed reads, `subscribe`, `step`), `RowKey`, `Cached`, `InboxEvent`, `EventKind`, `InboxError`, the `RepoReaders` port |
| `repo.rs` | one repository's state and the single-flight read path |
| `rows.rs` | how each row is fetched and parsed conditionally |
| `feed.rs` | the events feed, the pending-check poll and the updated-pulls sweep |
| `pacing.rs` | poll interval, rate-limit and failure scheduling |
| `subscriptions.rs`, `in_flight.rs`, `paths.rs`, `trunk.rs` | subscribers, per-row turns, request paths, `InboxTrunkFeed` |

Depends on `tddy-github` only (the transport and the DTOs).

## Documentation

- [docs/architecture.md](docs/architecture.md) — rows, freshness, the feed, rate limits, ports
- [docs/changesets/](docs/changesets/) — change history
