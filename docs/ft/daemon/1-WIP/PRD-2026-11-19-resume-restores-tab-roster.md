# Resume restores the whole tab roster - PRD

**Date**: 2026-11-19
**Status**: Draft
**Stack**: `#session-liveness` 4/4 — `feature/session-liveness/resume-roster`
**Changeset**: [2026-11-19-resume-restores-tab-roster.md](../../../dev/1-WIP/2026-11-19-resume-restores-tab-roster.md)

## Affected Features

- [Inactive session activities § Resume](../../web/inactive-session-activities.md#resume)
- [Session terminal tabs § Stopped sessions](../../web/session-terminal-tabs.md#stopped-sessions)
- [Claude CLI session](../claude-cli-session.md) — resume of main

## Summary

`ResumeSession` brings back **every tab** in the session's roster (`#session-liveness` 2/4), not only
main, each under its **recorded terminal id**:

- an agent tab resumes its own conversation;
- a tmux-wrapped tab whose tmux session is alive is re-attached, and one whose tmux session is gone
  is relaunched inside a new one;
- a shell reopens fresh;
- a `tmux-attach` tab re-attaches if the user's tmux session still exists.

## Background

Resume restores only main today, so an operator who worked with a second Claude tab or a tmux agent
tab loses it at every stop. The developer asked for a session to come back "to its state with all
its tabs … including resume from a session on a secondary tab". The roster that 2/4 records is what
makes that possible.

## Proposed Changes

### What's Changing

- **Restore order.** Main first (unchanged path), then the other roster entries in roster order, each
  started under its recorded `terminal_id` (the manager accepts a caller-chosen id; none is minted).
  An entry whose terminal is already live is left alone.
- **Agent tabs resume their own conversation**: the tab's `conversation_id` is rendered with
  `resume` set, so claude gets `--resume <id>` and cursor gets `--resume <chat id>`. A cursor tab is
  started with a **real minted chat id** (`cursor-create-chat`) rather than a random uuid, so it can
  be resumed at all.
- **Tabs that cannot resume a given conversation** (opencode and muse, whose `--continue` would pick
  the worktree's last conversation, ambiguous across tabs; warp, which needs an exit-hook token tabs
  do not capture) reopen **fresh** under their id. Their roster entry is marked
  `resumable: false` on its `agent` node, and the tab says so. A TODO per harness records the gap.
- **Failures are per tab.** A tab that cannot be restored (its harness is gone, its user's tmux
  session is gone) is dropped from the roster and named in `ResumeSessionResponse` (a new
  `repeated RestoredTab` with `terminal_id` and `outcome: RESTORED | FRESH | DROPPED, reason`). Main
  failing still fails the resume, as today.
- **Web.** After Resume the strip lists the restored tabs with their ids, so the tab that was active
  before the stop stays selected. A tab that came back fresh shows a one-line notice; a dropped tab
  is reported in the Resume toast.

### What's Staying the Same

- Main's resume path: conversation choice, tmux re-attach versus relaunch, sandboxed sessions.
- Tool sessions (3/4) resume as today; they have no secondary tabs.

## Acceptance Criteria

1. Resuming a session whose roster holds main, a second claude tab and a shell restores all three
   under their recorded terminal ids; the claude tab is launched with `--resume <its conversation id>`.
2. A tmux-wrapped agent tab whose tmux session is alive is re-attached on Resume without relaunching
   its agent; one whose tmux session is gone is relaunched inside a new one and resumes its
   conversation.
3. A tmux-attach tab re-attaches when the user's tmux session exists; when it does not, it is
   dropped and named in the response.
4. An opencode or muse tab reopens fresh, is marked not resumable, and is reported `FRESH`.
5. A cursor agent tab is started with a minted chat id, and resumes it.
6. A tab already live at Resume is not restarted.
7. After Resume the web strip shows the restored tabs, and the tab that was active stays selected.
