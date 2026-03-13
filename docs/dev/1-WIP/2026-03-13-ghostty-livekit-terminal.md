# Changeset: Ghostty Terminal Integration via LiveKit

**Date**: 2026-03-13
**Status**: ✅ Complete (pending user review)
**Type**: Feature

## Affected Packages

- **tddy-livekit** (Rust): New TerminalServiceImpl + terminal.proto
- **tddy-livekit-web** (TS): Proto codegen for terminal messages
- **tddy-web** (TS): GhosttyTerminal component, Storybook stories, E2E test
- **tddy-coder** (Rust): Add LiveKit args to DemoArgs, wire terminal byte capture to LiveKit participant
- **tddy-grpc** (Rust): Add StreamTerminalIO bidi RPC + TerminalInput message to remote.proto

## Related Feature Documentation

- [PRD-2026-03-13-ghostty-livekit-terminal.md](../../ft/web/1-WIP/PRD-2026-03-13-ghostty-livekit-terminal.md)

## Summary

Integrate ghostty-web terminal emulator into tddy-web, streaming terminal output from tddy-demo over LiveKit via a new TerminalService RPC. Includes a standalone Storybook component and a Cypress E2E test that validates rendered content through the full stack.

## Background

ghostty-web is a WASM terminal emulator with xterm.js-compatible API. LiveKitTransport already supports bidi streaming. tddy-demo produces deterministic TUI output. This changeset wires terminal bytes from tddy-demo through LiveKit to the browser and sends keyboard/mouse input back.

## Scope

- [ ] **Package Documentation**: Update package READMEs and dev docs
- [x] **Implementation**: Complete code changes across affected packages
- [x] **Testing**: All acceptance tests passing
- [x] **Integration**: Cross-package integration verified
- [x] **Technical Debt**: Production readiness gaps addressed
- [x] **Code Quality**: Linting, type checking, and code review complete

## Technical Changes

### State A (Current)

- tddy-livekit has EchoServiceImpl only; no terminal RPC
- tddy-demo has no LiveKit CLI args; terminal bytes go to gRPC only
- tddy-web has no GhosttyTerminal component
- No E2E test for terminal streaming

### State B (Target)

- tddy-livekit: TerminalServiceImpl with StreamTerminalIO bidi RPC
- tddy-demo: LiveKit args, byte capture wired to LiveKit participant
- tddy-web: GhosttyTerminal component, Storybook stories, E2E test
- E2E: Cypress starts tddy-demo, connects via LiveKit, asserts terminal buffer text

### Delta

#### tddy-livekit
- Add terminal.proto with TerminalService
- Implement TerminalServiceImpl (broadcast output, mpsc input sink)
- Register in lib.rs

#### tddy-coder
- Add livekit_url, livekit_token, livekit_room, livekit_identity to DemoArgs
- Wire byte capture to LiveKit participant in run_full_workflow_tui

#### tddy-livekit-web
- Generate TypeScript from terminal.proto
- Export types from index.ts

#### tddy-web
- Add ghostty-web dependency
- Create GhosttyTerminal component
- Create Storybook stories (Default, WithContent, ColorPalette)
- Create E2E test with startTerminalServer/stopTerminalServer tasks

## Implementation Milestones

- [x] Milestone 1: Add terminal.proto to tddy-livekit, TerminalInput to remote.proto, implement TerminalServiceImpl
- [x] Milestone 2: Add LiveKit args to DemoArgs, wire byte capture to LiveKit participant
- [x] Milestone 3: Generate TypeScript from terminal.proto in tddy-livekit-web
- [x] Milestone 4: Create GhosttyTerminal React component
- [x] Milestone 5: Create Storybook stories (Default, WithContent, ColorPalette, LiveKitConnected)
- [x] Milestone 6: Create E2E test with Cypress orchestration

## Acceptance Tests

### tddy-livekit
- [x] **Unit**: terminal.proto compiles, TerminalServiceImpl handles StreamTerminalIO

### tddy-web
- [x] **Component**: GhosttyTerminal renders ANSI content
- [x] **Storybook**: Stories exist (Default, WithContent, ColorPalette, LiveKitConnected)
- [x] **E2E**: Cypress asserts terminal buffer text through full stack

## Implementation Progress

**Last Synced with Code**: 2026-03-13 (via validate-changes)

**Core Features**: All milestones complete. Implementation includes GhosttyTerminal, GhosttyTerminalLiveKit, terminal.proto, TerminalServiceImpl, LiveKit wiring in tddy-demo, E2E test with startTerminalServer/stopTerminalServer tasks.

**Testing**: cargo test, cypress:component, cypress:e2e — all pass.

## Validation Results

### Change Validation (@validate-changes)

**Last Run**: 2026-03-13
**Status**: ✅ Passed
**Risk Level**: 🟢 Low

**Build Validation**:
| Package | Status | Notes |
|---------|--------|-------|
| tddy-livekit | ✅ Pass | Built successfully |
| tddy-coder | ✅ Pass | Built successfully |
| tddy-demo | ✅ Pass | Built successfully |
| tddy-web | ✅ Pass | bun build succeeded |

**Analysis Summary**: No critical issues. Implementation aligns with plan.

### Test Validation (@validate-tests)

**Last Run**: 2026-03-13
**Status**: ✅ Passed

**Tests Analyzed**:
- ghostty-terminal.cy.ts (1 E2E) — assertions on streamed-byte-count, terminal-buffer-text
- ghostty-terminal-stories.cy.ts (3 E2E) — Default, WithContent, ColorPalette stories
- GhosttyTerminal.cy.tsx (2 component) — initialContent render, onData callback
- terminal_service_acceptance.rs (1 integration) — StreamTerminalIO bytes flow
- rpc_scenarios.rs — StreamTerminalIO bidi tests

**Quality Summary**: No anti-patterns. Tests have proper assertions, deterministic where possible. E2E skips when LIVEKIT_TESTKIT_WS_URL unset (appropriate). Rust tests use #[serial] for LiveKit isolation.

### Production Readiness (@validate-prod-ready)

**Last Run**: 2026-03-13
**Status**: ✅ Ready

**Summary**:
- Mock/Stub in production: N/A (StubBackend is intentional for tddy-demo)
- Console statements: Guarded by debug/debugMode flags (GhosttyTerminalLiveKit, LiveKitTransport)
- TODO/FIXME in feature files: None
- Unused code: None identified

**Blockers**: None.

### Code Quality (@analyze-clean-code)

**Last Run**: 2026-03-13
**Overall Score**: 8/10 ⭐

**Summary**:
- Function length: Within thresholds (terminal_service.rs, GhosttyTerminal.tsx, GhosttyTerminalLiveKit.tsx)
- Nesting depth: ≤3 levels
- Parameter count: Within thresholds
- Magic values: Acceptable (timeouts, defaults documented)

**Priority Fixes**: None critical.

## Technical Debt & Production Readiness

- None identified for this changeset.

## Decisions & Trade-offs

- TerminalService in tddy-livekit/proto/ — separate from remote.proto for LiveKit RPC independence
- GhosttyTerminal standalone — no LiveKit dependency in component
- term.buffer API for E2E assertions — avoids brittle pixel/screenshot matching

## References

- Plan: ghostty_livekit_terminal_985da7d6.plan.md
