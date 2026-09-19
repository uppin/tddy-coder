# 2026-09-19 — Every test binary moves to the crate it actually exercises

**Type:** Refactor

`#carve` 4/10, PR [#498](https://github.com/uppin/tddy-coder/pull/498).

`tddy-daemon` carried **140** test binaries over 2,377 production lines, and 119 of them never
reached this crate's code. They got there through a facade: `src/lib.rs` re-exported **82** modules
from `tddy-session-lifecycle`, which re-exported 49 of those from ten further crates, under the
comment *"Legacy paths for integration suites"*. So an import proved nothing about what a suite
exercised — `tddy_daemon::host_registry` is `tddy-host-service`'s — and
`tddy-session-lifecycle`, 31,700 production lines, had **no `tests/` directory at all** while its
own acceptance coverage sat in another crate.

`tddy-code-restructuring` gains **`move_test_binary_to_crate`**, and **120** suites move with it:
119 out of `tddy-daemon` and one out of `tddy-workflow-recipes`, plus `tests/common/mod.rs`, which
is a module rather than a binary. The facade and three re-export shims go, and with them the
dependencies whose only consumers were the departing tests. Per-package detail is in each package's
own entry.

| Destination | Suites |
|---|---:|
| `tddy-session-lifecycle` | 95 (96 renames, counting `tests/common/mod.rs`) |
| `tddy-worktree-service` | 7 |
| `tddy-daemon-livekit` | 6 |
| `tddy-projects`, `tddy-tool-engine`, `tddy-vm` | 2 each |
| `tddy-core`, `tddy-daemon-auth`, `tddy-daemon-kernel`, `tddy-daemon-sandbox`, `tddy-session-agents` | 1 each |

**21 originals stay** in `tddy-daemon`, and `test_placement.rs` — added here as the 141st — asserts
the rule that keeps it that way: a suite belongs there when it names a module the crate defines, or
when it asserts about the package's own `src/` or `Cargo.toml`. The second kind cannot move at all,
because `CARGO_MANIFEST_DIR` would follow it and the assertion would go on passing while silently
being about something else. Four suites the plan had counted as strays are that shape and were
renamed into the keep list rather than moved.

## What the operation had to learn that a module move never needed

A test binary is a **different shape**, and simpler in two ways: cargo auto-discovers `tests/*.rs`,
so there is no `mod` line to remove and no origin edit at all; and nothing can reference a test
binary, so `reexport` is refused rather than ignored. What it is not simpler about is re-pointing.
A moved module keeps its own `crate::` and its bodies mean the same thing afterwards; a test binary
leaves its crate entirely, so **every** occurrence of the origin's extern name is re-pointed, not
just the `use` header — 110 such paths in bodies, and one `use` indented inside a `mod tests { … }`,
survived a header-only pass. The destination gains `[dev-dependencies]`, not `[dependencies]`, and
there is deliberately no cycle refusal: cargo permits a dev-dependency cycle because it never enters
the library's build.

Two corrections to the tooling came out of running it for real, both recorded because the nodes
above build on the same code:

- **The dependency question has to be asked of the whole file, not the header.** One suite named
  `tddy_github` in a return type, `tddy_connectrpc` in a `let` and `axum` in a statement while
  declaring none of the three; read from the header, the destination gained a line for none of them
  and the moved suite failed with `E0433`. A name is taken as a crate only when it opens a path, is
  not bound by the file, is not a built-in root, and **the origin's manifest declares it** — the
  last is what makes "no line to carry across" a refusal rather than a guess.
- **Comments are re-pointed, strings never are.** A comment naming a crate the file no longer uses
  is exactly the debt this operation pays off; a string literal is data the suite asserts on, and
  rewriting it would change the assertion. The cost is that a `CARGO_MANIFEST_DIR`-relative string
  path to a sibling crate's source is a hand edit after the move — two suites needed one.

## A departure from the boundary contract, with consent

The node's `## Dependencies` said it would **not** touch `module_home` / `unrunnable_moves`. It
changed `module_home::defining_crate`, and the developer authorised that on 2026-09-19 after the
alternative was put to them. **Nodes 5–10 build on that symbol and should know it moved under them.**

`defining_crate` could not resolve either facade in this workspace, so the header pass did nothing
at all: the first real run moved 31 suites that each kept `use tddy_daemon::…` and each gave its new
crate a `tddy-daemon` dev-dependency — a leaf crate depending back on the daemon, which is the
precise failure the operation exists to prevent. All 31 were reverted. Two defects, both in
`module_home.rs`:

1. `re_export_target` matched within a single line, and every facade here is a multi-line braced
   group whose crate-naming line — `pub use tddy_session_lifecycle::{` — carries no member at all.
   A group of forty re-exports read as a re-export of nothing.
2. `defining_module_in_crate` read only `<crate>/src/lib.rs`, so a `pub mod config;` whose
   `src/config.rs` is nothing but `pub use tddy_daemon_kernel::config::*;` read as locally defined.

Node 1/10 had already merged, so there was no parent branch left to carry the fix, and
re-implementing facade resolution inside this node is what that `## Dependencies` row exists to
forbid. `module_home()` and `defining_crate()` keep their signatures, every change is in private
helpers, and every existing `module_home` test stayed green.

## Closing measurements

Re-measured on 2026-09-19 from the tree. These are the numbers the deleted `code-issues` records
carried, recorded here so they survive the deletion; all three measured clean, so none is a partial
fix being closed.

| Record | First detection (2026-09-15) | At wrap (2026-09-19) |
|---|---|---|
| `tddy-daemon` · `heavy-dependency-tests-only-runtime-deps.md` | 17 `tddy-*` runtime dependencies named by no file in `src/` | **0** of 27 |
| `tddy-daemon` · `misplaced-tests-integration-suites.md` | 122 of 139 suites never reach this crate's production code | **0** of 22 |
| `tddy-session-lifecycle` · `missing-tests-crate-has-no-test-directory.md` | **0** test binaries; 97 suites / 38,629 lines living in `tddy-daemon` | **95** test binaries and a `tests/common/`, in the crate |

`tddy-daemon/src/lib.rs` carries **0** `pub use tddy_session_lifecycle::` lines, down from two
blocks re-exporting 82 modules. `src/config.rs` stays: it forwards to `tddy-daemon-kernel`, which
owns the configuration the crate loads.

CI on `5e91f2b2`: **7,006 Rust tests passed, 2,637 web tests passed, 0 failed**, both build arches,
lint, generated-code and every VM check green. That whole-workspace count is the load-bearing
evidence — a relocation that silently dropped a suite, by landing it in a crate whose
`[dev-dependencies]` cannot build it, looks identical to a clean move from any scoped run.

## Resolved backlog entries

- `2026-09-15-122-of-tddy-daemons-139-test-suites-belong-to-other-crates.md` — this node was that
  entry; 0 misplaced suites remain of 22.
- `2026-09-15-stack-progress-contract-acceptance-tests-only-tddy-core.md` — the one misplaced
  `tddy-workflow-recipes` suite is now `packages/tddy-core/tests/stack_progress_contract_acceptance.rs`.

`2026-09-09-tddy-daemon-untested-complexity-hotspots.md` stays open and was **annotated, not
claimed**: its by-file coverage figures were stated against `tddy-daemon` and those files now sit in
other crates.

## Deferred, with consent

Three files sit at or over the 500-production-line budget, all deferred on 2026-09-19 with the
developer's explicit consent and each carrying a `code-issues` record rather than a mention:
`crate_move/test_binary.rs` (0 → **966**, created here at 1.9× budget, with its own backlog entry
`2026-09-19-test-binary-rs-is-950-production-lines.md`); `backends/rust.rs` (4,772 → **4,788**,
where #491 is also in flight, so the stack-overlap stop applies); and `tddy-daemon/src/runtime.rs`
(1,420 → **1,423** reflowed lines on a composition root already 2.8× budget).
