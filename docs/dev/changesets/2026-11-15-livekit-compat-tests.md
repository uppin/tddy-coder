# 2026-11-15 — LiveKit contract suites move into `tddy-livekit-compat-tests`, a separate PR check

**Type:** Refactor

24 LiveKit connectivity/contract suites moved out of `tddy-livekit` (9), `tddy-daemon-livekit`
(9), `tddy-e2e` (4) and `tddy-coder-cli` (2) into a new crate, `tddy-livekit-compat-tests`, with
`move_test_binary_to_crate` — one plan per suite, each deep-checked and applied with its own
compile gate. Test names are unchanged; only the owning package changed. The origin crates dropped
their now-unused `tddy-livekit-testkit` dev-dependencies.

The package holds no product logic, and no logic suite moved into it. `livekit_test_placement.rs`
pins the split and names the 15 logic suites still running over a real room in the crates that own
them (`tddy-daemon` 11, `tddy-session-lifecycle` 2, `tddy-daemon-rpc` 1, `tddy-worktree-service`
1); converting those onto in-memory/IPC transports is the remaining half of
`docs/dev/todo/2026-11-05-tests-use-cheap-rpc-livekit-only-for-compatibility.md`, which is narrowed
accordingly.

CI: `Rust LiveKit e2e tests` is a new, separate check running
`.config/rust-livekit-e2e.filterset` (`tddy-livekit-compat-tests` plus the testkit's own suites)
against one shared LiveKit server. The moved binaries left `.config/rust-e2e.filterset`, the
`Rust tests` leg now excludes both e2e sets, and the `ci` nextest profile's LiveKit
`slow-timeout` override names the new package instead of the crates the suites left.

Per-package detail is in each package's own entry under
`packages/<pkg>/docs/changesets/2026-11-15-livekit-compat-tests.md`.
