# Changeset: a terminal reports ready only once its initial content is written

**Date**: 2026-11-03
**Status**: 📋 Planned
**Type**: Bug fix
**Complexity**: 2/5 — one component, one package (`tddy-web`), no API change
**Stack**: `#tmux-agent` 4/4 (sweep) — base `feature/tmux-agent/harnesses-in-tmux` (#816)

## Affected Packages

- `tddy-web` — `src/components/GhosttyTerminal.tsx`, `cypress/component/GhosttyTerminal.cy.tsx`

## Problem (State A)

`GhosttyTerminal` opens its emulator, then calls `setReady(true)` and `onReady()` together. It does
**not** write `initialContent` at that point. A separate `[initialContent, ready]` effect writes it
on the render after `ready` flips. So for one render, two things are true while the buffer is still
empty:

- `onReady` has fired;
- `data-terminal-ready="true"` is set.

Who sees the empty buffer:

- **`DormantAgentTerminal`** — its `onReady` sets `opened`. The history fetch and "No recorded
  output" logic run against a terminal that has not yet shown what it was given.
- **Cypress specs** that act right after ready or after the canvas appears. A drag, a tap or a
  buffer read can land before the content does. #815 (`TerminalTouchScrollRoutingAcceptance`) and
  #816 (`GhosttyTerminal.cy` drag-to-scroll) each failed on CI this way. Each got a test-only wait
  (`expectBufferHoldsTimes("row-200", 1)`). Four other specs pass `initialContent` and carry the
  same latent race: `GhosttyTerminalDpr`, `GhosttyTerminalIsolation`, `GhosttyTerminalOsc52Acceptance`
  and `TerminalTapMouseClickAcceptance`.

The race was exposed by the #session-tabs sweep (#813). That PR moved the write out of the open path
so it would not render twice or copy OSC 52 twice.

## Target (State B)

`initialContent` that is present when the emulator opens is written in the open path, **before**
`setReady(true)` and `onReady()`. Ready therefore means "showing what it was given".

- The content is written **once**. It must not be written a second time, by the open path and then
  the effect.
- OSC 52 is copied **once**.
- A later change to `initialContent` is still written by the effect. This includes content that
  arrives after the emulator opens.

The test-only waits added in #815 and #816 stay. They are harmless, and they now document the
contract.

## Responsibility

Owns the ordering inside `GhosttyTerminal` between writing `initialContent` and signalling ready
(`setReady`, `onReady`, `data-terminal-ready`).

## Boundaries

- No change to `GhosttyTerminalHandle`, to props, or to any consumer.
- No change to the other specs. Their waits are not removed.
- Out of scope, and deferred as code issues per the developer's sweep rule: the shared tmux test
  fixtures (three `TODO(tmux-agent)` markers), `service.rs` at 573 lines, `CreateSessionPane.tsx`
  at 524 lines, and the length of `spawn_tool` and `spawn_claude_cli_session_inner`.
- Deferred TODO, not touched here:
  [detached tmux agent output is not recorded](../todo/2026-11-03-detached-tmux-agent-output-is-not-recorded.md).
  The developer chose a follow-up PR after the stack lands.

## Dependencies

- #816 (`#tmux-agent 3/3`): base branch only. Nothing from it is consumed.
- #813 (merged): introduced the effect-only write this node reorders.

## Draft PR contract

The contract is one failing component test in `GhosttyTerminal.cy.tsx`:

- **"holds its initial content by the time it reports ready"** — `onReady` reads the handle's
  `getBufferText()` and finds the marker.

The existing test "renders its initial content once" keeps guarding against a double write.

## Green wave

Wave 1 of the sweep. Its dependency is #816 (base only, already delivered).
