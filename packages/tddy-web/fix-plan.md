# Fix: Web plan preview / approval (embedded terminal)

## Symptom

In **tddy-web**, when the session TUI shows the plan in **Markdown viewer** mode (Approve/Reject in the activity area, shortcuts in the prompt bar), users report:

1. **Mouse**: Approve / Reject clicks do nothing.
2. **Enter**: Pressing Enter on an “empty” prompt should approve (expected UX).
3. **Esc**: Should close the plan viewer and return to the user-question / activity flow (`DismissViewer`).
4. **Alt+A / Alt+R**: Shown in the prompt bar but do not work when using the embedded terminal (including when the outer environment is **Ghostty**).

The live session is still **crossterm/ratatui in the daemon PTY**; the web UI is **ghostty-web** + LiveKit forwarding bytes. There is no separate HTML Approve button—clicks are **SGR mouse sequences**, and keys go through the terminal’s **keydown → onData** path.

## Investigation

### 1) Mouse clicks (tddy-web) — **fixed in this changeset**

`GhosttyTerminal` forwards SGR mouse (`\x1b[<…`) only when `term.hasMouseTracking?.()` is true (TUI must enable mouse reporting).

Cell coordinates for that forwarding were computed using the **outer container** width/height while pointer math mixed **canvas-local** offsets with that box. The ghostty-web **canvas** is sized to `cols × cell metrics`; the wrapper `div` can differ. That skewed **column/row** indices so the TUI’s `plan_approval_activity_footer_click` / `mouse_map` rarely matched **Approve/Reject**.

**Fix**: derive the grid from the **`canvas` `getBoundingClientRect()`** and map **`clientX` / `clientY`** with a small pure helper (`clientPointToTerminalCell` in `src/lib/terminalMouseCellCoords.ts`). Unit tests lock the geometry.

### 2) Enter on “empty prompt” (tddy-tui)

In `packages/tddy-tui/src/key_map.rs`, `markdown_viewer_key`:

- **Enter → Approve/Reject** only when `view_state.markdown_at_end` is true (user has scrolled to the end). If the prompt still says “read to the end for Approve / Reject”, Enter intentionally does nothing.
- **Refinement pending**: Enter submits non-empty refinement text; **empty Enter does not** map to approve.

If product intent is “empty Enter means approve” for a specific prompt state, that is a **TUI behavior change** (and tests in `key_map` / presenter), not a web-only fix.

### 3) Esc

TUI maps `Esc` → `UserIntent::DismissViewer` in markdown viewer (`key_map.rs`). The web stack should send **`\x1b`** for Escape; `ghostty-web`’s `InputHandler` does so for the focused terminal textarea. If Esc still fails, check **focus** (another element focused) or **IME** (there is already an IME Escape workaround in `GhosttyTerminal`).

### 4) Alt+A / Alt+R (and “Ghostty”)

Shortcuts are handled in `plan_view_approve_reject_shortcuts` (requires **Alt** + `a`/`r`). The embedded terminal relies on **ghostty-web**’s key encoder for modified keys; browsers and OS-level terminal settings may **eat or remap Alt** before it reaches the page. **Ghostty** (the terminal emulator) may use Alt for its own UI when the “browser” runs inside it—**verify focus is on the page/canvas**, not the terminal chrome. Remedies often include: alternate bindings without Alt (TUI), or documenting OS/terminal settings.

## Tests added

- `src/lib/terminalMouseCellCoords.test.ts` — **bun** unit tests for `clientPointToTerminalCell`.

## Verification

```bash
cd packages/tddy-web && bun test src/lib/terminalMouseCellCoords.test.ts
```

Full web unit suite (per `package.json`):

```bash
bun run test:unit
```

## Follow-up (optional)

- Cypress component test with a stub `Terminal` that forces `hasMouseTracking() === true` and asserts `onData` receives SGR for a synthetic click at a known cell (heavier setup).
- Product decision + TDD change if **empty Enter** should approve in a defined markdown-viewer state.
- If Alt shortcuts remain unreliable in the field, add **non-Alt** shortcuts in the TUI (parallel bindings) after UX sign-off.
