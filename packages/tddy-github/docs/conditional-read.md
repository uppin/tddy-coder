# Conditional reads

`tddy_github::conditional` is the transport under the daemon's GitHub inbox
(`packages/tddy-github-inbox/docs/architecture.md`): a `GET` that says whether anything changed and
carries what a shared, quota-aware reader needs.

## `ConditionalRead`

`get(path, if_none_match)` — `path` is relative to the API root and carries its query
(`repos/o/n/pulls/1`). It answers:

- `ConditionalAnswer::NotModified { poll_interval_ms, rate_limit }` for a `304`;
- `ConditionalAnswer::Modified { body, etag, poll_interval_ms, rate_limit }` for a `2xx`;
- `GithubReadError { status, message, retry_after_s, rate_limit }` for a transport failure or any
  other status.

Headers read from **every** answer, whatever its status: `ETag`, `X-Poll-Interval` (seconds → ms),
`X-RateLimit-Remaining` + `X-RateLimit-Reset` (`RateLimit { remaining, reset_at_unix_s }`, only when
both are present) and `Retry-After`.

| Transport | How it reads | Identity |
|---|---|---|
| `RestConditionalRead::new(api_base, token)` | `reqwest` to `api_base` (`GITHUB_API_BASE` = `https://api.github.com`), `If-None-Match` when an ETag is held, `X-GitHub-Api-Version: 2022-11-28` | a `TokenSource` called on **every** read, so a rotated token is used from the next read on |
| `GhCliConditionalRead::new(gh)` | a `GhApi` port running `gh api -i [--hostname] [-H If-None-Match: …] <path>` | the `gh` login of the OS user the port runs as |

`gh` exits non-zero for any non-2xx answer, a `304` included, while still printing the head: the
status line decides, never the exit code.

## Parsing

The PR resource family is parsed in one place, shared by the inbox and `RealGithubPrApi`:

- `pr_json` — `parse_pr_detail` / `parse_pr_body` (`GET pulls/{n}`), `parse_first_pr`
  (`GET pulls?head=`), `parse_reviews`, `parse_review_comments`, `parse_issue_comments`;
- `check_runs::parse_check_runs` — `GET commits/{sha}/check-runs` into `CheckRun { id, name, status,
  conclusion, started_at, completed_at, details_url }`. Any status short of `completed` is unfinished
  (`CheckStatus::Queued` unless it is `in_progress`); a `null` conclusion reads as empty, never as a
  failure.

Tests: `packages/tddy-github/tests/conditional_read_over_http.rs` (a local axum GitHub and a scripted
`gh`).
