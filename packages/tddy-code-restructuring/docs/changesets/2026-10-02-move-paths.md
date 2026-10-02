# 2026-10-02 — Move paths

**Type:** Fix

`#live-plan` 3/7, PR [#540](https://github.com/uppin/tddy-coder/pull/540). Product entry:
[2026-10-02-move-paths.md](../../../../docs/ft/coder/changelog/2026-10-02-move-paths.md). Single-package
change, so no cross-package entry.

`crate_move/survey.rs` (new) holds `PathSurvey`, `SurveyedPath { written, resolved, defining_crate,
defined_at, in_test, in_body, site }` and `survey_moved_file`; `crate_move/source_scan.rs` (new) is the
token reading under it and `crate_move/reexports.rs` (new) follows a path through the origin's
re-exports to its defining crate. `header.rs` rewrites from the survey, `refusals.rs` and `cluster.rs`
read the edges from it, `moving.rs` writes `[dependencies]` and `[dev-dependencies]` from it and asserts
the destination is never in its own manifest. `test_binary.rs` widened `is_a_built_in_root`,
`names_bound_in` and `segment_length` to `pub(crate)`, no behaviour change. `Header::header_origin_paths`
(`TODO(check-parity)`) is the header-only subset `check` still reads.
`SurveyedPath.defined_at`, the crate-rooted path after re-export following, was added during green because
the rewrite needs the whole followed path, which `defining_crate` cannot carry; `check-parity`
([#543](https://github.com/uppin/tddy-coder/pull/543)) consumes it. See [path-survey.md](../path-survey.md).

Resolves four backlog entries, each deleted by this wrap and each pinned by a case in
`tests/move_paths_acceptance.rs`:

- `2026-09-25-restructure-move-to-crate-follows-a-facade-back-to-the-destination` — `move_module_to_crate`
  re-points a `crate::` path through the origin's facade to the destination's own extern name.
- `2026-09-25-restructure-move-to-crate-leaves-the-destinations-own-extern-name` — `move_module_to_crate`
  leaves a path that names the destination by its extern name, and makes the destination depend on itself.
- `2026-09-25-restructure-move-to-crate-misses-a-crate-named-only-in-a-body-path` — a cross-crate move does
  not carry an external crate the moved file names only in a body path.
- `2026-09-25-restructure-move-to-crate-reads-an-import-reaching-the-destination-as-an-edge` —
  `move_module_to_crate` reads a header import that reaches the destination as an edge back to the origin.

Tests: `tests/move_paths_acceptance.rs` (new, 11 cases, live rust-analyzer) and unit tests in
`survey.rs`, `source_scan.rs`, `reexports.rs`, `header.rs`, `moving.rs`.

Final measurements, scoped to `tddy-code-restructuring`: `cargo clippy -p tddy-code-restructuring
--all-targets -- -D warnings` clean; `crate_move::` unit tests 64 passed; `move_paths_acceptance` 11
passed. CI on `703944e8` (the commit before the documentation commits): all 9 checks pass, Rust tests
8046/8046, Web tests 2760/2760.

File-length alerts, deferred with the developer's consent: `crate_move/cluster.rs` 611 to 617 production
lines, `source_scan.rs` (new) 527, `test_binary.rs` 966 unchanged, all over the 500 budget. `cluster.rs`
and `source_scan.rs` are tracked in `docs/dev/todo/2026-10-02-cluster-rs-is-617-production-lines.md`;
`test_binary.rs` in `docs/code-issues/oversized-file-test-binary.md`, which gained a measurement row.
The two scanners (`source_scan.rs`, `test_binary.rs`'s line scanner) coexist until one of those splits.
