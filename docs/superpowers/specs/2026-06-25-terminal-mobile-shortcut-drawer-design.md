# Terminal Mobile Shortcut Drawer — Design Spec

**Date:** 2026-06-25  
**Status:** Approved

## Overview

Add a floating, draggable shortcut-preset panel to the terminal view in the tddy web UI. On mobile, users cannot type key combinations like Shift+Tab directly. The drawer presents per-tool shortcut buttons (e.g. "Shift+Tab", "Ctrl+C") that send the correct ANSI escape sequences to the terminal. Shortcut presets are configured per-tool in a static frontend map, so Claude CLI sessions get a different set than tddy-coder sessions.

## Scope

- Frontend-only change (no proto or Rust modifications required)
- Applies to fullscreen terminal layout only (not the compact overlay pane)
- Visible only when `showMobileKeyboard` is true (i.e. `isMobile`)

## Data Model

### `ToolShortcutDef`

```ts
interface ToolShortcutDef {
  label: string;   // button label, e.g. "Shift+Tab"
  keys: string[];  // symbolic key list, e.g. ["Shift", "Tab"]
}
```

### `keySequenceToBytes(keys: string[]): Uint8Array`

Pure function mapping symbolic keys to ANSI/VT escape sequences. Supported symbolic keys:

| Symbol | Sequence |
|--------|----------|
| `Tab` | `0x09` |
| `Shift`+`Tab` | `\x1b[Z` |
| `Escape` | `0x1b` |
| `Enter` | `0x0d` |
| `Backspace` | `0x7f` |
| `Delete` | `\x1b[3~` |
| `ArrowUp` | `\x1b[A` |
| `ArrowDown` | `\x1b[B` |
| `ArrowRight` | `\x1b[C` |
| `ArrowLeft` | `\x1b[D` |
| `Home` | `\x1b[H` |
| `End` | `\x1b[F` |
| `PageUp` | `\x1b[5~` |
| `PageDown` | `\x1b[6~` |
| `Ctrl`+letter | `charCode - 96` (e.g. `Ctrl`+`C` → `0x03`) |
| `F1`–`F12` | Standard VT sequences |
| Single printable char | UTF-8 encoded |

Unrecognized combinations return an empty `Uint8Array` (no-op).

### `TOOL_SHORTCUTS` map

Static map in `src/lib/toolShortcuts.ts`:

```ts
const TOOL_SHORTCUTS: Record<string, ToolShortcutDef[]> = {
  "tddy-coder": [
    { label: "Shift+Tab", keys: ["Shift", "Tab"] },
    { label: "Ctrl+C",    keys: ["Ctrl", "C"] },
    { label: "Escape",    keys: ["Escape"] },
  ],
  "claude-cli": [
    { label: "Escape",    keys: ["Escape"] },
    { label: "Ctrl+R",   keys: ["Ctrl", "R"] },
    { label: "Ctrl+C",   keys: ["Ctrl", "C"] },
  ],
  "default": [],
};
```

Tool identifier resolution (`toolIdentifierFromPath(path: string): string`):
- If path basename contains `"tddy-coder"` → `"tddy-coder"`
- If path basename contains `"tddy-tools"` → `"tddy-tools"`
- Otherwise → `"default"`

Claude CLI sessions always use `"claude-cli"` (identified via `claudeCli` field in attachment).

### `SessionAttachment` extension

Add to `SessionAttachment` in `multiSessionState.ts`:

```ts
shortcuts?: ToolShortcutDef[];
```

Resolved once at session-attach time in `ConnectionScreen` and stored immutably in the attachment map. Never re-derived after that.

Resolution function (`resolveShortcutsForSession`):

```ts
function resolveShortcutsForSession(
  isClaudeCli: boolean,
  toolPath: string,
): ToolShortcutDef[] {
  if (isClaudeCli) return TOOL_SHORTCUTS["claude-cli"] ?? [];
  const id = toolIdentifierFromPath(toolPath);
  return TOOL_SHORTCUTS[id] ?? TOOL_SHORTCUTS["default"] ?? [];
}
```

Called in 4 places in `ConnectionScreen`: `handleStartSession`, `handleConnectSession`, `handleResumeSession`, and the deep-link `useEffect`.

## Components

### `ShortcutDrawer` (new)

**File:** `src/components/connection/ShortcutDrawer.tsx`

**Props:**
```ts
interface ShortcutDrawerProps {
  shortcuts: ToolShortcutDef[];
  onSend: (bytes: Uint8Array) => void;
}
```

**Behavior:**
- `position: fixed`, `z-index: 200` (above terminal, below fullscreen overlay at 300+)
- Not rendered when `shortcuts` is empty
- Initial snap: `bottom` edge, horizontally centered
- Drag handle: grip icon (`GripVertical` from lucide-react) — uses `setPointerCapture` / `releasePointerCapture` (same pattern as overlay pane drag in `ConnectedTerminal`)
- On `pointerUp`: snap to nearest edge. Edge proximity measured by Manhattan distance from panel center to each edge midpoint. Snapped state: `{ edge: "top"|"bottom"|"left"|"right", offset: number }` where `offset` is the position along the edge (preserves user's placement along that axis, avoiding jump to center)
- Layout: horizontal button row when snapped to `top`/`bottom`; vertical button column when snapped to `left`/`right`
- Position clamped to visual viewport bounds during drag
- Bottom-snap position uses `useVisualViewport` height (passed as prop) so the drawer clears the software keyboard when it's open

**Props addition for viewport height:**
```ts
interface ShortcutDrawerProps {
  shortcuts: ToolShortcutDef[];
  onSend: (bytes: Uint8Array) => void;
  viewportHeight: number;  // from useVisualViewport, 0 = use window.innerHeight
}
```

### `GhosttyTerminalLiveKit` changes

New prop:
```ts
mobileShortcuts?: ToolShortcutDef[];
```

When `showMobileKeyboard` is true and `mobileShortcuts` is non-empty, render:
```tsx
<ShortcutDrawer
  shortcuts={mobileShortcuts}
  onSend={pushInput}
  viewportHeight={viewportHeight}
/>
```

`pushInput` is already the single write path for all terminal input (keyboard, SGR mouse) — reuse without modification.

`GhosttyTerminalLiveKit` needs `viewportHeight` passed in (currently not a prop). Add:
```ts
viewportHeight?: number;
```

### `ConnectedTerminal` changes

Receives `shortcuts?: ToolShortcutDef[]` from `ConnectionScreen` (via `focusedAttachment.shortcuts`). Passes to `GhosttyTerminalLiveKit` as `mobileShortcuts={shortcuts}` and `viewportHeight={viewportHeight}` (already available from `useVisualViewport`).

### `ConnectionScreen` changes

At each session attachment creation site, add shortcuts resolution:

```ts
const shortcuts = resolveShortcutsForSession(
  isClaudeCli,
  form.toolPath,  // "" for connect/resume — fallback to "default"
);
// stored in addSessionAttachment(..., { ..., shortcuts })
```

For `handleConnectSession` / `handleResumeSession`, `toolPath` is not available (no active form is in scope at the call site). Use `""` → `"default"` shortcuts for v1. Full resolution is only available at `handleStartSession` time (form is explicit) and in the deep-link `useEffect` where the session's agent is known — for the deep-link case use `isClaudeCli` flag from `isClaudeCliSession(sess.agent)` and `toolPath: ""` (default) since no project form is in scope.

## Snap Behavior Detail

```
snap edge = argmin over {top, bottom, left, right} of:
  distance(panelCenter, edgeMidpoint)

snapped position formula:
  top:    top = MARGIN, left = clamp(offset, MARGIN, vw - panelWidth - MARGIN)
  bottom: top = vh - panelHeight - MARGIN, left = clamp(offset, ...)
  left:   left = MARGIN, top = clamp(offset, MARGIN, vh - panelHeight - MARGIN)
  right:  left = vw - panelWidth - MARGIN, top = clamp(offset, ...)

MARGIN = 8px
vh = viewportHeight (visual viewport, accounts for keyboard)
vw = window.innerWidth
```

## File Map

| File | Change |
|------|--------|
| `src/lib/toolShortcuts.ts` | New — `ToolShortcutDef`, `TOOL_SHORTCUTS`, `keySequenceToBytes`, `toolIdentifierFromPath`, `resolveShortcutsForSession` |
| `src/lib/toolShortcuts.test.ts` | New — unit tests |
| `src/components/connection/ShortcutDrawer.tsx` | New — draggable floating panel |
| `src/components/connection/ShortcutDrawer.cy.tsx` | New — Cypress component tests |
| `src/components/connection/multiSessionState.ts` | Add `shortcuts?: ToolShortcutDef[]` to `SessionAttachment` |
| `src/components/GhosttyTerminalLiveKit.tsx` | Add `mobileShortcuts?` + `viewportHeight?` props, render `ShortcutDrawer` |
| `src/components/ConnectionScreen.tsx` | Resolve and store shortcuts at 4 attach sites; pass through `ConnectedTerminal` |

## Testing

**Unit tests (`toolShortcuts.test.ts`):**
- `keySequenceToBytes` — all named keys, Ctrl+letter range, Shift+Tab, unknown key → empty
- `toolIdentifierFromPath` — debug/release paths, unknown path → default
- `resolveShortcutsForSession` — claude-cli flag, known tool, unknown tool

**Cypress component tests (`ShortcutDrawer.cy.tsx`):**
- Renders buttons for each shortcut label
- Clicking a button calls `onSend` with the correct byte sequence
- Empty shortcuts → nothing rendered
- Drag and snap: drag to right half → snaps to right edge
- Layout: horizontal when snapped top/bottom, vertical when left/right

## Out of Scope

- Proto / Rust changes (daemon YAML `mobile_shortcuts` field) — deferred to a follow-up if runtime editability is needed
- Shortcut drawer in compact/overlay terminal pane layouts
- Per-agent (not just per-tool) configuration
- Persisting snap position across page reloads
