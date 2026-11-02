# A terminal's OSC 52 copy reaches the viewer's clipboard - PRD

**Date**: 2026-11-02
**PRD Type**: Feature Enhancement
**Stack**: `#session-tabs 4/7` (branch `feature/session-tabs/osc52-clipboard`)

## Affected Features

- **Primary**: [Web terminal](../web-terminal.md) — every terminal honours OSC 52 clipboard writes
- **Related**: [Session terminal tabs](../session-terminal-tabs.md) — tmux copy-mode in a tmux tab
  copies to the viewer's machine

## Summary

When a program in any session terminal (agent, bash, harness or tmux tab) emits an **OSC 52 clipboard
write** (`ESC ] 52 ; <selection> ; <base64> BEL|ST`), the web terminal decodes it and writes the text
to the clipboard of the machine the viewer is on: the browser's or Tddy Desktop's. The user can then
paste it outside the session. tmux copy-mode, Neovim with `clipboard=unnamedplus` over OSC 52 and
Claude Code's copy all use this mechanism.

**OSC 52 reads** (`?` payload) are refused and never answered, so a program cannot read the viewer's
clipboard.

## Background

Copying inside tmux today goes to tmux's own paste buffer, which lives on the host and is unreachable
from the viewer's machine. The terminal emulator is the standard place to bridge it, and OSC 52 is the
standard protocol for that. The web terminal (ghostty-web) currently has paste only.

## Proposed Changes

### What's Changing

- An OSC 52 recognizer runs over each terminal's output stream.
  - It handles a sequence split across chunks, both terminators (BEL and ST), and an empty or `c`
    selection parameter.
  - It decodes base64 to UTF-8 and writes it with the app's `copyToClipboard`.
  - It caps the decoded payload (1 MiB) and drops larger ones.
- Reads (`?`) and malformed payloads are ignored.
- The sequence still reaches the terminal emulator unchanged; recognition is a tap, not a filter.
- **Tddy Desktop:** clipboard writes work in the Tauri webview. If the webview refuses
  `navigator.clipboard` there, the desktop gets the clipboard capability it needs; that is a new
  dependency and is decided with the developer first.

### What's Staying the Same

- Paste and drag-and-drop into the terminal.
- Mouse selection in the terminal.

## Impact Analysis

### Technical Impact

- `tddy-web`: a new OSC 52 module (not `GhosttyTerminal.tsx`, which is 1362 lines against a 500
  budget), tapped where terminal output is written.
- Possibly `tddy-desktop` (see above).

### User Impact

- Copy inside tmux or Neovim in a session tab pastes on the viewer's machine.
- A program in a shared session can write the viewer's clipboard, which is the standard OSC 52 trust
  model of terminal emulators. Reads are never allowed.

## Acceptance Criteria

- [ ] A terminal that receives `ESC]52;c;<base64("hello")>BEL` writes `hello` to the clipboard.
- [ ] The same with the ST terminator and with an empty selection parameter.
- [ ] A sequence split across two output chunks is recognized once.
- [ ] Multibyte UTF-8 text round-trips.
- [ ] `ESC]52;c;?BEL` writes nothing and sends nothing back.
- [ ] A payload over the cap writes nothing.
- [ ] The terminal still renders the output around the sequence unchanged.
- [ ] It works on an agent tab and on a bash tab (both feed paths).

## Out of Scope

- OSC 52 reads.
- Selecting a clipboard other than the system one (primary selection).
- Desktop-native copy shortcuts.
