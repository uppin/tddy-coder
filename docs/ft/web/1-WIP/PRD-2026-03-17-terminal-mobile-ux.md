# PRD: Terminal Mobile UX — Keyboard, Resize, and Touch

## Summary

Fix three mobile UX issues in the web terminal on Android Chrome:

1. Terminal does not resize when virtual keyboard opens/closes — content remains hidden behind keyboard.
2. Keyboard auto-opens on connect (via `term.focus()`), which is disorienting on mobile — replace with a manual "open keyboard" button.
3. Touch tap events don't register for TUI interaction (e.g., menu selection).

## Background

The web terminal (`GhosttyTerminal` + `ConnectedTerminal`) was built for desktop. On mobile:

- `position: fixed; inset: 0` fills the full viewport, but the virtual keyboard overlaps the bottom portion without triggering a layout resize.
- `term.focus()` on ready immediately opens the keyboard, which is unexpected on mobile.
- Only `mousedown`/`mouseup`/`wheel` listeners exist — no `touchstart`/`touchend` handling for SGR mouse sequences.

## Affected Features

- [web-terminal.md](../web-terminal.md) — Connected Terminal UX section (fullscreen, auto-focus, touch/mouse mode)

## Proposed Changes

### 1. Terminal Resize on Keyboard Open/Close

Use the **Visual Viewport API** (`window.visualViewport`) to detect keyboard presence. When the visual viewport height shrinks (keyboard opens), resize the terminal container to `visualViewport.height`. When it returns to full height, restore fullscreen.

**What changes**: `ConnectedTerminal` container in `index.tsx` and/or a new hook.
**What stays**: FitAddon auto-sizing within the container (it reacts to container size changes).

### 2. Manual Keyboard Button (Bottom Center)

- Remove auto-focus on mobile (detect via `'ontouchstart' in window` or viewport width).
- Show a floating "Keyboard" button at the bottom center of the screen.
- Tapping it focuses the terminal (opens keyboard) and hides the button.
- When the keyboard closes (detected via Visual Viewport resize back), show the button again.

**What changes**: `ConnectedTerminal` layout, `GhosttyTerminalLiveKit` `onReady` behavior.
**What stays**: Desktop behavior (auto-focus, no button).

### 3. Touch Event Forwarding

Add `touchstart` and `touchend` listeners alongside existing mouse listeners in `GhosttyTerminal.tsx`. Convert touch coordinates to cell coordinates and send SGR mouse sequences, same as mouse events. This enables tap-to-click for TUI menus on mobile.

**What changes**: Mouse/touch forwarding effect in `GhosttyTerminal.tsx`.
**What stays**: Existing mouse event handling (desktop unaffected).

## Acceptance Criteria

1. On Android Chrome, opening/closing the virtual keyboard causes the terminal to resize to fit the visible area above the keyboard.
2. On mobile, a floating "Keyboard" button appears at the bottom center. Tapping it opens the keyboard and hides the button.
3. When the keyboard closes, the button reappears and the terminal fills the screen.
4. Tapping on TUI elements (menus, buttons) sends the correct SGR mouse sequences.
5. Desktop behavior is unchanged: auto-focus, no keyboard button, mouse events work as before.
