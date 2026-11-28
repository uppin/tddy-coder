# Daemon PR-status refresh: ~2,200 lookups/h, currently 0 GitHub requests

**Status:** open, not the cause of the 403.

## Evidence
`~/.tddy/logs/daemon` (desktop-hosted daemon, from 22:59 to 09:20 the next day) contains 19,000 lines of
`[tddy_daemon_rpc::work_queue::pr_status] PR status unavailable for branch …: no credential vault is
available to this session start`. The top branch is feature/cross-host-watch with 4,434.
Per clock hour: 01h 2216 · 02h 2194 · 03h 2178 · 04h 2222 · 05h 2139 · 06h 1927, while nobody was at the
machine. In the same file: 77,183 `QueryBranch` lines and 29,078 `the blocking lane is full — refusing instead of queueing`.

So the dashboard's `QueryBranch` refresh asks for PR status about 37 times a minute. It returns from
`pr_status_for_caller` (`packages/tddy-daemon-rpc/src/work_queue/pr_status.rs:48-87`) **without a
GitHub call**, because the site cannot be resolved. Rotated logs show 0 successful PR-status reads.

## Once a vault is configured
The lookups go through `pr_status_view_through_inbox` (`pr_status.rs:77`), which uses the per-repo inbox:
ETag revalidation, 304s are free, concurrent reads are coalesced
(`packages/tddy-github-inbox/src/lib.rs:1-14`), and a feedless row is held for 30 s
(`packages/tddy-github-inbox/src/repo.rs:16`). With feed polling at 60 s, `X-Poll-Interval` honoured,
pending checks at 30 s and a sweep every 15 min (`pacing.rs:8-12`), the worst case is roughly one
conditional GET per row per 30 s. That is about 120/h per active branch row, and 304s do not count.
**Not duplicated per session**: there is one inbox per repository.

## Concern
The refresh rate (and the blocking-lane saturation) is a cost of its own, and it should be confirmed
under a real vault before assuming it stays quota-free.
