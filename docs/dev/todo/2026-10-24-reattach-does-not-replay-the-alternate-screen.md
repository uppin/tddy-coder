# 2026-10-24 — A reattach is not told the program is on the alternate screen

> **Partly resolved by `#term-snapshot 1/4`:** a `SNAPSHOT` open (every web open) carries the alternate
> screen — the snapshot is serialized from the screen model and switches to it. The legacy
> `TAIL` / `FROM_OFFSET` replays (non-web clients, `StreamTerminalOutput`) still do not: their
> `mode_prologue` is pinned to the mouse modes by `terminal_capture_replay`'s
> `ignores_private_modes_outside_the_mouse_tracking_family`. What remains is below, narrowed to them.

**Category:** Deferred from "No terminal history offer while a full-screen TUI owns navigation" (#714)
**Source:** `packages/tddy-task/src/terminal_capture.rs` (`mode_prologue`, `MOUSE_TRACKING_MODES`)

## What happens

On every attach the daemon replays the mouse-tracking modes the program enabled
(`TerminalCapture::mode_prologue`), but not the alternate screen (DEC `1049` / `1047` / `47`). A
browser that attaches to a program which is on the alternate screen **without** mouse tracking
(`less`, `man`) therefore believes it is on the normal screen. It shows the terminal's
**Load earlier output** / **View history** offer until that program next switches modes. Its first
repaint also lands on the browser's normal screen rather than the alternate one.

## Why deferred

#714 is a tddy-web change. The case it targets, a fullscreen Claude, enables mouse tracking, and the
replayed mouse modes already cover a reattach for it. Pagers reattached mid-page are rare.

## What would close it

Track the alternate-screen modes alongside the mouse modes in `TerminalCapture`'s sniffer and
replay the one that is set in `mode_prologue`. That is a Rust change in `tddy-task`, with a
`terminal_mode_replay_acceptance.rs` case for a program that entered `1049` before the attach.
