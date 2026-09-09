# 2026-09-09 — `analyze coverage`: instrument only the crate under analysis

**Type:** Performance

Capturing `tddy-daemon` ran at **21–25 s per test**, putting a full run at **~14 hours** for ~2,014 tests. Measuring one cycle on the `acceptance_daemon` harness: running the test took 0.14 s, `llvm-profdata merge` 1.9 s, and `llvm-cov export` **25.1 s** — ~93% of it — emitting a **209 MB** JSON of **341,709** functions that `serde_json` then parsed in full, of which `is_foreign_source` kept a few hundred. Every test re-decoded the same 28.8 MB `__llvm_covfun` section, virtually all of it dependencies.

**Instrumentation is now per unit, not per graph.** `-C instrument-coverage` moved out of `RUSTFLAGS` — where it reaches every dependency — into a generated `RUSTC_WRAPPER` that applies it to the package under analysis and to every libtest harness. The harness clause is load-bearing: rustc links the profiling runtime only into a unit it instruments, so an uninstrumented integration harness runs and writes no `.profraw` at all. Measured on `tddy-code-analysis`, whole-graph vs targeted: `__llvm_covfun` **961.7 KB → 13.6 KB**, export **0.30 s → 0.01 s**, JSON **8.4 MB → 168 KB**, functions **11,535 → 131**.

**`llvm-cov export` is told how many threads it may use.** It renders single-threaded by default: **25.06 s → 4.96 s** on the daemon harness. Parallel worker processes were measured too (4× and 8× concurrent exports) and did not beat it — 5.5–7.0 s/test against 6.2 s — because the machine is already saturated.

**Instrumented builds no longer share `target/`.** They differ from an ordinary build only in rustflags, so one dir made each invalidate the other on every capture. They now build under a stable directory in the system temp dir, which also keeps the wrapper's mtime stable — cargo folds it into the fingerprint, so regenerating an identical script would force a full rebuild each run.

**Build- and list-time profiles are diverted.** Anything instrumented that *runs* during a capture writes its profile to the current directory: one run left **11,116 `.profraw` files / 379 MB** in the working tree. `list_tests` was a source of these too — merely enumerating an instrumented harness writes a profile, and it set no destination. Both now write to the build directory, fixing at the source what [2026-09-06](2026-09-06-analyze-coverage-runnable-from-the-dev-shell.md) could only gitignore.

**Contract change:** `instrumented_rustflags_for` is now `link_rustflags_for` and carries linker flags only. The three tests added in #462 asserted `RUSTFLAGS` contains `-C instrument-coverage`; that is deliberately no longer true, and they now assert the opposite — with the wrapper's behaviour covered by executing it against a stand-in rustc. lld branch coverage is retained.

**Not addressed:** true bulk export — parsing each harness's mapping once and reading per-test counters via `llvm-profdata show --counts` — would mean reimplementing LLVM's counter-expression evaluation, risking silently wrong coverage for less gain than the above. (tddy-code-analysis)
