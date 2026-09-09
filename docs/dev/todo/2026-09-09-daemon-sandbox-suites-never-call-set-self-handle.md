# 2026-09-09 — 17 daemon tests panic in `self_arc` because their harness never wires the handle

**Category:** Known failing test
**Source:** `analyze-coverage-export-and-harness-selection` (#466), `tddy-daemon` baseline

With every sibling binary built, `cargo test -p tddy-daemon --no-fail-fast` is **1,993 passed / 18
failed / 3 ignored**. Seventeen of the eighteen are one root cause:

```
ConnectionServiceImpl::self_arc called before set_self_handle
  packages/tddy-daemon/src/connection_service.rs:1861
```

The doc comment there states the invariant as *"tests that do not exercise the sandbox-IPC RPC
bridge never call this"* — but the failing tests **are** the sandbox suites
(`sandboxed_claude_cli_*`, `sandboxed_cursor_cli_*`, `sandboxed_session_*`,
`cursor_cli_sandbox_start_*`, `resume_sandbox_session_*`, `delete_sandbox_session_*`), and their
builders construct the service without ever calling `set_self_handle`. So the comment describes an
invariant the harness violates rather than one it upholds: `main.rs` wires it at startup and the
tests do not. One fix, seventeen tests.

This is long-standing — it matches the pre-existing `sandbox_behavior_acceptance` failures already
on record for `master` and is not a regression from any sandbox change.

The eighteenth failure is unrelated and environmental: `restores_a_mirror_that_was_corrupted_by_hand`
fails when the LiveKit testkit container cannot bind its host port
(`failed to bind host port 0.0.0.0:53206/tcp: address already in use`). `./run-livekit-testkit-server`
exists precisely to reuse one container instead of starting one per run; the suite does not use it.

Left alone in #466 because that branch touches no daemon code, and a red baseline was recorded
rather than repaired so the analysis results could be read against a known state.
