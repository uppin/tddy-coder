# Project-home auto-stop cannot stop an untracked session process

**Raised:** 2026-10-23 by `/pr-wrap` on #701 (`#project-home` 6/7)
**Why deferred:** the reaper stops through `CliSessionManager::stop_session`, which only knows terminals this daemon process started. An idle project-home session whose pid is alive but untracked (started by a previous daemon process, or re-parented) is skipped: the stop is a no-op, the reaper waits 5 s, logs `did not exit in time`, and repeats every 30 s tick. Signalling a bare pid from metadata was not done because pids are reused and the daemon cannot prove the pid is still that agent.
**What to do:** give the reaper a pid-verified stop for untracked sessions (match the pid's start time or command line against the session before signalling), or skip untracked sessions up front without the 5 s wait. Test: an acceptance case with a session whose process is alive but absent from the manager.
