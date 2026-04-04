# Changeset: Web terminal font zoom (pitch in / out)

**Date**: 2026-04-04  
**Status**: ✅ Complete (implementation + docs; merge pending)  
**Type**: Feature  
**Branch / worktree**: `feature/terminal-zoom-pitch` · `/var/tddy/Code/tddy-coder/.worktrees/terminal-zoom-pitch`

## Affected Packages

- **tddy-web** (primary): `terminalZoom`, `terminalZoomBridge`, `TerminalZoomToolbar`, `GhosttyTerminal`, `GhosttyTerminalLiveKit`, `ConnectionTerminalChrome`, Cypress + Bun tests  
- **docs** (product): `docs/ft/web/web-terminal.md`, `docs/ft/web/changelog.md`, `docs/dev/changesets.md`

## Related Feature Documentation

- [web-terminal.md](../../ft/web/web-terminal.md) — Font zoom (pitch) section  
- [changelog.md](../../ft/web/changelog.md) — 2026-04-04 entry  

## Summary

Embedded web terminal supports pitch-in / pitch-out / reset via a toolbar and keyboard (Ctrl/⌘ +/-/0) when focus is on the terminal or toolbar. Font changes run through `GhosttyTerminal` (`options.fontSize` + `FitAddon.fit()`), expose live size on `data-terminal-font-size`, and reuse the existing resize path so the TUI receives `\x1b]resize;cols;rows\x07`. Bridge and sync payloads are validated with `parseTerminalZoomBridgeDetail` / `parseTerminalFontSizeSyncDetail`. Verbose logging is opt-in via `GhosttyTerminal` `debugLogging` or `VITE_TERMINAL_ZOOM_DEBUG=true`.

## Implementation Progress

**Last synced with code**: 2026-04-04 (via @validate-changes)

**Core features**:

- [x] Pure zoom math (`terminalZoom.ts`) + unit tests — ✅ Complete  
- [x] Bridge + sync events + parsers (`terminalZoomBridge.ts`) + unit tests — ✅ Complete  
- [x] `TerminalZoomToolbar` (buttons, keyboard, sync listener) — ✅ Complete  
- [x] `GhosttyTerminal` (`applyFontSizePx`, bridge subscription, `data-terminal-font-size`, `setTerminalFontSize`) — ✅ Complete  
- [x] `ConnectionTerminalChrome` (`terminalBaselineFontSize`, toolbar) — ✅ Complete  
- [x] `GhosttyTerminalLiveKit` (`fontSize` prop + toolbar when no overlay) — ✅ Complete  
- [x] Feature + changelog + wrapped entries (`docs/dev/changesets.md`, `packages/tddy-web/docs/changesets.md`) — ✅ Complete  
- [x] `.gitignore` — `.tddy-red-test-output.txt` — ✅ Complete  

**Testing**:

- [x] Bun — `terminalZoom.test.ts`, `terminalZoomBridge.test.ts`, rest of `packages/tddy-web/src/lib` — ✅ 14 passed (2026-04-04)  
- [x] Cypress component — `TerminalZoomAcceptance.cy.tsx` — ✅ 5 passed (2026-04-04)  

**Session / planning artifacts** (external): `~/.tddy/sessions/019d5779-88d7-7030-86ca-f311f7937aca` — state `DocsUpdated` for this feature.

## Technical Changes (high level)

| Area | Change |
|------|--------|
| Lib | `DEFAULT_*` bounds, `clampTerminalFontSize`, pitch helpers, `canPitchIn` / `canPitchOut` |
| Bridge | `TERMINAL_ZOOM_BRIDGE_EVENT`, `TERMINAL_FONT_SIZE_SYNC_EVENT`, `isTerminalZoomDebugEnabled`, parsers, dispatch helpers |
| UI | `TerminalZoomToolbar`; `GhosttyTerminal` min/max font props, bridge effect, imperative font API |
| LiveKit / chrome | Toolbar placement; `fontSize` passed to `GhosttyTerminal` and toolbar baseline |

## Acceptance Criteria

- [x] Toolbar test IDs and `data-terminal-font-size` for assertions  
- [x] Resize fires after font change (Cypress + LiveKit OSC logging scenario)  
- [x] Bounds 8–32 default; buttons disabled at limits  
- [x] Docs describe keyboard shortcuts and optional `VITE_TERMINAL_ZOOM_DEBUG`  

### Change Validation (@validate-changes)

**Last run**: 2026-04-04  
**Status**: ✅ Passed (with notes below)  
**Risk level**: 🟢 Low  

**Changeset sync**:

- ✅ This changeset created to track **this** branch; it replaces ad-hoc folder-only notes as the canonical WIP record for terminal zoom.  
- ⚠️ Separate file `docs/dev/1-WIP/2026-04-03-workflow-free-prompting-validate-changes.md` remains **🚧 In Progress** for an unrelated slice (workflow free-prompting); do not conflate with terminal zoom.  

**Documentation validation**:

- Feature docs updated: `web-terminal.md`, `changelog.md` align with code (parsers, keyboard, env flag).  
- `docs/dev/1-WIP/terminal-zoom-pitch/validate-prod-ready-report.md` and `analyze-clean-code-report.md` were written against an **earlier** revision; several findings (e.g. logging inside pure `terminalZoom` helpers, unvalidated sync detail) are **addressed in current code**. Treat those reports as historical unless refreshed.  

**Analysis summary**:

| Check | Result |
|--------|--------|
| `bun run build --filter tddy-web` | ✅ Pass |
| `bun test packages/tddy-web/src/lib` | ✅ 14 pass |
| Cypress `TerminalZoomAcceptance.cy.tsx` | ✅ 5 pass |
| `cargo test` (full workspace, `./dev`) | ❌ Failed: `tddy-integration-tests` ACP tests require `cargo build -p tddy-acp-stub` — **not caused by this branch** (no Rust changes) |

**Risk assessment**:

| Area | Level | Notes |
|------|-------|--------|
| Build (tddy-web) | 🟢 Low | Vite + `tsc` OK |
| Changeset alignment | 🟢 Low | This file matches `master...HEAD` scope |
| Test infrastructure | 🟢 Low | No test-only branches in production paths |
| Production code | 🟢 Low | Validated event payloads; gated verbose logs |
| Security | 🟢 Low | Same-origin `CustomEvent` coordination; no HTML injection in zoom path |
| Code quality | 🟡 Medium | Global `window` listeners (documented); `GhosttyTerminal` still large (mostly pre-existing) |

## Refactoring Needed (optional follow-ups)

Deferred post-merge (not blocking PR):

- [ ] Optional: single exported `DEFAULT_TERMINAL_FONT_SIZE = 14` shared with LiveKit/toolbar if defaults ever diverge further.  
- [ ] Optional: `useTerminalZoomBridge` hook or instance-scoped events if multiple terminals per page become a requirement.  
- [ ] Refresh or archive `docs/dev/1-WIP/terminal-zoom-pitch/validate-prod-ready-report.md` / `analyze-clean-code-report.md` to match post-refactor code, or mark explicitly as superseded.  

### Test validation (@validate-tests) — PR wrap

**Last run**: 2026-04-04  
**Status**: ✅ Passed  

- **Bun**: `terminalZoom.test.ts` and `terminalZoomBridge.test.ts` cover pure math and event parsers; no production code mocked inappropriately.  
- **Cypress**: `TerminalZoomAcceptance.cy.tsx` exercises toolbar → bridge → terminal → resize; LiveKit scenario uses console stub for OSC logging (acceptable for component test).  
- **Gaps (non-blocking)**: Isolated `TerminalZoomToolbar` unit tests optional; bridge dispatch could be unit-tested further (already partially covered by parsers).  

### Production readiness (@validate-prod-ready) — PR wrap

**Last run**: 2026-04-04  
**Status**: ✅ Passed  

- No `TODO` / `FIXME` in `terminalZoom.ts`, `terminalZoomBridge.ts`, `TerminalZoomToolbar.tsx`, or zoom paths in `GhosttyTerminal.tsx`.  
- Verbose logging gated by `debugLogging` / `VITE_TERMINAL_ZOOM_DEBUG`; bridge and sync payloads validated before use.  
- No test-only imports in production zoom modules.  

### Clean code (@analyze-clean-code) — PR wrap

**Last run**: 2026-04-04  
**Status**: ✅ Acceptable  

- Separation: pure lib → bridge → UI → `GhosttyTerminal` integration is clear.  
- Remaining tradeoffs: global `window` events (documented), large `GhosttyTerminal` file (mostly pre-existing). Optional refactors listed above.  

### Final validation (@validate-changes) — PR wrap

**Last run**: 2026-04-04 (after fmt/clippy/test)  
**Status**: ✅ Passed — no new issues from tooling.  

### Linting and type checking — PR wrap

| Step | Command | Result |
|------|---------|--------|
| Rust format | `./dev cargo fmt --all` | ✅ |
| Rust clippy | `./dev cargo clippy --workspace --all-targets -- -D warnings` | ✅ |
| Rust tests | `./test` (builds `tddy-coder`, `tddy-tools`, `tddy-livekit`, `tddy-acp-stub` + full `cargo test`) | ✅ `exit_code: 0` (see `.verify-result.txt`) |
| Web build | `./dev bun run build --filter tddy-web` | ✅ (prior run) |
| Web unit | `./dev bun test packages/tddy-web/src/lib` | ✅ 14 passed (prior run) |
| Web Cypress | `TerminalZoomAcceptance.cy.tsx` | ✅ 5 passed (prior run) |

### Documentation wrap (@wrap-context-docs)

**Status**: ⚠️ **Source changeset retained** (not deleted)

- **Feature docs (State B)** already merged: `docs/ft/web/web-terminal.md`, `docs/ft/web/changelog.md`, `docs/dev/changesets.md`, `packages/tddy-web/docs/changesets.md` describe the shipped behavior.  
- **Optional follow-ups** above remain open; per workflow, full wrap (transfer + delete WIP file) can run after those are done or explicitly deferred in a later changeset.  

## References

- Sub-reports: `docs/dev/1-WIP/terminal-zoom-pitch/` (`validate-tests-report.md`, `refactoring-plan.md`, …)  
- Session: `019d5779-88d7-7030-86ca-f311f7937aca` (`changeset.yaml`, `progress.md`)  
