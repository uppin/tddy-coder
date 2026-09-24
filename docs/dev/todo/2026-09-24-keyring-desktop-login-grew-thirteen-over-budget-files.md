# 2026-09-24 — `#keyring` 2/9 grew thirteen over-budget files, deferred with consent

**Category:** Deferred from `#keyring` 2/9 `desktop-login` (#509)
**Source:** `/pr-wrap` step 3.5 file-length gate on #509, range `4e7157d2..9a98292c` (merge-base with
`origin/master`), production lines counted to the first `#[cfg(test)]`
**Consent:** the developer chose **"defer all, record"**, 2026-09-24

#509 grew thirteen non-test source files that end at or over the 500-production-line budget —
twelve already over it, and `auth.rs`, which it pushed across. None is decomposed in #509. **The follow-up runs after the
`#keyring` stack lands** (#510–#516 still open on 2026-09-24).

| File | Production lines (before → after) | Why deferred |
|---|---|---|
| `packages/tddy-daemon-auth/src/auth.rs` | **499 → 596** (crossed the budget) | touched by #510, #511; #509's `## Boundaries` rules out splitting it in this stack |
| `packages/tddy-daemon/src/runtime.rs` | 1,562 → 1,619 | touched by #510, #511, #512 |
| `packages/tddy-coder/src/run.rs` | 2,682 → 2,715 | touched by #510, #511 |
| `packages/tddy-daemon-kernel/src/config.rs` | 1,470 → 1,472 | touched by #510–#513; #509's `## Boundaries` rules out splitting it in this stack |
| `packages/tddy-session-tool-client/src/lib.rs` | 1,043 → 1,047¹ | +4 from transport stamping only |
| `packages/tddy-daemon-sandbox/src/sandbox_session.rs` | 911 → 916 | +5 from transport stamping only |
| `packages/tddy-supervisor/src/server.rs` | 703 → 708 | +5 from transport stamping only |
| `packages/tddy-toolcall/src/toolcall/listener.rs` | 671 → 676 | +5 from transport stamping only |
| `packages/tddy-sandbox-runner/src/runner.rs` | 2,650 → 2,654 | +4 from transport stamping only |
| `packages/tddy-codegen/src/generator.rs` | 1,116 → 1,119 | +3 from transport stamping only |
| `packages/tddy-livekit/src/participant.rs` | 954 → 956 | +2 from transport stamping only |
| `packages/tddy-sandbox-app/src/sandboxed_session.rs` | 735 → 736 | +1 from transport stamping only |
| `packages/tddy-session-lifecycle/src/cli_session_manager.rs` | 1,371 → 1,372 | +1 (the bidi `metadata` parameter) only |

"Transport stamping" is #509's V1 fix: every serving host names the `tddy_rpc::RequestTransport` its
requests arrive on (`ServerEngine::new(service, transport)`, `StdioEndpoint::from_duplex(.., transport)`),
so enrolment can tell the desktop's own window from any other caller. The +1..+5 growth is that
argument at each host, not new responsibility, which is why decomposing those nine files for it was
judged not worth the churn inside the stack.

Existing `oversized-file` records carry the standing measurements for `run.rs`, `runtime.rs`,
`config.rs`, `sandbox_session.rs`, `listener.rs` and session-tool-client `lib.rs`; the other
files have none yet.

## What would close it

After `#keyring` lands, decompose each file with the `code-restructuring` skill
(`restructure check --deep`, then apply) under a green baseline, starting with the four the stack
blocked — `auth.rs`, `runtime.rs`, `run.rs`, `config.rs` — whose growth is responsibility, not
plumbing. Delete this entry when every row is under budget or has its own `oversized-file` record
carrying the follow-up.

## A separate finding: the gate undercounts `cfg(any(feature, test))`

¹ The gate's pattern `/^[[:space:]]*#\[cfg\(.*test[),]/` also stops at
`#[cfg(any(feature = "livekit", test))]` (`packages/tddy-session-tool-client/src/lib.rs:603`), which
guards **one production function**, and reports 602 → 602. The real `#[cfg(test)] mod tests` is at
`:1048`. It was the only file in #509's range the pattern undercounts, re-measured by hand with a
`#[cfg(test)]`-only stop.

This is a second instance of the defect in
[2026-09-19-the-file-length-gate-stops-at-the-first-cfg-test-use.md](2026-09-19-the-file-length-gate-stops-at-the-first-cfg-test-use.md)
(a production-guarding or import-only `cfg` item cutting the count short), and the fix recorded there
— exclude `#[cfg(test)]` items one by one, syntax-aware — closes both. Not fixed in #509
(`.agents/commands/pr-wrap.md` is unchanged), by the developer's instruction.
