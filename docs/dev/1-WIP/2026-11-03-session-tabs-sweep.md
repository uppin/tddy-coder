# Changeset: `#session-tabs` sweep — logic fixes and backlog records

**Date**: 2026-11-03
**Status**: 🚧 In Progress
**Type**: Bug Fix
**Complexity**: 4 — three packages (`tddy-web`, `tddy-terminal-rpc`, docs), the shared PTY spawn path every session's main terminal uses, an unresolved design for the full privilege-drop unification

Discovery: [2026-11-03-session-tabs-sweep-initial-discovery.md](./2026-11-03-session-tabs-sweep-initial-discovery.md)

## Prerequisites

| Item | Verdict |
|---|---|
| [`oversized-file-ghostty-terminal.md`](../../../packages/tddy-web/docs/code-issues/oversized-file-ghostty-terminal.md) | **Deferred** by the developer 2026-11-03. The fix removes lines; no split. |
| [`oversized-file-session-main-pane.md`](../../../packages/tddy-web/docs/code-issues/oversized-file-session-main-pane.md) | **Deferred** by the developer 2026-11-03, untouched. |
| Five reconciled records (`tddy-agent-launch`, `tddy-daemon-sandbox`, `tddy-task`, `tddy-terminal-rpc`) | **Deferred** by the developer 2026-11-03: "code issues deferred, logic issues solved". |
| [`2026-11-02-ghostty-terminal-initial-content-written-twice.md`](../todo/2026-11-02-ghostty-terminal-initial-content-written-twice.md) | **Solved here**; file deleted. |
| [`2026-11-02-tmux-listing-and-tab-use-two-privilege-drops.md`](../todo/2026-11-02-tmux-listing-and-tab-use-two-privilege-drops.md) | **Narrowed here** (shared decision and refusal); the mechanism half stays open, see Decisions. |
| [`2026-09-06-unprivileged-daemon-cannot-probe-another-os-user.md`](../todo/2026-09-06-unprivileged-daemon-cannot-probe-another-os-user.md) | **Still open, not affected**: needs a deployment decision (supervisor-brokered spawn). Verdict recorded in the entry. |
| [`2026-11-02-osc52-clipboard-reachability-unverified.md`](../todo/2026-11-02-osc52-clipboard-reachability-unverified.md) | **Deferred** with the developer's consent, recorded in the entry. |

## Affected Packages

- **tddy-web**: `src/components/GhosttyTerminal.tsx`, `src/components/sessions/useTmuxSessions.ts`; Cypress specs and driver.
- **tddy-terminal-rpc**: `src/pty_runtime.rs`.
- Docs at wrap: [session-terminal-tabs.md](../../ft/web/session-terminal-tabs.md), [terminal-osc52-clipboard.md](../../../packages/tddy-web/docs/terminal-osc52-clipboard.md).

## State A

1. `GhosttyTerminal` writes `initialContent` in its setup effect and again in the `[initialContent, ready]` effect: content renders twice, an OSC 52 inside it copies twice.
2. `useTmuxSessions` without a client reports `loading` for ever: "Loading tmux sessions…".
3. The tmux listing refuses a non-root daemon with `CannotBecomeUser` before forking; the PTY spawn (main terminal, every as-user tab) does not: it launches `setpriv`, which fails inside the PTY.

## State B

1. `initialContent` is written once, by the `ready` effect, with the OSC 52 tap beside that write.
2. With no client the menu shows `hostNotConnectedText(null)`, not loading; when the client arrives the listing runs and the error clears.
3. The PTY spawn plans its privilege change with the kernel's `plan_privilege_change_from` and refuses with the same `CannotBecomeUser` text before opening a PTY. Same-user and root-daemon spawns are unchanged (`setpriv` front-loaded for the latter).

## Delta

- `GhosttyTerminal.tsx`: drop the write + tap at the end of the setup effect.
- `useTmuxSessions.ts`: derive `error`/`loading` from `client === null && requests > 0`.
- `pty_runtime.rs`: `resolve_final_argv_env_as(spec, &EffectiveIdentity)` using the kernel plan.

## Testing

| Test | Proves |
|---|---|
| `GhosttyTerminal.cy.tsx` "renders its initial content once…" | State B 1, render |
| `GhosttyTerminalOsc52Acceptance.cy.tsx` "…initial content copies exactly once" | State B 1, copy |
| `UseTmuxSessionsHostNotConnected.cy.tsx` (2) | State B 2 |
| `pty_runtime::tests` (3 new) | State B 3: refusal, root drop wrapped, same user untouched |
| Existing: `tmux_tabs_acceptance`, `tmux_new_session_acceptance`, `terminal_session_acceptance`, `agent_terminal_tabs_acceptance`, `dormant_session_tabs_acceptance` | No regression |

## Decisions & Trade-offs

- **Full unification (a `pre_exec` kernel drop in the PTY child) is not done.** portable-pty 0.9 offers no hook (see discovery 1); bypassing it rewrites the shared spawn path, and the Linux root drop cannot be exercised locally. Options for the developer: (a) patch/vendor portable-pty with a `pre_exec` hook; (b) own `openpty` + `StagedCommand` in `tddy-pty`; (c) keep `setpriv` and the shared decision made here. This changeset ships (c) and leaves the TODO narrowed.
- `ensure_reachable` stays: it refuses the RPC before a handle exists.
- Menu error flickers while a client is genuinely still connecting; accepted — it matches `useSessionTerminals`.

## TODO

- [x] Red: failing tests
- [x] Green: the three fixes (item 3 narrowed: shared decision + refusal; mechanism left open)
- [x] Backlog records: deferral lines, new plus-menu TODO
