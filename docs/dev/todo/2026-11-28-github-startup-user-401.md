# Session start calls GET /user and gets 401 for account `uppin`

**Status:** open, low volume (15 calls in the current daemon log), but the failure is real.

`~/.tddy/logs/daemon`: `[tddy_daemon::connection_service] session … starts without an account identity:
the github account uppin has no subject_id recorded, and asking github who its token belongs to failed
(GitHub API GET https://api.github.com/user returned HTTP 401`. This happens on every session start
(15 times, e.g. 09:19:56 for 01a22106). The stored token is rejected, and because the `subject_id` is
never saved, each start tries again. This is not a quota problem, but it is why the daemon has no identity
or vault (see the PR-status finding).
