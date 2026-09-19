# 2026-09-19 — The endpoint keeps 22 suites, and the facade that held the rest is gone

**Type:** Refactor

`#carve` 4/10 ([#498](https://github.com/uppin/tddy-coder/pull/498)). Cross-package entry:
[2026-09-19-carve-test-homes.md](../../../../docs/dev/changesets/2026-09-19-carve-test-homes.md).

`tests/` held **140** binaries over 2,377 production lines, and **119** of them never reached this
crate's code. They resolved through `src/lib.rs`'s two `pub use` blocks — 82 `tddy-session-lifecycle`
modules, 49 of them re-exported again from ten further crates, under the comment *"Legacy paths for
integration suites"*. Those blocks and three re-export shims (`relay_idle`, `tddy_user_config`,
`user_sessions_path`) are deleted; `src/config.rs` stays, because it forwards to
`tddy-daemon-kernel`, which owns the configuration this crate loads. The daemon's own `src/` named
two of the deleted shims — `crate::relay_idle` twice, `crate::user_sessions_path` six times — so
those callers were re-pointed too; deleting a shim without them breaks the crate, not just its
tests.

**17 `tddy-*` crates were declared in `[dependencies]`, not `[dev-dependencies]`, and named by no
file in `src/`** — so every consumer of this crate rebuilt them for the sake of tests that are no
longer here. Now **0 of 27**.

**21 originals stay, plus `test_placement.rs`**, which is added here as the 141st binary and asserts
the placement rule, the absent facade, the absent shims and the dependency measurement. The rule is
in [test-placement.md](../test-placement.md): a suite belongs here when it names a module this crate
defines, or when it asserts about this package's own `src/` or `Cargo.toml` — the second kind cannot
move at all, since `CARGO_MANIFEST_DIR` would follow it and the assertion would go on passing while
being about something else. Four suites the plan had counted as strays are exactly that shape
(`index_daemon_lifecycle_acceptance.rs`, `local_socket_reachability_acceptance.rs`,
`unbundle_endpoint.rs`, `unbundle_tools_dependency_dropped.rs`) and stayed.

No assertion changed anywhere. The relocated diffs are crate-path rewrites and rustfmt reflow of
`use` groups, and CI on `5e91f2b2` carried 7,006 Rust tests passing with 0 failed — the
whole-workspace count is the only check that would catch a suite silently dropped by the move.

**Unchanged and deferred:** `src/runtime.rs` gained **3** reflowed lines from the shim re-points
(1,420 → 1,423, already 2.8× the file budget). See
[`oversized-file-runtime.md`](../code-issues/oversized-file-runtime.md).
