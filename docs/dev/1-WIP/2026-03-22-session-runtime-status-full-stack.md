# Changeset: Session runtime status — full stack

**Date**: 2026-03-22  
**Status**: Complete (implementation + tests)  
**Type**: Feature  
**PRD**: [docs/ft/web/1-WIP/PRD-2026-03-22-session-runtime-status-full-stack.md](../../ft/web/1-WIP/PRD-2026-03-22-session-runtime-status-full-stack.md)

## Plan mode summary (technical)

**Approach**: Add a dedicated proto message `SessionRuntimeStatus` on `ServerMessage` (field 12) so Connect/LiveKit clients have one authoritative payload matching the TUI status line. Emit it from the presenter by extending `broadcast()`: after every `PresenterEvent` except `SessionRuntimeStatus` itself, send a second event `PresenterEvent::SessionRuntimeStatus(snapshot)` where `snapshot` is derived from `PresenterState` (goal, workflow state string, elapsed since `goal_start_time`, agent, model, plus a formatted `status_line` aligned with `tddy-tui::ui::format_status_bar` semantics without the scroll hint).

**Trade-offs**: Elapsed time updates only when another presenter event fires (not sub-second ticks like the TUI redraw timer). Acceptable until a dedicated tick or throttle is product-required.

**Alternatives considered**: Client-side composition from `goal_started` / `state_changed` only — rejected (no elapsed, no single line parity).

## Affected packages

| Package | Changes |
|---------|---------|
| `tddy-service` | `remote.proto`; `convert.rs` maps `PresenterEvent::SessionRuntimeStatus` → proto |
| `tddy-core` | `SessionRuntimeSnapshot`, `PresenterEvent::SessionRuntimeStatus`; `session_runtime.rs`; `broadcast()` chains snapshot |
| `tddy-tui` | `virtual_tui`: ignore snapshot (no local UI change) |
| `tddy-web` | `buf generate`; `GhosttyTerminalLiveKit` handles `sessionRuntimeStatus` only for bar text |
| `tddy-rust-typescript-tests` | Contract test + regenerated `gen/tddy/v1/remote_pb.ts` |

## Implementation milestones

- [x] Proto + tonic codegen
- [x] Core snapshot + broadcast wiring
- [x] Service conversion + integration test assertion
- [x] TS codegen + contract test
- [x] Web stream handler + Cypress
- [x] PRD + changeset + changelog

## Acceptance tests

| Test | Location |
|------|----------|
| gRPC stream receives `SessionRuntimeStatus` | `packages/tddy-service/src/integration_tests.rs` — `test_submit_feature_input_triggers_goal_started_and_mode_changed` |
| TS round-trip | `packages/tddy-rust-typescript-tests/tests/remote-session-runtime-contract.test.ts` |
| UI bar | `packages/tddy-web/cypress/component/SessionRuntimeStatusBar.cy.tsx` |
| Storybook e2e | `packages/tddy-web/cypress/e2e/session-runtime-status.cy.ts` |
| Snapshot unit | `packages/tddy-core/src/presenter/session_runtime.rs` (unit test) |

## Codegen

**tddy-web**

```bash
cd packages/tddy-web && bunx buf generate ../tddy-service/proto
```

**tddy-rust-typescript-tests**

```bash
cd packages/tddy-rust-typescript-tests && bun run generate
```

## Validation results

| Check | Result |
|-------|--------|
| `cargo clippy -p tddy-core -p tddy-service -- -D warnings` | Pass |
| `cargo test -p tddy-service integration_tests::tests::test_submit_feature_input_triggers_goal_started_and_mode_changed` | Pass |
| `cargo test -p tddy-coder --test presenter_integration` | Pass |
| `bun test tests/remote-session-runtime-contract.test.ts` | Pass |
| `bun run build` (tddy-livekit-web + tddy-web) | Pass |
| Cypress component + e2e (session runtime specs) | Pass |

**Note**: Full `bun test` in `tddy-rust-typescript-tests` includes `echo-unary.test.ts`, which spawns `tddy-coder --daemon` and may time out if the binary or web bundle is missing; run contract test in isolation in constrained environments.

## Requirement clarification (Updated: 2026-03-22)

**Product direction**: **Live** session runtime / workflow status shown in the web UI must be retrieved from the **`tddy-*` process** via **`TddyRemote`** over **gRPC / LiveKit RPC** (stream `ServerMessage`, including **`SessionRuntimeStatus`**). The UI **subscribes** to that stream and renders updates **in real time**.

- **Do not** treat the on-disk **changeset** (`changeset.yaml`) as the authoritative channel for **live** status in the browser; it remains persistence for workflow continuity and tooling, not a substitute for the remote-control event stream.
- **Implementation alignment**: `SessionRuntimeStatus` on the stream + `GhosttyTerminalLiveKit` handling matches this for the **connected** terminal status bar. **`ListSessions`** / `tddy-daemon` session listing uses **`.session.yaml` metadata only** for the status field (lifecycle); it does **not** read `changeset.yaml` for workflow display.

## References

- `packages/tddy-service/proto/tddy/v1/remote.proto`
- `packages/tddy-core/src/presenter/session_runtime.rs`
