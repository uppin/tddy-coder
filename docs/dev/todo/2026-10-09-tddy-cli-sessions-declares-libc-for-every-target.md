# 2026-10-09 — `tddy-cli-sessions` declares `libc` for every target

**Category:** Hygiene (a hand fix after an engine move widened a dependency's platform scope)
**Source:** #reshape 9/19 (`feature/reshape/new-crate`) discovery; the hand fix is from #carve 21/21 (PR #536), R7

`packages/tddy-session-lifecycle/Cargo.toml` declares `libc = "0.2"` only under `[target.'cfg(unix)'.dependencies]`. When
`cli_session_manager` moved to `tddy-cli-sessions`, the engine missed `libc`: the target table was never read, which
`#reshape` 9 fixes. The hand fix then added `libc = "0.2"` to `packages/tddy-cli-sessions/Cargo.toml` under plain
`[dependencies]`, so the crate now depends on `libc` on every target.

Today this is harmless, because `libc` builds everywhere and the code that uses it (`cli_session_manager.rs`, `libc::kill`)
is unix-only in practice. Still, the manifest no longer says what the origin said.

**Why deferred.** It is outside `tddy-code-restructuring`, and `#reshape` 9 changes only the engine, not manifests that
earlier hand fixes wrote.

**What would close it.** Move the line to `[target.'cfg(unix)'.dependencies]` in `packages/tddy-cli-sessions/Cargo.toml`,
run `cargo check -p tddy-cli-sessions --all-targets`, and check whether any non-`cfg(unix)` code names `libc` (if so, keep
it plain and delete this file). Check `tddy-lsp-executor` for the same pattern from #live-plan 12/15's hand-carried
`libc`. It declares none today.
