# 2026-09-19 — `test_binary.rs` was created at 950 production lines

**Category:** Deferred decomposition
**Source:** the `/pr-wrap` file-length gate on #498 (`#carve` 4/10 `test-homes`)
**Deferred with explicit developer consent on 2026-09-19.**

#498 created `packages/tddy-code-restructuring/src/crate_move/test_binary.rs` at **966 production
lines** (950 before `/pr-wrap`'s function-level refactor added doc comments), 1.9× the 500 budget. The gate's rule for a file a PR pushes past the budget is *decompose
now*; deferring one requires the developer's consent, which was given, with the reason recorded here
rather than left implicit.

**Why deferred, not done:** CI was fully green on the exact commit (7,006 Rust tests, 2,637 web
tests, both build arches, lint, generated-code and VM checks). The operation in that file took four
rounds of defects to stabilise — every one found by applying the real plan and reading the output
rather than trusting an `applied N of N`. A 950-line split plus a cold rebuild re-opens a node that
had just settled, while #491 waits on this branch.

**This is not a licence to leave it.** The measurement, the seams and the hazard are in
[`packages/tddy-code-restructuring/docs/code-issues/oversized-file-test-binary.md`](../../../packages/tddy-code-restructuring/docs/code-issues/oversized-file-test-binary.md).
The scanner is the clean cut and takes the file under budget on its own — but it is the shared
definition of "code" for both the re-pointing and dependency-collection passes, and those two must
never disagree about whether a span is a comment.

**Best time to do it:** after the `#carve` stack lands, before anything else grows that file.
