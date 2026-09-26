# 2026-09-26 — Holds relay_idle and local_token_tonic_adapter

**Type:** Refactor

`#carve` 15/21 ([#526](https://github.com/uppin/tddy-coder/pull/526)); cross-package entry:
[2026-09-26-carve-lifecycle-leaf-moves.md](../../../../docs/dev/changesets/2026-09-26-carve-lifecycle-leaf-moves.md).

`relay_idle` (`RpcActivity`) and `local_token_tonic_adapter` (`LocalTokenUdsTonicAdapter`) moved
here from `tddy-session-lifecycle`, which re-exports both. New dependencies `tddy-task`,
`tddy-github` and `tonic`; no cycle. Production lines 3,290 → 3,409. Neither file had tests of its
own. The admission rule in [daemon-kernel.md](../daemon-kernel.md) names them beside `config.rs` as
whole modules held here.

Code issues: `misplaced-tests-privilege-drop` and `missing-tests-privilege-drop-resolve-pty-os-user`
record that `privilege_drop`'s tests and one call site moved with `pty_runtime` to
`tddy-terminal-rpc` (still another crate; still 0 tests on `resolve_pty_os_user`);
`heavy-dependency-livekit-peer-forwarding` re-measured at 1 SDK module of 15 and 17 dependents.
`oversized-file-config` was not touched.
