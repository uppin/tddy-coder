# Trunk feed

`tddy_github::trunk_feed` reads a repository branch's newest commits **conditionally**, for the
daemon's trunk watch (`tddy_session_activity::trunk_watch`).

## `TrunkFeed`

`newest_commits(repo, branch, etag)` answers `Conditional::NotModified { poll_interval_ms }` or
`Conditional::Modified { value, etag, poll_interval_ms }` — the newest 100 commits, newest first
(`GET /repos/{owner}/{name}/commits?sha={branch}&per_page=100`), each as a `TrunkCommit` (sha, author
name, subject line, commit date, first parent). `X-Poll-Interval` (seconds) becomes
`poll_interval_ms`; any status other than 200 and 304 is a `TrunkReadError`.

| Transport | How it reads | Identity |
|---|---|---|
| `RestTrunkFeed` | `reqwest` to `https://api.github.com`, `If-None-Match` when an ETag is held | a `TokenSource` called on **every** read, so a rotated token is used from the next read on |
| `GhCliTrunkFeed` | a `GhApi` port running `gh api -i [--hostname] [-H If-None-Match: …] <path>` | the `gh` login of the OS user the port runs as |

`gh` exits non-zero for any non-2xx answer, a `304` included, while still printing the head: the
status line decides, never the exit code.

## `TrunkPoller`

One branch's polling rule as a step — `poll(now_unix_ms) -> PollOutcome { changes, next_poll_at_unix_ms }`
— with no loop and no clock of its own:

- the first answer is the baseline and reports nothing;
- after that, the commits above the last tip seen are reported once, oldest first
  (`TrunkChange::Landed`); a last tip missing from the answer is `TrunkChange::Rewritten { from, to }`
  — which a burst of more than 100 commits between two polls also reads as;
- the next poll is due after the answer's poll interval (`DEFAULT_POLL_INTERVAL_MS`, 60 s, when it
  names none), and at least `ERROR_BACKOFF_MS` (60 s) after a failed read;
- every `SWEEP_EVERY_MS` (15 min) the read ignores its ETag.

Only `github.com` remotes are read; Enterprise hosts are a backlog item.

Tests: `packages/tddy-github/tests/trunk_feed.rs`.
