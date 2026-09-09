# 2026-09-09 — `connection_service.rs` 24,858 ➜ 18,300 lines: tests to their own files

**Type:** Refactor

The 29 inline `#[cfg(test)] mod …` blocks are now one file each under
`src/connection_service/`, declared from the parent as `#[cfg(test)] mod <name>;`. 6,529 lines of
tests left the module; nothing else moved.

**No caller anywhere was rewritten.** 93 files outside the module reach 17 of its symbols —
`ConnectionServiceImpl` alone accounts for 62 of those references — and none of them appears in the
diff: the operation relocates a whole module, so every path resolves exactly as before, and each test
file reaches the code under test through `use super::*`, which a child module resolves against its
parent unchanged.

Behaviour unchanged, and the evidence is a statement-level comparison as well as the suites. Normalise
whitespace and `pub(crate)` away and set-compare every line against the pre-change tree: 29 `mod X {`
and their 29 closing braces lost, 29 `mod X;` gained, **nothing else** — distinct non-blank lines
identical at 12,249 on both sides, and zero visibility widenings, because relocating a whole module
crosses no visibility boundary. `cargo test -p tddy-daemon --lib` 809 passed / 0 failed before and
after; CI's full run 6,335 passed / 0 failed.

One consequence worth knowing before adding a test: a facade re-exports a module's **items**, not its
imports, so a name the parent only *imports* is not reachable from a child through `use super::*`.
Bind it in the test file, or under `#[cfg(test)]` in the parent where several test modules need it.

Every moved character came from rust-analyzer; the only hand-written lines were import
disambiguations and a `cargo fmt --all` pass — extractions relocate bodies to a new indentation, and a
body correctly wrapped at one indentation is not correctly wrapped at another.

Dev doc: [connection-service.md](../connection-service.md) § Where the code lives.
