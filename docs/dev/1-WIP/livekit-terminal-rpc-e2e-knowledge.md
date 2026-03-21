# LiveKit Terminal RPC E2E — Current Knowledge

Knowledge document for the LiveKit and gRPC StreamTerminalIO e2e tests, virtual terminal viewer, and tddy-web Ghostty integration.

## Test Status

| Protocol | Reliability | Notes |
|----------|-------------|-------|
| **gRPC** | Passes reliably | Single VirtualTui per connection; no interleaving. Use for CI. |
| **LiveKit** | Flaky | `wait_pc_connection timed out` on macOS; interleaved output from multiple VirtualTuis can produce garbage stripped text. Pre-started server (`LIVEKIT_TESTKIT_WS_URL`) may improve stability. |

## Architecture Overview

```
┌─────────────────┐   LiveKit / gRPC    ┌─────────────────────┐
│  tddy-web       │◄──────────────────►│  tddy-coder         │
│  GhosttyTerminal│  StreamTerminalIO   │  TerminalServiceImpl │
│  (browser)      │  TerminalInput/     │  PerConnection /     │
│  or e2e test    │  TerminalOutput    │  TddyRemoteService   │
└─────────────────┘                     │  → VirtualTui        │
                                         └─────────────────────┘
```

- **RPC**: `StreamTerminalIO` — bidirectional stream of `TerminalInput` (keyboard) and `TerminalOutput` (ANSI bytes).
- **LiveKit**: `TerminalServiceImplPerConnection` + `TerminalServiceServer` (RpcService).
- **gRPC**: `TddyRemoteService` with `with_view_connection_factory` (tonic).
- **Server**: Creates one `VirtualTui` per connection via `ViewConnection` factory. gRPC: one per stream. LiveKit bidi: one per message (multiple VirtualTuis).
- **Client (tddy-web)**: `GhosttyTerminalLiveKit` uses `term.write(output.data)` and `onData` → input queue.

## Sequence of Events (Test Flow)

### 1. Setup

1. `LiveKitTestkit::start()` — starts LiveKit server (testcontainers or `LIVEKIT_TESTKIT_WS_URL`).
2. `spawn_presenter_with_view_connection_factory(Some("Build auth"))` — presenter with StubBackend, initial feature "Build auth".
3. Server connects to room, serves `TerminalService`.
4. Client connects, waits for server participant.

### 2. RPC Call

- **Server-stream** (`livekit_terminal_io_receives_ansi_output`): single `TerminalInput { data: [] }`, receive `TerminalOutput` stream.
- **Bidi-stream** (`livekit_terminal_io_keyboard_input_affects_output`, `livekit_ghostty_virtual_terminal_e2e`): list of `TerminalInput` messages sent upfront via `call_bidi_stream`.

### 3. Virtual Keyboard Sequence (Bidi Tests)

| Step | Input | Bytes | Intent |
|------|-------|-------|--------|
| 0 | (init) | `[]` | Start stream |
| 1 | Enter | `\r` | Submit feature "Build auth" (or first Select option) |
| 2 | Enter | `\r` | Answer Scope: "Email/password" (first option) |
| 3 | Enter | `\r` | (if more questions) |
| 4 | Down | `\x1b[B` | Navigate PlanReview menu to "Approve" |
| 5 | Enter | `\r` | Approve plan |

### 4. Presenter State Flow (StubBackend, "Build auth")

1. **FeatureInput** → user types "Build auth", Enter → `SubmitFeatureInput`.
2. **Running** → workflow starts, Goal: plan.
3. **Select** (Scope) → "Which authentication method do you want?" — Enter selects first option.
4. **Running** → plan generated.
5. **PlanReview** → View / Approve / Refine. Down + Enter → Approve.
6. **Running** → acceptance-tests, red, green, etc.
7. **Select** (Permission) — if demo/acceptance-tests asks.
8. **Done** or further goals.

## Visible Content — Before vs After Keyboard

*Examples below are hypothetical (inferred from render code); actual output may differ due to interleaving (LiveKit) or timing.*

### Before Keyboard (Initial Screen)

After first `TerminalOutput` chunks, stripped ANSI might show:

```
State: — → plan
Goal: plan │ State: plan │ 0s │ stub opus │ PgUp/PgDn scroll
Type your feature description and press Enter...
```

Or with clarification question (Scope):

```
Scope
Which authentication method do you want?
  ○ Email/password  Traditional login
  ● OAuth          Social login
  ○ Other (type your own)
Up/Down navigate  Enter select
```

### After Keyboard (Progressed)

After Enter×3 + Down + Enter:

```
State: plan → AcceptanceTesting
Goal: acceptance-tests │ State: AcceptanceTesting │ ...
Plan dir: /tmp/tddy-e2e-...
```

Or:

```
Workflow complete. Press Enter to exit.
```

### Example Assertion Strings

Tests assert on stripped text containing any of:

- `State:` — status bar
- `Goal:` — status bar
- `Feature` — prompt "Type your feature..."
- `plan` — goal name
- `Build` — from "Build auth"
- `Plan dir:` — after plan approval
- `AcceptanceTesting` — state name
- `GreenComplete` — state
- `Workflow complete` — Done mode
- `DocsUpdated` — state
- `Type your feature` — FeatureInput prompt

## Virtual Terminal Viewer (vt100)

### Intended Role

`VirtualTerminalViewer` mimics Ghostty: receives ANSI via RPC, parses with `vt100::Parser`, exposes `screen().contents()` for assertions.

### Current Limitation

**vt100 output is unreliable** for ratatui/crossterm ANSI:

- **Observed**: `parser.screen().contents()` returns garbage, e.g.  
  `"|n→13;;            ;;1791;moHH;m[p1e1;H:mm;;SW;1chm18|Hoi[;c11181eh]H;m:  S1H;Wucmhtoihp;m1ceemhn:["`
- **Causes**: ratatui/crossterm use different escape sequences; output may be interleaved from multiple VirtualTui streams (bidi creates one VirtualTui per message).

### Fallback

Tests use `strip_ansi_escapes::strip()` on raw bytes + `String::from_utf8_lossy` for assertions (same as `livekit_terminal_io_keyboard_input_affects_output`).

## Bidi Stream Behavior

`call_bidi_stream` sends all messages in sequence. The participant processes each message immediately (bidi mode). Each message triggers `handle_rpc_stream` with a single-element batch → **multiple VirtualTuis** are created (one per message). They share the same presenter via `connect_view()`. Output from all streams is interleaved on the client `rx`.

## Keyboard Encoding (VirtualTui)

Raw bytes compatible with `parse_key_from_buf` in `packages/tddy-tui/src/virtual_tui.rs`:

| Key | Bytes |
|-----|-------|
| Enter | `\r` (`0x0d`) |
| Down | `\x1b[B` (CSI B) |
| Up | `\x1b[A` (CSI A) |

## LiveKit / macOS Notes

- **`-ObjC` linker flag**: Required for libwebrtc on macOS (Objective-C categories). Add to `.cargo/config.toml`:
  ```toml
  [target.aarch64-apple-darwin]
  rustflags = ["-C", "link-arg=-ObjC"]
  [target.x86_64-apple-darwin]
  rustflags = ["-C", "link-arg=-ObjC"]
  ```
- **`wait_pc_connection timed out`**: WebRTC peer connection timeout; often network/TURN/ICE. Pre-started server via `LIVEKIT_TESTKIT_WS_URL` can improve stability.

## Test Files

### LiveKit (`tests/livekit_terminal_rpc.rs`, `--features livekit`)

| Test | RPC Mode | Assertion |
|------|----------|-----------|
| `livekit_terminal_io_receives_ansi_output` | Server stream | Stripped text contains State/Goal/Feature/plan/Build |
| `livekit_terminal_io_keyboard_input_affects_output` | Bidi stream | Stripped text contains Plan dir/AcceptanceTesting/... or len > 100 |
| `livekit_ghostty_virtual_terminal_e2e` | Bidi stream | VirtualTerminalViewer (vt100) feeds output; asserts on stripped. Flaky due to interleaving. |

### gRPC (`tests/grpc_terminal_rpc.rs`)

| Test | Protocol | Assertion |
|------|----------|-----------|
| `grpc_terminal_io_receives_ansi_output` | TddyRemote.StreamTerminalIO | Stripped text contains State/Goal/Feature/plan/Build |
| `grpc_terminal_io_keyboard_input_affects_output` | TddyRemote.StreamTerminalIO | Stripped text contains Plan dir/AcceptanceTesting/... or len > 100 |
| `grpc_ghostty_virtual_terminal_e2e` | TddyRemote.StreamTerminalIO | VirtualTerminalViewer (vt100) feeds output; asserts on stripped. **Passes reliably.** |

gRPC uses a single VirtualTui (one stream of input) so output is not interleaved. LiveKit bidi creates multiple VirtualTuis (one per message); output is interleaved.

## References

- `packages/tddy-e2e/tests/livekit_terminal_rpc.rs`
- `packages/tddy-e2e/tests/grpc_terminal_rpc.rs`
- `packages/tddy-web/src/components/GhosttyTerminalLiveKit.tsx`
- `packages/tddy-web/src/components/GhosttyTerminal.tsx` — `getBufferText()`
- `packages/tddy-tui/src/virtual_tui.rs` — `run_virtual_tui`, key parsing
- `packages/tddy-core/src/backend/stub.rs` — StubBackend clarification questions

## PR wrap validation (2026-03-21)

| Check | Result |
|-------|--------|
| `cargo fmt` | PASS (workspace formatted) |
| `cargo clippy -- -D warnings` | PASS |
| `./test` | PASS (all crates; 0 failed) |
| Validate changes (manual) | PASS — aligns with web terminal + VirtualTui Ctrl+C + coder-disconnect UX |
| Validate tests (manual) | PASS — `ghostty-terminal.cy.ts` e2e for coder exit requires `LIVEKIT_TESTKIT_WS_URL` + Storybook serve |
| Validate prod-ready (manual) | PASS — no `NODE_ENV === test` in changed web paths; LiveKit handlers use real events |
| Clean code | B — acceptable for merge; optional follow-up: extract banner copy to constant |

**Ready for review:** toolchain green; commit excludes untracked debug logs (`web-debug.txt`, `webrtc-debug.txt`) unless intentionally included.
