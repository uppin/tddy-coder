# oversized-file: test_binary.rs — a whole operation, scanner and all, in one module

**Location:** `packages/tddy-code-restructuring/src/crate_move/test_binary.rs`
**Category:** oversized-file
**Detected:** 2026-09-19 by the `/pr-wrap` file-length gate on #498
**Metrics:** **966 production lines** (0 before this PR) · budget 500
**Restructure:** required — `extract_module --to_file` along the three seams below
**Status:** Open — deferred from #498 with explicit developer consent
**Deferred by:** #498 — `#carve` 4/10 `test-homes`, see `docs/dev/todo/2026-09-19-test-binary-rs-is-950-production-lines.md`

## Measurement history

| Run | Production lines | Note |
|---|---|---|
| 2026-09-19 | 950 | first detection — the file is new in #498 |
| 2026-09-19 | 966 | after `/pr-wrap`'s function-level refactor: two oversized functions split, +16 lines of doc comment. Function sizes improved, file size did not — which is why the decomposition below still stands |
| 2026-09-24 | 966 | unchanged by #527, which touched the file: `Prose` and `readable_spans` widened to `pub(crate)` because `backends/rust/early_return.rs` masks strings and comments with the same scanner. That is a second consumer of the scanner outside this file, which strengthens the case for extracting it below |

## What the gate found

The file was created by #498 at **1.9× the budget**. It is not one idea: it is the
`move_test_binary_to_crate` operation *plus* a hand-written lexical scanner it needed and nothing
else in the crate has.

| Seam | What it is |
|---|---|
| The operation | `TestBinaryMove`, `read_test_binary_move`, `resolve_test_binary_move` — the three edits a move writes |
| The facade walk | `defining_home`, `Defining` — repeated `module_home::defining_crate` with a visited set |
| The lexical scanner | `readable_spans`, `crate_shaped_heads`, `opens_a_path`, `names_bound_in`, `use_trees`, `record_names_bound_by`, `grouped_members`, `modules_declared_in` — separating code from comments and string literals, and tracking what a file binds |

The scanner is the largest of the three and the least specific to test binaries: it answers "what
does this Rust file name, in code, that it did not bind itself". Nothing about that is about moving
a test.

## What would close it

Extract the scanner to its own module — it is the clean cut and takes the file under budget on its
own. The facade walk is a smaller, optional second cut.

Beware: the scanner is the shared definition of "code" for both the re-pointing pass and the
dependency-collection pass. Whatever it becomes must stay one definition, or the two passes can
disagree about whether a span is a comment — which is precisely the bug class #498 spent four rounds
fixing.
