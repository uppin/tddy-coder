# 2026-11-22 — Conditional reads, shared PR parsing, richer check runs

**Type:** Feature

`conditional` (new): `ConditionalRead` over REST (`RestConditionalRead`, API base a constructor argument) and
`gh api -i` (`GhCliConditionalRead`), answering status, ETag, `X-Poll-Interval`, rate limit and `Retry-After` —
see [conditional-read.md](../conditional-read.md). `pr_json` / `check_runs` (new) parse the PR family's JSON;
`RealGithubPrApi`'s insight reads delegate to them. `CheckRun` gains `id`, `status: CheckStatus`, `started_at`,
`completed_at`, `details_url` and `Default`; `PrReview` and `PrIssueComment` gain `id`. `RestTrunkFeed` and
`GhCliTrunkFeed` are removed — the trunk is read through the GitHub inbox. Full record:
[`docs/dev/changesets/2026-11-22-github-event-inbox.md`](../../../../docs/dev/changesets/2026-11-22-github-event-inbox.md) (#1003, `#workqueue` 2/14).
