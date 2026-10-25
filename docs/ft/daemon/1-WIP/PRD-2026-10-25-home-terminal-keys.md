# Project-home agent types into a session's terminal - PRD

**Date**: 2026-10-25
**PRD Type**: Enhancement
**Stack**: `#home-inbox 5/8` (branch `feature/home-inbox/terminal-keys`)

## Affected Features

- **Primary**: [Project concept § Project-home tools](../project-concept.md) - `ClaimSessionTerminal` and `SendSessionKeys`
- **Related**: [Terminal sessions § Terminal Control Ownership](../terminal-sessions.md) - the home agent is a screen that can hold the control lease

## Summary

The project-home agent takes control of a child's terminal and types into it:

```
ClaimSessionTerminal(session_id)                     → { holder: "project-home:<home id>" }
SendSessionKeys(session_id, text?, keys?: ["Enter", "Down", "C-c", …])
```

`SendSessionKeys` works only while the home agent **holds the control lease** on that session.

- A person who takes control back in the browser, with the existing steal, makes every later send
  fail until the agent claims again.
- A claim always takes control. The agent is told in the tool description to claim only when it
  means to act, and never to fight a person for control.

## Background

`#home-inbox 2/8` lets the home agent read a child's screen, but it still can't act on it: answer a
dialog, press Escape, interrupt a runaway command. The terminal control lease is the existing
mechanism that decides who may type into a session. Leases are never released today, so the rule
becomes "type only while you hold it" rather than "type unless someone else holds it".

## Proposed Changes

### What's Changing

- **`ClaimSessionTerminal(session_id)`** claims the session's lease with `steal = true` for the
  screen id `project-home:<home session id>`.
  - The daemon keeps the issued control token on the home caller's behalf; the agent never sees it.
  - The lease-change event is broadcast as for any steal, so an attached browser drops to watch-only
    and shows who took control.
- **`SendSessionKeys(session_id, text?, keys?)`.**
  - It refuses with "control is held by `<screen>`; claim it again" unless the home agent's token is
    the session's current lease.
  - Text is sent first, as a bracketed paste when the terminal has enabled it. Named keys follow in
    order.
  - **Key names:** `Enter`, `Escape`, `Tab`, `BTab`, `Backspace`, `Delete`, `Up`, `Down`, `Left`,
    `Right`, `Home`, `End`, `PageUp`, `PageDown`, `Space`, `C-a` … `C-z`, `F1` … `F12`. An unknown
    key name is refused before anything is sent.
- **Both terminal kinds.** The rule applies to direct and jailed terminals alike. A jailed terminal's
  lease is checked explicitly, because its stream path skips the check.
- **Logging.** Every send is recorded in the home session's tool-call log, keys included.

### What's Staying the Same

- A person can always take control back from the browser.
- Reading (`ReadSessionTerminal`) needs no lease.
- Lease semantics: no auto-release and no timeouts.

## Impact Analysis

- **Technical.**
  - A control port and a terminal-input path reachable from the exec-tool host.
  - A per-home token store.
  - A key encoder.
- **User.** The home agent can unstick a child. A person watching sees exactly when it took control.

## Acceptance Criteria

- [ ] After `ClaimSessionTerminal`, `SendSessionKeys` with text and `Enter` writes exactly the
  encoded bytes to the child's PTY, and the child's screen reflects them through
  `ReadSessionTerminal`.
- [ ] `SendSessionKeys` without a prior claim is refused and writes nothing.
- [ ] When a browser steals control after the claim, the next `SendSessionKeys` is refused and
  names the holder. After a new claim it succeeds again.
- [ ] An unknown key name is refused and writes nothing.
- [ ] The claim broadcasts a control change that names `project-home:<home id>`.
- [ ] A jailed child's terminal follows the same rules.

## Out of scope

- Mouse events.
- Resizing.
- A release tool (the person reclaims).
- Typing into the home session itself.

## Changes from the 2026-10-25 plan

Replan of 2026-10-26: `master` moved, and this stack now sits on `#start-parity 9/9` (#758).

- `ClaimSessionTerminal` and `SendSessionKeys` take an optional `daemon_instance_id`. A session on
  another host is claimed and typed into on that host, through `#start-parity 1/9`'s forward. The lease
  is held there, under the calling home session's identity.
