# Changeset: Snapshots for jailed terminals

**Created:** 2026-10-25
**Status:** 🚧 In Progress
**Type:** Feature
**Complexity**: 5 — three crates, a stdio-bridge contract change, ordering semantics (size change must sit at the right point in the output stream)
**Stack:** `#term-snapshot 3/4` — branch `feature/term-snapshot/jail-size`, base `feature/term-snapshot/resync`
**PR:** [#762](https://github.com/uppin/tddy-coder/pull/762)
**PRD:** [PRD-2026-10-25-terminal-snapshot-jail.md](../../ft/daemon/1-WIP/PRD-2026-10-25-terminal-snapshot-jail.md)

## Initial Discovery

[2026-10-25-term-snapshot-jail-size-initial-discovery.md](./2026-10-25-term-snapshot-jail-size-initial-discovery.md)

## Prerequisites

| Record | Verdict |
|---|---|
| `packages/tddy-daemon-sandbox/docs/code-issues/heavy-params-sandbox-session-dial-and-bridge.md` | ⚠ during: the size message is consumed in the same dial-and-bridge path. Do not add parameters, and carry the size on the existing relay. Deferral allowed (developer consent) |

## Affected Packages

- [ ] `tddy-sandbox-runner`: report the applied PTY size at spawn and after each resize
- [ ] `tddy-daemon-sandbox`: consume the size message and resize the terminal's capture/model in order with output
- [ ] `tddy-session-lifecycle`: the sandbox adapter stops refusing `SNAPSHOT`

## Contract (pass 2, approved 2026-10-25)

- **Opt-in event channel.** `HostRelayConfig::with_terminal_events(tx)` delivers
  `HostTerminalEvent::{Output, Resized}` in order *instead of* bytes on `terminal_sink`. The 11
  byte-only consumers (sandbox-app, sandbox-qemu, testing-commons, integration-tests, runner tests)
  are untouched. Only the daemon's `dial_and_bridge` switches to it, and to
  `forward_jail_terminal_events`, in green.
- Proto: `SessionFrame.terminal_resized = 18` / `SandboxTerminalResized { cols, rows, session_id,
  terminal_id }`. Regenerated `sandbox_pb.ts` in tddy-web and tddy-rust-typescript-tests.
- Runner: `SandboxSessionRelay::push_terminal_size` (stub). The PTY threads (generic, claude,
  cursor) call it after `openpty` and after each applied `master.resize`, and its
  `cfg_attr(not(test), allow(dead_code))` goes once they do.
- Daemon: `sandbox_session::forward_jail_terminal_events` (output path real, `Resized` stubbed).
- The `session-lifecycle` parity extension is dropped: a sized capture answers through #760's
  default `snapshot_and_subscribe`, which #760's tests cover.
- Dev-dependency `vt100 = "0.15"` in tddy-daemon-sandbox (already in the lockfile).

| Test | File | Fails today because |
|---|---|---|
| `a_reported_size_reaches_the_host_in_order_with_the_output_around_it` | `packages/tddy-sandbox-runner/src/runner.rs` tests | `push_terminal_size` stub |
| `the_host_relay_reports_the_jails_output_and_size_changes_in_order` | `packages/tddy-sandbox-runner/tests/host_relay_terminal_events.rs` | relay does not route to `terminal_events` |
| `a_jailed_terminal_snapshots_at_the_size_the_jail_reported` | `packages/tddy-daemon-sandbox/tests/jail_terminal_snapshot_acceptance.rs` | `Resized` stub, plus #760's `snapshot()` |
| `a_jailed_terminal_that_never_reported_its_size_has_no_snapshot` | same | passes today, as a guard |

## Responsibility

- A **terminal-size message** on the runner → daemon stdio relay, carrying `(cols, rows)`. The
  runner sends it after `openpty` (spawn size) and after every successful `master.resize`, on the
  same ordered channel as terminal output, so the size change sits at the right output offset.
- In the daemon's sandbox stdio loop, the message calls `TerminalCapture::resize` under the capture
  lock (the same lock the output append and broadcast hold since #760).
- The sandbox `TerminalSession` adapter uses the default `snapshot_and_subscribe` instead of refusing.
  A jailed terminal that has not reported its size yet refuses with `FAILED_PRECONDITION`, so the
  model is never serialized at a guessed size.

## Boundaries

- Who resizes a jailed PTY does not change: the runner still applies the in-band resize OSC.
- No change to the bridge, the proto, the web client or the room wire.

## Dependencies

What each parent PR delivers that this PR consumes. These surfaces are **theirs to create**;
implementing one here collides with the PR that owns it.

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `#term-snapshot 1/4` host-snapshot (#760) | `TerminalCapture::resize`, `snapshot_and_subscribe` default, broadcast-under-lock in the sandbox stdio loop, the sandbox adapter's `FAILED_PRECONDITION` override | calls `resize` on the size message; removes the override | change the capture, the port or the bridge |
| `#term-snapshot 2/4` resync (#761) | nothing this node consumes; it sits below this node only because `gh stack` needs a line | — | — |

## Draft PR contract

The first push (pass 2) publishes the size-message variant on the stdio relay (runner and daemon
sides, with handlers stubbed `// TODO(jail-size): implement`) and the failing tests below.

## Green wave

**Wave:** 2 of 3
**Greenable independently:** no. The tests serialize snapshots through #760's capture (behavior).
**Concurrent with:** `#term-snapshot 2/4` resync
**Blocks:** none

    n1 → n2, n3, n4      n2 → n4

## State A (after #760)

The daemon's model for a jailed terminal has no size, so `SNAPSHOT` is refused with
`FAILED_PRECONDITION`. Resizes happen only inside the jail.

## State B

The runner reports the applied size and the daemon's model follows it at the right point in the
output stream, so jailed terminals snapshot like every other terminal.

## Delta
### New
- The terminal-size relay message (runner → daemon)
### Modified
- Runner PTY threads (generic, claude, cursor): send the size after `openpty` and after `resize`
- `sandbox_session.rs` stdio loop: handle the size message under the capture lock
- Sandbox adapter: no `SNAPSHOT` refusal once a size is known
### Removed
- The unconditional `FAILED_PRECONDITION` for jailed `SNAPSHOT`

## Testing Strategy

### Acceptance tests
- [ ] **Integration** `packages/tddy-daemon-sandbox/tests/sandbox_terminal_snapshot_acceptance.rs`
  - `a_jailed_terminal_snapshots_at_the_size_the_runner_reported`: an in-process relay double
    reports a spawn size of 100×30, sends output, reports a resize to 140×40, and sends more output.
    A `SNAPSHOT` open yields `snapshot_rows/cols == 40/140`, and the reference emulator equals the
    jail-side reference fed the same stream with the same resizes.
  - `a_jailed_terminal_without_a_reported_size_refuses_snapshot`.
- [ ] **Unit** in `packages/tddy-sandbox-runner/src/runner.rs` tests: `a_resize_escape_reports_the_applied_size`
  (after `strip_resize_escape` + resize, the relay received the size message, ordered after the
  output preceding it).
- [ ] **Integration** `packages/tddy-session-lifecycle/tests/sandbox_terminal_parity_acceptance.rs`:
  extend it with `a_jailed_terminal_answers_snapshot_like_a_cli_terminal`.

### Test level decisions
| Aspect | Level | Rationale |
|---|---|---|
| Size ordering vs output | Integration with a relay double | The guarantee is ordering across the relay |
| Runner reporting | Unit | Pure function of the stdin parse |

## Decisions & Trade-offs
- The runner reports the **applied** size (PRD), rather than the daemon parsing the forwarded OSC,
  so a failed resize in the jail is never mirrored as a size change.

## TODO

- [x] Record initial discovery (`2026-10-25-term-snapshot-jail-size-initial-discovery.md`)
- [x] Cross-check `packages/*/docs/code-issues/` and `docs/dev/todo/` for items this change touches (Step 2b)
- [x] Create/update PRD documentation
- [x] Create changeset (this document)
- [x] Create failing acceptance tests
- [x] Run acceptance tests (verify they fail)
- [x] USER REVIEW — acceptance tests
- [x] TDD Red — write failing unit/integration tests
- [ ] TDD Green — implement with quality code
- [ ] Update documentation with progress
- [ ] Repeat Red→Green→Update cycle until feature complete
- [ ] Run the touched packages' tests (`./test -p {package}` per package) — verify 100% pass
- [ ] Validate changes (/validate-changes)
- [ ] Refactor issues from change validation
- [ ] USER REVIEW — development complete
- [ ] Validate tests (/validate-tests)
- [ ] Refactor test issues
- [ ] Validate production readiness (/validate-prod-ready)
- [ ] Refactor production readiness issues
- [ ] Analyze code quality (/analyze-clean-code)
- [ ] Refactor code quality issues
- [ ] Final validation (/validate-changes)
- [ ] Linting and formatting (`cargo clippy -p {package} -- -D warnings`, `cargo fmt`)
- [ ] USER REVIEW — scope developed, every gap approved: ready to wrap

## Refactoring Needed
### From /red (TDD red phase)
### From /validate-changes
### From /validate-tests
### From /validate-prod-ready
### From /analyze-clean-code

## Validation Results
| Validation | Last Run | Status |
|------------|----------|--------|
| /validate-changes | Not yet run | Pending |
| /validate-tests | Not yet run | Pending |
| /validate-prod-ready | Not yet run | Pending |
| /analyze-clean-code | Not yet run | Pending |
