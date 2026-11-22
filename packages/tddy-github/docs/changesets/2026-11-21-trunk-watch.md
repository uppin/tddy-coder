# 2026-11-21 — New trunk commits reach every live orchestrator as activities

**Type:** Feature

`trunk_feed` (new): `TrunkFeed` — a conditional read of a branch's newest commits over REST (`RestTrunkFeed`, its token read on every read) and `gh api -i` (`GhCliTrunkFeed`) — and `TrunkPoller`, the polling rule as a clock-free step. See [trunk-feed.md](../trunk-feed.md). Full record: [`docs/dev/changesets/2026-11-21-trunk-watch.md`](../../../../docs/dev/changesets/2026-11-21-trunk-watch.md) (#983, `#orchestra` 5/9).
