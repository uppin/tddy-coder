# 2026-11-14 — A Muse Code session starts on a trust prompt, then a long blank screen

**Category:** Future enhancement
**Source:** project-home orchestrator driving `muse-cli` session `01a1d89b-0e66-7880-bed2-a6bec7ba7238`
on 2026-11-14

A new Muse session first shows Muse's workspace-trust prompt (the daemon-cut worktree is new to
Muse). After it was accepted, the screen stayed blank for about 30s before Muse rendered.

## Where the time went

Muse's own startup timeline (session log seq 13, and its CLI log
`…/01a1d89b-27ee-7fd0-9a7e-c1723ee1a1b5/cli-*.log`), in ms from process start (about 0.7s after tddy
created the session):

| phase | start | duration |
|---|---|---|
| terminal_probe | 102 | 101 |
| *(trust prompt waiting for its answer)* | ~200 | ~12 100 |
| session_open | 12 311 | 95 |
| runtime_init | 12 410 | 6 298 |
| *(credentials, provider transport, approval judge, session create)* | | ~7 100 |
| first_frame | 25 879 | 18 |

From the trust answer to Muse's first frame is **~13.6s, all inside Muse**. Nothing in tddy's path
adds time: the PTY is relayed live, and `terminals/main.capture` is a periodic disk recording, not the
read path. The rest of the observed ~30s is in no log. Most likely it is the orchestrator's own
screen-read cadence; its tool-call timestamps would settle it.

## What remains

- Decide whether a daemon-cut worktree should start with `--trust-workspace` (this run only, not
  saved). It removes the prompt and its round trip, but loads the worktree's project skills and rules
  without asking. Project `.muse/hooks.json` may need trust too, which ties into
  [2026-11-14-muse-hooks-not-wired.md](2026-11-14-muse-hooks-not-wired.md). A spec decision for the
  developer.
- The ~13.6s runtime init is Muse's to fix, not tddy's.

**Why deferred:** low priority; the trust choice is a security decision, not a bug fix.
