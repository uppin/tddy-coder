# A stopped session's Agent tab shows its greyed history and offers Resume - PRD

**Date**: 2026-11-02
**PRD Type**: Feature Enhancement
**Stack**: `#session-tabs 6/7` (branch `feature/session-tabs/dormant-agent-tab`)

## Affected Features

- **Primary**: [Session terminal tabs](../session-terminal-tabs.md) — the Agent tab of a stopped
  session
- **Related**: [Inactive session activities](../inactive-session-activities.md) — Resume from the
  Agent tab, without the ~2s poll lag
- **Related**: [Terminal replay lazy scroll](../terminal-replay-lazy-scroll.md) — the greyed history
  scheme is reused

## Summary

In a stopped session, the **Agent** tab shows what the agent terminal last printed, rendered read-only
in the terminal's greyed **history** scheme (the same scheme as "View history"). A **toast** over it
says the session is not running and offers **Resume**. Resuming turns the Agent tab live in place:

- the greyed view is replaced by the live agent terminal;
- every other tab (shells, harness and tmux tabs, agent conversations) stays open and keeps its state;
- the switch happens as soon as `ResumeSession` succeeds, not at the next 2s session-list poll.

The recorded transcript (activities) is still reachable from the session's activities view; it is no
longer the Agent tab's body.

## Background

`#session-tabs 1/7` gives a stopped session its tab strip, with the transcript as a stand-in in the
Agent tab. The developer wants the Agent tab to look like the terminal it was: the agent's last
screen, greyed to say it is not live. `#session-tabs 2/7` makes `GetTerminalHistory` serve that output
after the process exited. There is no toast component in `tddy-web`; this node adds one.

## Proposed Changes

### What's Changing

- **Agent tab, stopped session:** a read-only terminal filled from `GetTerminalHistory` for `main`,
  in the history colour scheme (`HISTORY_THEME`, greyscale filter), and scrollable.
  - Keystrokes are not sent anywhere.
  - With no recorded history, the tab says so ("No recorded output") under the toast.
- **Toast:** a reusable `Toast` UI component (in-repo, no new dependency) with a message, one action
  and an accessible `role="status"`.
  - On the Agent tab it reads "This session is not running" with a **Resume** action.
  - It shows a spinner while resuming and an inline error if Resume fails.
- **Resume in place:** on a successful `ResumeSession`, the session is treated as live at once (an
  optimistic liveness hint until the next listing agrees). The Agent tab mounts the live terminal and
  the other tabs are untouched.
- The top bar's Resume button behaves the same (shared action).

### What's Staying the Same

- The `ResumeSession` RPC.
- A live session's Agent tab.
- The activities transcript component.

## Impact Analysis

### Technical Impact

- `tddy-web`: a `Toast` component; a dormant Agent-tab body built on the existing history fetcher and
  history scheme (new modules: `GhosttyTerminalSession.tsx` is over budget); an optimistic liveness
  hint in the resume path.

### User Impact

- A stopped session reads like a paused terminal, and Resume is one click on it.

## Acceptance Criteria

- [ ] A stopped session's Agent tab shows the output returned by `GetTerminalHistory(main)`, in the
  history colour scheme.
- [ ] Typing into it sends no input RPC.
- [ ] A toast "This session is not running" with a Resume action is shown over it.
- [ ] Clicking Resume calls `ResumeSession` once and the Agent tab shows the live terminal without
  waiting for a session-list refresh.
- [ ] A bash tab opened before Resume is still open, and still the same terminal, after it.
- [ ] A failed Resume shows its error in the toast and keeps the greyed view.
- [ ] With no recorded history, the tab shows "No recorded output" and the toast.

## Out of Scope

- Recording the history (`#session-tabs 2/7`).
- A global toast queue or notification centre.
