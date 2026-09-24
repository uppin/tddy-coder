# 2026-09-24 — The Rust backend closes every document an operation opens

**Type:** Fix

`#carve` 14/15, PR [#524](https://github.com/uppin/tddy-coder/pull/524) (`3714a654`). Cross-package
entry: [2026-09-23-carve-lifecycle-destructure.md](../../../../docs/dev/changesets/2026-09-23-carve-lifecycle-destructure.md).

**Defect.** Plan `05` of the lifecycle destructure was refused 3 of 3, at `check --deep` and at
`apply`, over a correct path: "writing the parent's own `use super::super::SplitStartFailure;` into
the module left 4 unresolved occurrence(s) of it, where there were 3". A fresh daemon never refused
it; the same warm daemon refused it after a `check --deep` of plan `01`.

**Root cause.** The backend sent `didOpen` for every document it worked on and never sent
`didClose`. rust-analyzer treats an open document as the authority on its file, and
`tddy-index-daemon` serves backend after backend on one server per root, so plan `01`'s check left
`connection_service.rs` open at a 999-line rehearsed text of a 1,652-line file, in which
`SplitStartFailure` was declared nowhere. The refused run saw 16 unresolved names in the untouched
file (a passing run: 0). `serverStatus` read quiescent throughout, so it was not a missing
re-analysis wait.

**Fix.** `backends/rust/documents.rs` owns `did_open`, records each document opened, and closes
them all when `resolve`, `anchor_for` or `outside_references` ends (`closing_what_it_opens`).
The entry point's error wins when both it and the close fail. Test:
`import_pass_acceptance::imports_the_parent_s_type_on_a_server_an_earlier_check_rehearsed_its_parent_on`,
which failed with the production message before the fix; the harness gains
`resolving_after_a_check_of`, `a_plan_of` and `SERVICE_MODULE`. `./test -p tddy-code-restructuring`:
488 passed, 0 failed, 1 ignored. See
[readiness-and-gates.md](../readiness-and-gates.md#documents-are-closed-when-an-operation-ends).

Code issues: `oversized-file-backends-rust` re-measured and kept open, 4,313 → **4,342** production
lines (the three `closing_what_it_opens` wrappers and the `opened` field; `documents.rs` is 59).
The other four records were not touched.
