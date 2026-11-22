# GitHub inbox — architecture

`tddy_github_inbox::GithubInbox` is the daemon's shared, quota-aware path for reading GitHub pull
request state. One `GithubInbox` serves every repository; each repository it reads has its own
`RepoInbox` inside (rows, subscribers, schedule, feed memory).

## Rows

A **row** is one cached GitHub resource, keyed by `RowKey`:

| `RowKey` | Read | Value | Event kind |
|---|---|---|---|
| `Pr { number }` | `GET pulls/{n}` | `PrDetail` | `Pr` |
| `PrForHead { branch }` | `GET pulls?state=all&head={owner}:{branch}&per_page=1` | `Option<PrDetail>` | `Pr` |
| `ChecksAt { sha }` | `GET commits/{sha}/check-runs` | `Vec<CheckRun>` | `Checks` |
| `Conversation { number }` | `GET pulls/{n}/reviews`, `pulls/{n}/comments`, `issues/{n}/comments` | `PrConversation` | `Comments` |
| `PrCommits { number, head_sha }` | `GET pulls/{n}/commits` | `Vec<PrCommit>` | `Pr` |
| `BranchCommits { branch }` | `GET commits?sha={branch}&per_page=100` | `Vec<TrunkCommit>` | `Push` |

The request paths live in `paths`, so a scripted GitHub in tests answers exactly what production
asks. Every row keeps **one ETag per request it is made of** and is revalidated with them: a `304`
keeps the value (and costs no quota), a `2xx` replaces it. A multi-request row (`Conversation`)
re-parses only the parts that changed.

The typed reads are `pr`, `pr_for_head`, `checks_at`, `conversation`, `pr_commits` and
`branch_commits`; each answers `Cached<T> { value, fetched_at_unix_ms, etag }` or an `InboxError`
(`Unavailable`, `RateLimited { reset_at_unix_s }`, `NotFound`). `peek_pr_for_head` answers what is
cached without ever making a request.

## Freshness

A row is answered from the cache while it is **fresh**:

- **not stale** — no event has invalidated it since it was read, and
- **kept fresh by a feed** — the repository has a live subscriber, whose feed invalidates the row
  when it changes; **or**, with no subscriber, the row is younger than `FEEDLESS_ROW_TTL_MS` (30 s).

Otherwise the next read revalidates it. **Single-flight** (`in_flight::InFlight`): a reader takes the
row's turn before asking GitHub and looks at the cache again once it has it, so concurrent reads of
one stale row make one request.

## The feed

`step(now_unix_ms)` advances every repository that has a live subscriber (a repository nobody
subscribes to is not polled; its rows age out by the TTL instead). Per repository:

- **Events** — `GET repos/{o}/{n}/events?per_page=100`, conditional on the last ETag, no oftener
  than GitHub's `X-Poll-Interval` (`DEFAULT_POLL_INTERVAL_MS`, 60 s, when it names none). The first
  answer is the baseline; afterwards each event not seen before invalidates its rows, oldest first:

  | Event | Rows marked stale and announced |
  |---|---|
  | `PullRequestEvent` | `Pr{n}`, `PrForHead{head}`, and any cached `PrCommits{n, …}` |
  | `PushEvent` | `BranchCommits{branch}`, `PrForHead{branch}` |
  | `IssueCommentEvent` on a PR, `PullRequestReviewEvent`, `PullRequestReviewCommentEvent` | `Conversation{n}` |

- **Pending-check poll** — check runs are not in the events feed, so every cached `ChecksAt` row
  holding a queued or in-progress run is revalidated every `PENDING_CHECK_POLL_MS` (30 s), and
  announced when it changed. No pending checks, no poll.
- **Updated-pulls sweep** — every `SWEEP_EVERY_MS` (15 min), while the quota is comfortable
  (≥ 4 × `LOW_QUOTA_REMAINING`), `GET pulls?state=all&sort=updated` conditionally; when it changed,
  every cached row of a listed PR is invalidated. It catches what the events feed missed.

`StepReport` gives the requests made, the earliest time the next step has work
(`next_step_at_unix_ms`) and the tightest rate limit seen.

## Rate limits

Every answer's `X-RateLimit-Remaining`/`Reset` is recorded. Below `LOW_QUOTA_REMAINING` (100) the
repository's feed pauses until the window resets, and reads answer from the cache however stale; a
read with nothing cached answers `RateLimited`. A failed feed read waits out `Retry-After`, at least
60 s.

## Subscriptions

`subscribe(repo, kinds)` answers an `InboxSubscription` (`try_next`, `next`) carrying
`InboxEvent { repo, kind, row }` for rows of those kinds. Dropping it unsubscribes; a repository
whose last subscriber dropped stops its feed. Subscribers hear *that* a row changed; they read the
new value through the typed reads.

## Time

The feed's schedule is driven only by the injected `step(now)`. Row freshness and the quota pause on
the read path use the system clock; in the daemon both are the same clock.

## Ports

- `RepoReaders::reader_for(repo)` — which `tddy_github::conditional::ConditionalRead` reads a
  repository, resolved on every read. The daemon's implementation (`HostRepoReaders` in
  `tddy-session-lifecycle`'s trunk-watch wiring) registers, per repository, the reader of the
  project that most recently subscribed to its trunk: REST with the project's vault account token
  (`RestConditionalRead`, token resolved per read) or the host's `gh` as the session's OS user
  (`GhCliConditionalRead`).
- `InboxTrunkFeed` — a `tddy_github::trunk_feed::TrunkFeed` over the `BranchCommits` row, which is
  how the trunk watch's `TrunkPoller` reads through the inbox unchanged.

## Consumers

| Consumer | Reads |
|---|---|
| Trunk watch (`svc_trunk_watch_wiring.rs`) | `branch_commits` via `InboxTrunkFeed`; steps the inbox in its task |

## Limits

- In memory only: a daemon restart costs one unconditional read per row.
- One events page (100): a burst of more than 100 events between two polls can miss a push; PR rows
  are caught up by the sweep.
- `github.com` only (`GITHUB_API_BASE`).

Tests: `tests/inbox_feed_acceptance.rs` (scripted `ConditionalRead`, injected time);
`packages/tddy-github/tests/conditional_read_over_http.rs` for the transport.
