# 2026-09-23 — The restructure engine stops reporting success over results it cannot vouch for

**Type:** Fix

`#carve` 13/15, PR [#527](https://github.com/uppin/tddy-coder/pull/527), between `core-split`
([#522](https://github.com/uppin/tddy-coder/pull/522)) and `lifecycle-wiring`
([#524](https://github.com/uppin/tddy-coder/pull/524), the destructure node, which runs its plans
against this engine).

Packages: `tddy-code-restructuring`, `tddy-index-daemon`, `tddy-lsp`; root script `run-index-daemon`;
`.config/nextest.toml` (six new live suites join the `rust-analyzer` test group). `tddy-tools` is
unchanged: it renders the new refusals through the library's existing error path.

Current behaviour is documented in:

- [`tddy-code-restructuring/README.md`](../../../packages/tddy-code-restructuring/README.md),
  [`docs/assist-output-repairs.md`](../../../packages/tddy-code-restructuring/docs/assist-output-repairs.md)
  (import pass, `impl` seams, nested-module repair) and
  [`docs/readiness-and-gates.md`](../../../packages/tddy-code-restructuring/docs/readiness-and-gates.md)
  (readiness, health gate, early-return refusal, compile gate)
- [`tddy-index-daemon/docs/code-index-service.md`](../../../packages/tddy-index-daemon/docs/code-index-service.md)
- [Rust code restructuring](../../ft/coder/rust-code-restructuring.md),
  [Warm code-intelligence daemon](../../ft/coder/warm-code-intelligence-daemon.md),
  [Reusable LSP](../../ft/coder/reusable-lsp.md)
- `.agents/skills/code-restructuring/SKILL.md` and `references/plan-schema.md` (extract-method
  ordering, the early-return rule, inherent versus trait `impl` cuts, the `modname::` repairs)

## Why

Planning the destructure node ran 14 plans through `restructure check --deep` against a warm index,
on `tddy-session-lifecycle`. Eight were clean and six were refused by three engine defects. The
developer chose to fix the engine rather than hand-split the refused seams (2026-09-23), so the
destructure stays engine-driven and the fix outlives it, and placed the fix inside the stack directly
below #524 so #524 gets it without waiting for `master`.

## What was fixed

| | Defect | Cause | Fix |
|---|---|---|---|
| **E1** | The import pass looped and wrote the same `use` 512 times, in any file with a `use … as …` alias, even for a 24-line seam with no free names | `already_bound` read `use a::B as C;` as binding `B`, so the alias reconstruction never saw its own insertion; the trigger was a name unresolved everywhere, collected file-wide | `names_bound` reads what a `use` binds; both reconstructions are verified like an offered import and refused by name when they make no progress; only the names **the seam lost** are weighed |
| **E2** | Extract-method wrote `req: _` for `OUT_DIR` types (`StartSessionRequest`) | In fixtures: readiness accepted the first hover, before build scripts loaded. On the real repo: a degraded index (below) | Readiness waits for `serverStatus` quiescence; the health gate refuses a degraded index; `refuse_inferred_placeholder` stays the post-condition |
| **E3** | A seam cutting an inherent `impl` was refused because `self.method()` calls "would resolve nowhere" | Method calls resolve through the type | Only trait-`impl` cuts (and unnamed `impl`s) stay refused; the assist's `self.modname::method()` rewrite is undone |
| **Grouped `use`** | `tokio::sync::{…, mpsc, …}` offered two ways | The assist removed `mpsc` from the group before `choose_import` read the text | The choice reads the pre-assist file with the current text |
| **Gap A** | A reconstructed relative `use` was one level off (plan 05a) | The parent's declaration was written verbatim into its child | `rebased_for_child`: `super::X` → `super::super::X`, `self::X` → `super::X` |
| **Gap B** | `Self::modname::f` / `Type::modname::f` left behind (plan 02 op 3) | The assist inserts the placeholder before a moved associated function in every call form | `with_method_calls_restored` widened (`reached_through_the_type`) |
| **Gap C** | `modname::f()` inside the file's existing `mod tests` (plan 04 op 0) | The assist rewrote the call as well as the import, and `modname` is not `tests`' child | `with_nested_references_restored` (new `nested_modules.rs`); the leftover refusal names both module cases |

**E1, deviation from the plan.** The plan said to collect unresolved names from the produced module
only. A live probe showed that the file-wide pass also restores names the parent loses to the cut (a
trait moved while the parent still writes `impl Named for Thing`), so module-only collection reported
success over a parent that did not compile. The pass weighs the seam-lost set instead, pinned by
`imports_into_the_parent_a_trait_the_seam_moved_out_from_under_it`. D8's alias reconstruction (the
2026-09-09 connection-service split backlog entry) is kept and now verified; its guard test,
`imports_the_alias_the_moved_code_names_exactly_once`, passes.

## Three explicit-failure guards (developer-approved 2026-09-24)

"We must not have any implicit failures." Each turns a silent success into a truthful failure:

| Guard | Replaces |
|---|---|
| **Health gate**: an index whose `health` is not `ok` (including `warning`) is refused as `ServerDefect`, quoting rust-analyzer's message; `tddy-lsp` keeps the latest status (`LspClient::server_status`) so the gate holds on a warm, already-drained client | a run over answers from an index whose build scripts failed |
| **E4**: an `extract_method` whose range holds a `return` exiting the enclosing function is refused as `SeamRefused`, naming the lines, in `check`, `check --deep` and `apply` | plan 10: "applied 6 of 6", then seven `E0308`s |
| **Compile gate**: every writing `apply` is bracketed by `cargo check --all-targets` over the touched packages, on the CLI and the daemon path; two new variants, `BaselineDoesNotCompile` (`FailedPrecondition`) and `AppliedTreeDoesNotCompile` (`Internal`) | "applied N of N" over a tree that does not compile |

No opt-out flag exists for any of them; none was consented to.

## E2 on the real repo: not reproduced, now caught explicitly

Investigated by the developer on 2026-09-24. The readiness defect is real and fixed (the E2 fixture
proves it) but is **not** shown to be the cause of the real-repo failure. Plans `05` (shifted), `10`
and `10a` produce real types (`req: &StartSessionRequest`) on the current tree with **both** the
pre-fix engine (`origin/master`) and the fixed one, cold and against the warm index daemon.

The real cause was found in the daemon's environment. `run-index-daemon` passed rust-analyzer only
the dev shell's `PATH`; without the shell's `NIX_*` linker flags and SDK, the `webrtc-sys` build
script and the `sqlx-macros` proc macro failed to link inside rust-analyzer while `cargo check`
succeeded in the dev shell. rust-analyzer reported that as `warning` ("Failed to run build scripts of
some packages") and answered without the code it could not build. The engine ignored that account.
The script now launches the daemon with the dev shell's whole environment and a durable `TMPDIR`
(nix deletes its own when the shell exits, and rust-analyzer copies `Cargo.lock` there for every
build-script run), and truncates the log before launch so readiness is never read from the previous
daemon's line. The health gate refuses what is left.

## Real-repo results

**`check --deep`, plan 10** (fixed engine, cold, daemon stopped): ops 1–5 refused by E4, naming lines
569/598, 479/507, 289/371/404/407/426, 239 and 104/133, each checked against the source as a plain
early exit of `start_session_core`. Op 0 (776–893, no `return`) resolved clean against an index whose
health was `ok`.

**`check --deep` after gaps A–C**, against an index daemon rebuilt from this tree:
`05a-spawn-split-agent-teardown`, `04-split-session` (2 ops) and `02-cli-session-manager-dir`
(9 ops): no findings. A daemon built with diagnostics only had refused 04 op 0 and 02 op 3 exactly as
#524 saw them. 05a was already clean on that build, so its original trigger is not confirmed.

**`apply` of #524's plans** (2026-09-24, a local branch cut from #524 and rebased onto this one,
through the index daemon). Every plan passed `check --deep` against a healthy warm index first:

| Plan | Result | Gap |
|---|---|---|
| `03`, `10a`, `04`, `08` | applied, compiles | — |
| `06` ports files | 3 of 3 applied, **does not compile** | G |
| `07` host builders | 1 of 1 applied, **does not compile** | H |
| `05` spawn_split_agent | 5 of 5 applied, **does not compile** | L |
| `02` cli_session_manager (9 seams) | 9 of 9 applied, **does not compile** | G, I, J |
| `09` without op 6 | 7 of 7 applied, **does not compile** | K |
| `01` connection_service (10 seams) | 10 of 10 applied, **the test build does not compile** | M |

In every row the code moved, and the compile gate failed the run with the compiler's errors, leaving
the edits on disk. Before this change each failing row would have been reported as a success. The
developer deferred gaps G–M (2026-09-24: "it's important that it moves the code and the compilation
can be fixed manually"); they are recorded, with before/after code and rustc's errors, in the backlog
entry `2026-09-24-restructure-apply-gaps-from-the-lifecycle-destructure-run`. Plans `01`, `04`,
`04a`, `05`, `05a` and `07` had to be re-anchored by hand after #508 edited their files; that is
`2026-09-24-restructure-snapshot-cannot-rebase-a-stale-plan`. Raised during the wrap and filed at the
developer's request: `2026-09-24-restructure-has-no-signature-operations`.

## Backlog

No backlog entry is resolved here. The 2026-09-09 connection-service split entry and the other
restructure entries are unchanged; D8 was already fixed before this change. Added: the three
`2026-09-24-restructure-*` entries named above.

## Code issues, measured at wrap (2026-09-24)

| Record | Measurement | Outcome |
|---|---|---|
| `tddy-code-restructuring` `oversized-file-backends-rust` | **4,788 → 4,316** production lines (budget 500) | kept, partially fixed. Every changed piece went to a sibling under `backends/rust/` (`imports.rs`, `impl_seam.rs`, `chatter.rs`, `readiness.rs`, `early_return.rs`, `nested_modules.rs`) and `runner/compile_gate.rs`; none of `rust.rs`'s own seams were cut, so the decomposition still stands |
| `tddy-code-restructuring` `oversized-file-test-binary` | **966**, unchanged | kept. `Prose` and `readable_spans` were widened so `early_return.rs` can mask with the same lexer — a second consumer, which strengthens the case for extracting it |
| `tddy-code-restructuring` `complexity-rust-facade-lines` | **47** lines, nesting 5, unchanged | kept; moved from `rust.rs:3978` to `:3522` by code leaving the file above it |
| `tddy-index-daemon` `stale-repo-scoped-restructure-state-apply` | `apply.rs:45`, unchanged | kept. **Hit live** in the real run: after one plan applied through the daemon, the next was refused with `a journal already exists for this plan` until `.restructure/` was deleted by hand |

## Verification

Scoped to the three packages; whole-workspace health is CI's. After the `/pr-wrap` refactor pass:
`./test -p tddy-code-restructuring -p tddy-index-daemon -p tddy-lsp` — 44 result lines, all `ok`,
639 passed, 0 failed, 9 ignored. Scoped `cargo clippy --all-targets -- -D warnings`, `cargo fmt
--check` and `bash -n run-index-daemon` clean. `detached_daemon_production -- --ignored
--test-threads=1`: 4 of 4 against the real script.

## Left open

Recorded at wrap so they are not lost. None blocks #524.

**Engine**

- E4's scan is lexical. It does not see a `return` a macro expands to (`bail!`, `ensure!`): the
  compile gate catches it on `apply`, `check --deep` does not. A leading `|` in a match arm
  (`match x { | A => … }`) is misread as a closure. `break`/`continue` targeting a loop outside the
  range are the same hazard and are not refused.
- Two lexical repairs of the assist's output (`with_method_calls_restored`,
  `with_nested_references_restored`) sit between the assist and the rename, each with its own
  scanner. A generic qualifier (`Foo::<T>::modname::f`) is not undone and is still refused. If a free
  item and an inherent member with the same name both move, the type-qualifier rule could strip a
  legitimate `file_module::modname::f`; not seen, not guarded.
- Gap A's parent-binding `super::` case is not reproducible in a fixture (rust-analyzer offers the
  import itself there); only the alias path is proven live.
- The seam-lost baseline is name-level: a name already unresolved somewhere before the cut is not
  weighed for a parent occurrence the seam newly strands. A per-name count would close that.
- `already_bound` still checks the module's block even for an occurrence in the parent. Pre-existing,
  harmless while the reconstructions are gated to the module.
- A cut through a trait `impl` with **no** sibling reference is not refused, though it is E0119 all
  the same. Pre-existing, out of this change's scope.
- Three walkers read the same `use` tree: `expand_use` (paths, `rust.rs`), `collect_aliases` (alias
  pairs, `rust.rs`) and `collect_bound` (bound names, `imports.rs`). One leaf walker yielding
  `(path, alias)` would serve all three.
- `ServerChatter::quiescent`'s doc links to the private `RustBackend::ensure_indexed`, which lives
  in another module now.
- The degraded-index refusal names this repo's `./run-index-daemon` script: host knowledge inside a
  library, kept by decision because the advice is actionable where it is read.

**Compile gate**

- It adds a `cargo check --all-targets` before and after every writing apply: incremental on a warm
  target directory, the price of a check on a cold one, and it contends for the checkout's `target/`
  lock. No opt-out exists, by direction; one needs the developer's consent.
- The baseline runs before the per-operation static checks, so a plan one of those would refuse
  still pays a full `cargo check` first.
- On cancel the `cargo` child is killed but the `rustc` children it started are not; they finish
  their unit into `target/`.

**Index daemon and `run-index-daemon`**

- `Warm` reports `ready` for a degraded root; only the first operation refuses. `GraphLoad` could
  carry the health so `Warm` says so too.
- `durable_tmpdir` falls back to `/tmp` where `getconf DARWIN_USER_TEMP_DIR` is unknown (every
  non-macOS host). The same shape as the old `${TMPDIR:-/tmp}`, recorded because every fallback is
  named.

**Tests**

- The E2 fixture relies on a 15 s `sleep` in its build script outlasting the first hover: a timing
  race by design, justified in the fixture doc, at least 15 s per run.
- `detached_daemon_production`'s restart test guards a scheduling race it cannot force, so a pass is
  evidence, not proof. The suite is `#[ignore]`d.
- `tests/harness/mod.rs` is past 1,500 lines; the single-crate seam builders and `the_module_named`
  could move to a `harness/fixtures.rs` sibling if more extraction suites follow.
- `.config/nextest.toml`'s `rust-analyzer` group still omits older live binaries
  (`nested_module_move_acceptance`, `cluster_move_acceptance`, `facade_cycle_acceptance`, …).
- The same `include_str!` test-binary fixture is written twice, in the restructuring harness and in
  `code_index_service_acceptance.rs`; crate boundaries force it.
