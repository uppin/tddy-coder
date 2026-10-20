# 2026-10-20 — A ready PR builds on every commit

**Type:** Fix

The build gate decided "draft or not" from the `pull_request` event payload. Events are not delivered in
order, so the `synchronize` event of a push made while a PR was still a draft could arrive after the
`ready_for_review` event that followed it. That stale run read `draft: true`, skipped every job, and — the runs
share a concurrency group — cancelled the run that was building the ready PR. Result: a ready PR whose
checks were all *skipped* (observed on #630 and #631, 2026-10-19).

`.github/workflows/build-gate.yml` now asks the API for the PR's draft state when the gate runs, so every run
for a PR that is ready builds, whatever order its events arrive in. A draft still skips unless it carries a
`#forcebuild` comment. [CI guide § Draft PRs](../guides/ci.md#draft-prs).

Not changed: `forcebuild.yml`, the concurrency group, and the rule that a draft does not build.
