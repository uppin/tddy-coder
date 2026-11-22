# 2026-11-21 — The trunk watch reads github.com only

**Category:** Feature gap (left by `#orchestra` 5/9, #983)
**Source:** `packages/tddy-session-lifecycle/src/connection_service/svc_trunk_watch_wiring.rs` (`HostTrunkSource::poller_for`), `packages/tddy-github/src/conditional.rs` (`RestConditionalRead`, `GITHUB_API_BASE`) — the trunk is read through the GitHub inbox since `#workqueue` 2/14
**Consent:** the developer chose to defer this on 2026-11-22 at the `#orchestra` stack's TODO gate.

An orchestrator of a project whose remote is not on `github.com` — GitHub Enterprise, or any other host —
gets no `trunk_commit` activities. `poller_for` takes the repository from `owner_repo_from_remote_url`,
which recognises `github.com` remotes only, and the trunk-watch wiring builds its `RestConditionalRead`
with `GITHUB_API_BASE` (`https://api.github.com`). The project is logged (`tddy_daemon::trunk_watch`: "… names no github.com
repository") and left unwatched; the session runs on.

**Why deferred:** no project on a GitHub Enterprise host exists to test against, and the REST side would
need the API base derived from the remote's host (`https://<host>/api/v3`), which nothing in
`tddy-github` does yet. The Local gh path is closer: `GhCliConditionalRead`'s `gh` port already passes `--hostname`, so only
the slug parsing stands in its way.

**What would close it:** parse `owner/name` from any `https`/`ssh` remote whose host the project's identity
names (`github_hostname_of` already returns it for Local gh), hand `RestConditionalRead::new` an API base
chosen from that host (it already takes one — `#workqueue` 2/14 made it a constructor argument), key the
inbox's repositories by host as well as `owner/name`, and add a test for an Enterprise remote on each
transport.
