# "+" sits after the last tab - PRD

**Date**: 2026-11-02
**PRD Type**: Feature Enhancement
**Stack**: `#session-tabs 5/7` (branch `feature/session-tabs/tab-plus`)

## Affected Features

- **Primary**: [Session terminal tabs](../session-terminal-tabs.md) — the "+" control's position

## Summary

The session tab strip's **"+"** control moves from the strip's right edge to directly after the last
tab, as in browsers and editors. When the tabs overflow and the strip scrolls, "+" sticks to the
scroller's right edge, so it is never scrolled out of reach. Its menu still opens fully visible,
not clipped by the scroller.

## Background

With a few tabs, "+" sits far from them at the opposite edge of a wide strip, so the eye has to travel
to find it. It was placed outside the scroller to keep it reachable with many tabs. Sticky positioning
at the end of the scroller keeps that guarantee while placing it next to the tabs.

## Proposed Changes

### What's Changing

- "+" is the last child of the scrolling tab list, right after the last tab of any kind: terminal,
  child session or agent conversation.
- It is `position: sticky` at the scroller's right edge with an opaque background, so overflowing tabs
  scroll under it.
- Its menu renders outside the scroller (a portal positioned at the button), so `overflow-x: auto`
  does not clip it.

### What's Staying the Same

- The menu's entries and behaviour (Escape, outside click).
- The full-screen toggle stays pinned at the strip's right edge.

## Impact Analysis

### Technical Impact

- `tddy-web`: `SessionTerminalTabs` layout and `NewTerminalMenu`'s menu placement.

### User Impact

- "+" is where the user looks after the last tab.

## Acceptance Criteria

- [ ] "+" is rendered inside the tab list, immediately after the last tab, for each last-tab kind
  (Agent only, a terminal, a child session, an agent conversation).
- [ ] With more tabs than fit, "+" stays visible at the scroller's right edge while the tabs scroll.
- [ ] The "+" menu opens fully visible when the tab list is overflowing.
- [ ] Escape and an outside click still close the menu.

## Out of Scope

- Reordering tabs, tab overflow menus.
