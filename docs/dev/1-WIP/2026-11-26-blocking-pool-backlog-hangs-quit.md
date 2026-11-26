# Changeset: a QueryBranch backlog cannot starve ListSessions or hang Desktop quit

**Date**: 2026-11-26
**Status**: 🟢 Green
**Type**: Bug fix
**PR**: #1059

## Summary

On 2026-11-26 Tddy Desktop's session list went empty and never recovered, and Cmd+Q then left the
app "not responding", using CPU, for about 7 minutes until it was force-killed.

- The work-queue view polls `QueryBranch` once per row every few seconds (~12k calls in 4 h). Each
  call put its git subprocesses (worktree, remote ref, base comparison) and a session scan straight
  onto the blocking pool via `spawn_blocking_with_timeout`. When git slowed down, a call timed out
  after 300 s but **left its job running**, so the pool filled with jobs nobody waited for. Every
  other RPC that needs the pool — `ListSessions` among them — timed out behind them (~7,900
  timeouts), and the list read as empty.
- Quit runs the daemon's shutdown on the main (UI) thread. `CliSessionManager::kill_all` waited
  for its child processes on the blocking pool, so it queued behind the backlog, and nothing
  bounded the wait.

## Scope

- **`tddy-daemon-rpc`** — `work_queue::blocking_lane`: `BlockingLane<T>` coalesces requests for a
  key already in flight onto one job, and bounds how many run at once (permits held until the job
  itself finishes, not until its caller gives up). A full lane refuses a new key at once with
  `Unavailable` instead of queueing. `QueryBranchLanes` gives each of `QueryBranch`'s four blocking
  legs a lane, sharing 4 permits. The worktree, remote and base-sync legs already degrade on
  error; a refused session scan fails that one poll, and the next poll retries.
- **`tddy-cli-sessions`** — `kill_all` waits for cancelled processes on the async timer
  (`reap_or_kill`) instead of the blocking pool.
- **`tddy-desktop`** — `bounded_shutdown::shut_down_within`: a quit waits at most `QUIT_DEADLINE`
  (10 s) for the daemon's children, then exits anyway and logs it. tmux-run agents outlive the
  daemon by design, and the next start re-attaches them.

## Acceptance tests

| Test | Package |
|---|---|
| `concurrent_requests_for_one_key_share_a_single_blocking_job` | tddy-daemon-rpc |
| `a_saturated_lane_refuses_a_new_key_at_once` | tddy-daemon-rpc |
| `a_permit_comes_back_when_its_job_finishes` | tddy-daemon-rpc |
| `kill_all_finishes_within_its_deadline_while_the_blocking_pool_is_saturated` | tddy-cli-sessions |
| `a_shutdown_that_never_finishes_is_given_up_at_its_deadline` | tddy-desktop |
| `a_shutdown_that_finishes_in_time_reports_finished` | tddy-desktop |

## TODO

- [x] Lane for `QueryBranch` legs
- [x] `kill_all` off the blocking pool
- [x] Bounded quit
