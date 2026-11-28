# GitHub 403: agent sessions poll CI in hand-written sleep loops

**Status:** open, investigation only (no fix applied).

## Limit that was hit
The 403 is the **account's 5,000/h REST limit**, not the 60/h IP limit. The recorded error is
`HTTP 403: API rate limit exceeded for user ID 2762334` (in `~/.tddy/sessions/*/agent-activity.jsonl`,
in records from before the 24 h window; they have no `started_unix_ms`). At the time of
investigation (2026-11-28 09:20), `gh api rate_limit` showed core 168/5000 used, graphql 29/5000 and
search 0/30. No live pollers: `ps … | grep -E "gh (run|pr|api)|ci-status"` returned nothing.

## Evidence: last 24 h, agent tool calls that run gh (deduplicated by call_id)
Invocations per hour: 11-27 16h 78 · **17h 130** · 18h 61 · 19h 43 · **20h 121** · **21h 104** · 23h 90 ·
11-28 01h 79. Each is one Bash call; a call can make many requests.

By kind: `gh pr` 367 · `gh api` 282 · `gh stack` 154 · `gh issue` 24 · `gh run` 10 · `ci-status.sh` 7 ·
`gh run watch` 4 · `gh pr checks --watch` 1.

Top sessions: 01a21eb5 (121), 01a21d34 (104), 01a21e4c (79), 01a21e22 (78), 01a21d58 (71). Most of them
work on **wix-private/wixel-pdf** from the coder host, and they share the operator's account budget.

The main cost multiplier is loops that agents write themselves instead of `ci-status.sh --watch`. Examples:
- 01a21d58 11-27 18:45 `for i in $(seq 1 24); do sleep 40; gh api repos/…/commits/$SHA/check-runs` → 24 requests
- 01a21d58 11-27 18:24 `for i in $(seq 1 28); do sleep 40; gh pr view 15 …; gh pr view 16 …` → ≥56 requests
- 01a21e4c 21:23–00:26: five loops of `seq 1 11; sleep 55; gh pr checks` → ≥55 `gh pr checks` (each one makes several GraphQL/REST requests)
- 01a21eb5 11-27 23:33 `gh pr checks 1055 --watch` (polls every 10 s until done)
- 01a21e91 23:18–23:36: four loops of 8–12 iterations at 30–45 s on `gh api …/check-runs`

Estimated peak: about 300–600 requests/h from agents (not measured, because gh does not log
per-request). That is well under 5,000 for one hour. Exhausting the budget needs several such
sessions running at once, or a forgotten `gh run watch` (about 49 req/min, see the earlier incident on 2026-11-21).

## Suggested direction (needs operator approval)
- Give agents one shared, quota-aware wait instead of ad-hoc loops, e.g. a `tddy-tools` command
  backed by the daemon's GitHub inbox (it already uses ETag and 304s, see `packages/tddy-github-inbox/src/lib.rs:1-14`).
- Have the wixel-pdf repo's agent guidance forbid sleep-loop polling, as this repo's CLAUDE.md does.
