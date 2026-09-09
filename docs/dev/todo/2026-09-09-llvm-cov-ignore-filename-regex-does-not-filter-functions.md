# 2026-09-09 — `-ignore-filename-regex` does not filter llvm-cov's `functions` array

**Category:** Future enhancement
**Source:** `analyze-coverage-export-and-harness-selection` (#466)

Recorded so nobody "fixes" the wrong thing. `export_args` passes
`-ignore-filename-regex=(cargo/registry/|cargo/git/|/rustc/|/target/)` to `llvm-cov export`, and
llvm-cov applies it to the JSON's **`files`** array only. `normalize_export` reads **`functions`**,
which is unfiltered — measured directly: an export with the regex and one without were both 209 MB
with the same 341,709 functions.

So the flag does nothing for this crate's purposes. What actually excludes dependency sources is the
Rust-side `is_foreign_source` predicate applied in `normalize_export`, which is why the two are
built from one `FOREIGN_SOURCE_MARKERS` list. Before #466 the regex was also *wrong* (it matched
`/cargo/registry/`, never `~/.cargo/registry/`); correcting it changed no output, and only the
predicate moved the denominator — 5,179 → 127 on `tddy-code-analysis`.

The flag is kept because [targeted instrumentation](../changesets/2026-09-09-analyze-coverage-targeted-instrumentation.md)
means foreign sources should not be in the binary at all, and it is a cheap guard if one ever is. If
that ever needs revisiting, the question is whether llvm-cov gained a functions-level filter — not
whether the pattern is right.
