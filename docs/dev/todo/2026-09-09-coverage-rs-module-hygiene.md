# 2026-09-09 — `coverage.rs` hygiene: `normalize_export`, repeated status checks, magic indices

**Category:** Future enhancement
**Source:** `/analyze-clean-code` on #462, carried through #466

Flagged by a clean-code pass on `packages/tddy-code-analysis/src/coverage.rs` and deliberately not
bundled into #466, whose diff was already large and whose point was behaviour: mixing a module split
into it would have buried three real fixes under a move diff.

- **`normalize_export` is 93 lines at nesting depth 4** — the only "must refactor" item in the file.
  It does two unrelated jobs inside three nested loops: decoding llvm-cov's positional region tuples
  and merging function records by max count. Extracting `parse_region(region_arr, filenames) ->
  Option<(String, RustRegion)>` and `record_function(entry, function, func_line)` leaves the outer
  loop at ~25 lines with nothing escaping — `by_file` stays local.
- **The same 5-line "child failed, wrap stderr in `AnalysisError::Cargo`" block appears four times**,
  and `.map_err(|e| AnalysisError::Cargo(e.to_string()))` six times. One `fn run(cmd: &mut Command)
  -> Result<Output>` replaces both. This duplication is also *why* an llvm-cov usage error surfaced
  as `cargo failed: No filenames specified!` and read as a build problem for so long — the wrapper
  loses which tool actually failed.
- **Magic indices into llvm-cov's region tuple**: `region_arr.len() < 8`, `[5]` for the file id, `[7]`
  for the kind, `[0..4]` for the span. Named constants, or a `RegionTuple` decode, would name all
  seven at once. Related: `"code"` and `"branch"` are restated as literals at the kind check while
  also being entries `[0]` and `[4]` of `REGION_KINDS`, so the const and the check can drift.
- **The file is over 500 lines** (518 before #466, larger now). Cohesive split if it grows further:
  `coverage/toolchain.rs` for tool discovery, rustflags and the rustc wrapper; `coverage/export.rs`
  for `export_profile` + `normalize_export`; the pipeline driver and artifact loaders stay put.
  `RustFileCoverageWithRegions` and `DenominatorFile` would become `pub(crate)`; no external caller
  moves.
