# Cross-crate moves take the sibling test modules that test only the moved code - PRD

**Date**: 2026-10-09
**PRD Type**: Enhancement (plus one defect fix)
**Stack**: `#reshape` 14/19 (`feature/reshape/tests-follow`)

## Affected Features

- **Primary Feature**: [Rust code restructuring](../rust-code-restructuring.md). Sections:
  - `## Rust operations (v1)`: the `move_module_to_crate` and `move_cluster_to_crate` rows gain "sibling `#[cfg(test)]` modules that test only the moved code move too".
  - `## Path survey`: a file declared under `#[cfg(test)]` is test code throughout; a path that reaches a co-moving module through a re-export is re-pointed into the destination.
  - `## Known limitations`: one new entry (which test modules stay, and why), and the "`crate::` path in the moved file's `mod tests`" entry extended to out-of-line test files.

No other feature document changes. There is no new operation, plan field, flag or wire message. The plan schema (`.agents/skills/code-restructuring/references/plan-schema.md`) changes only in prose.

## Summary

Today a cross-crate move leaves behind every `#[cfg(test)] mod x_tests;` declared *beside* the moved module, in the parent file that stays. Those test files then fail to compile in the origin, because their `super::` paths name code that left. After this change:

- **A test module that tests only moved code moves with it.** A `#[cfg(test)] mod t;` declared in the same file as a moved module follows the move when everything it names in the origin is moving, and it names at least one moving thing. Its file (and its own children) lands beside the moved module in the destination, its declaration goes with it, and its paths and crates are handled like any moved file's.
- **A test module that also needs code staying behind stays,** and the run says so in a note naming the path that keeps it.
- **A test file is test code throughout.** A file reached through a `#[cfg(test)] mod` declaration sends its crates to `[dev-dependencies]` and makes no dependency-cycle edge, whether it follows as a sibling or is carried as a child.
- **A path through the parent's re-export of a co-moving module is re-pointed into the destination,** not back at the origin.

## Background

`#carve` 21/21 (R9, PR #536) moved 46 launch modules from `tddy-session-lifecycle` to `tddy-agent-launch` with one `move_cluster_to_crate`. The engine moved modules, not the test files declared beside them, and four were moved by hand in their own commit:

| Test file | Shape | Owner after `#reshape` |
|---|---|---|
| `claude_cli_spawn_steps/claude_cli_spawn_steps_tests.rs` | declared **inside** the moved `claude_cli_spawn_steps.rs`; `E0583` in the destination | node 5 (`move-children`) carries it as a directory child |
| `conversation_spawn_wiring_tests.rs`, `host_session_socket_tests.rs`, `session_acting_identity_tests.rs` | declared **beside** the moved modules in `connection_service.rs`, which stays; `E0432`/`E0433` in the origin | **this node** |
| `stack_child_spawn_tests.rs` | beside, but also builds a `DaemonSessionHost` (lifecycle) | stays: correct today, and correct after this node |

The hand fixes were four paths and one manifest line. Two paths go through origin facades to other crates, which the existing survey already re-points once the file is surveyed. One (`use super::recipe_enables_conversation_spawn;`) reaches a moved module through the parent's `pub(crate) use conversation_spawn::*;`, which the header pass today re-points back at the origin. The manifest line (`[dev-dependencies] tempfile`) is there because an out-of-line test file is not read as test code.

46 out-of-line `#[cfg(test)] mod x;` declarations exist in `packages/*/src`, about 30 of them as siblings in `tddy-session-lifecycle/src/connection_service.rs`, which later carves will keep splitting.

## Proposed Changes

### What's Changing

**Which test modules are considered.**
- Every `#[cfg(test)] mod t;` (or `#[cfg(all(test, …))]`, the marker every other `in_test` decision reads) declared at the top level of a moved member's declaring file, when that file **stays** in the origin.
- A test declaration inside a file that moves is a child, and node 5 carries it. This node adds nothing for it except reading it as test code (below).
- Inline `#[cfg(test)] mod tests { … }` blocks and test modules declared in any other file are not considered (see What's Staying the Same).

**Which of them follow.** For each considered module, every file it spans is surveyed at its own module path (the declaring file's path plus `t`), exactly as a moved file is. Then:
- a path *names the origin* when the crate that defines what it reaches is the origin;
- a path *reaches the move* when what it reaches, as written **or** after following the origin's re-exports, is inside a module this operation moves (members and the files they carry) or one an earlier operation of the plan already moved;
- the test module **follows** when every path that names the origin reaches the move, and at least one does;
- otherwise it **stays**. When it stays and names something that moves, the resolution carries a note naming one path that keeps it (`` `stack_child_spawn_tests` stays in `connection_service.rs`: it names `super::*`, which stays in `tddy-session-lifecycle` ``). A test module that names nothing the move takes is left alone, with no note.
- It also stays (with a note) when an item it defines is named from a file outside the moving set, or when its declaration places it with `#[path]`.

**What following means.**
- Each file of the test module moves by `git mv`, landing where the moved module's own declaration lands. For a member that lands at the destination root, `src/<parent>/t.rs` → `<dest>/src/t.rs`, and its own children go to `<dest>/src/t/…`.
- The declaration leaves the origin file together with the attributes and doc comments directly above it, and is appended to the destination root as `#[cfg(test)]` + `mod t;`, with the same doc comments, after the root's last line.
- Each file goes through the header re-point at its new position. `super::moved::X` becomes `crate::moved::X`; a path through an origin facade becomes the defining path; a `super::` that stays inside the test module is left as written.
- The test files join the travelling set before the caller survey, so their references to moved items are not re-pointed as callers.
- No facade is written for a test module: nothing outside it names it.

**Test code throughout.**
- A file reached through a `#[cfg(test)] mod` declaration, or below one, has every path read as `#[cfg(test)]`. Its crates go to the destination's `[dev-dependencies]` (unless production code names them too), and a path of it back into the origin is not a dependency-cycle edge, which is the existing rule for in-file `#[cfg(test)]` code.
- This applies to sibling test modules that follow and to node 5's carried children declared under `#[cfg(test)]` (the `claude_cli_spawn_steps_tests` shape).

**Paths through a re-export of a co-moving module.**
- The header pass treats a path as co-moving when its `defined_at` (after following re-exports) is inside the moving set, not only when its written resolution is. That is the reading the body precondition already takes. `super::recipe_enables_conversation_spawn` lands as `crate::conversation_spawn::recipe_enables_conversation_spawn`.
- This also fixes a moved member that reaches a co-moving sibling through its parent's glob.

**Refusals, each naming the file and written before anything moves.** Static checks, so plain `check` reports them too:
- a following test module whose landing file or `mod t` declaration already exists in the destination (a merge).

**Reporting.**
- Each following test module adds a note: `` test module `t` (`N` file(s)) follows `<module>`: everything it names in `<origin>` moves ``. Staying notes as above. `check --deep` and `apply` print them.
- The dry-run and apply `-> N file(s)` counts include the test files and stay equal.

### What's Staying the Same

- **Plan line.** No new field, no opt-out (F3, decided 2026-10-09).
- **Mixed test modules stay.** One that needs code staying behind is not split, widened or moved.
- **Inline `#[cfg(test)] mod tests { … }` in the parent** that tests a moved sibling is not split out (an `extract_module` job).
- **Test modules declared elsewhere** than the moved module's declaring file are not considered.
- **Same-crate operations** (`reparent_module`, `extract_module`, `move_item`) do not take sibling test modules.
- **Widening.** A following test module that reaches a private item is not widened (node 7, `feature/reshape/move-widen`); `pub(in crate::origin::…)` respelling is node 8's (`feature/reshape/move-grouped-use`).
- **`move_test_binary_to_crate`** and `tests/*.rs` binaries.

## Impact Analysis

### Technical Impact

All changes are in `tddy-code-restructuring`:

- New `crate_move/test_modules.rs`: considered declarations, classification, the follow edits, notes and the static refusal.
- New `crate_move/source_scan/test_declarations.rs`: reads `#[cfg(test)] mod t;` declarations with their attribute-and-doc span (kept out of `items_of_module`, which is on the function-size list).
- `crate_move/header.rs`: `reach` reads `defined_at` for co-moving, and a test-throughout header entry point over the same pass.
- `crate_move/cluster.rs`: the travelling set and the edit include following test files; `resolve_cluster` does not grow.
- `crate_move/preconditions.rs`: the static refusal joins `unrunnable`.

Tests: a new library-level binary `tests/crate_move_test_modules.rs`, additions to `tests/check_precondition_parity.rs`, unit tests for the declaration reader, and one compiled case in `tests/cluster_move_acceptance.rs` (already registered for rust-analyzer).

### User Impact

- A cluster or module move takes its sibling test modules in the same plan line: no hand `git mv`, no hand-moved declaration, no hand-written `[dev-dependencies]` line.
- Test files that stay behind are named in a note, so the reader knows the move considered them.
- Existing plans whose test modules qualify now move them. Anyone relying on them staying will see them in the destination.

## Implementation Plan

1. **Declaration reader**: `test_declarations` with its span, unit tests.
2. **Test code throughout**: the test-throughout header entry, for following siblings and node 5's gated children.
3. **`reach` reads `defined_at`**, with a member-level test.
4. **Classification** over considered declarations, including the earlier-operation case and outside references.
5. **Follow edits**: renames via node 5's relocation rule, origin declaration removal, destination declaration, travelling set.
6. **Refusal and notes**, static parity.
7. **Compiled live case.**
8. **Docs at wrap** and deletion of the claimed todo.

## Acceptance Criteria

- [ ] `move_cluster_to_crate` of a nested module whose parent declares `#[cfg(test)] mod a_tests;` naming only `super::a::…` moves `a_tests.rs` beside `a` in the destination, removes the declaration (with its doc comment) from the parent, appends `#[cfg(test)] mod a_tests;` to the destination root, and the workspace compiles with its tests ([Rust code restructuring](../rust-code-restructuring.md)).
- [ ] A following test module's `super::` path through the parent's glob of a moved module becomes `crate::<module>::…`; a `crate::` path through an origin facade becomes the defining path.
- [ ] A crate only a test file names (following or carried under `#[cfg(test)]`) joins the destination's `[dev-dependencies]`.
- [ ] A test module that also names a module staying behind stays, unchanged, and the resolution's note names the path that keeps it. One naming nothing the move takes stays without a note.
- [ ] A test module whose items another file names stays, with a note.
- [ ] With `reexport: "none"`, no reference inside a following test module is rewritten as a caller.
- [ ] In a two-operation plan, a test module of both modules stays at the first operation and follows at the second.
- [ ] A moved member reaching a co-moving sibling through its parent's glob is re-pointed into the destination.
- [ ] A following test module whose target exists in the destination is refused by `check` and `apply` with the same message, naming both paths, and nothing is written.
- [ ] `apply --dry-run` and `apply` print the same `-> N file(s)` count and the follow note.
- [ ] Existing crate-move suites pass unchanged.
- [ ] Tests pass for `tddy-code-restructuring` (scoped `./test -p tddy-code-restructuring`; CI for the rest).

## Decisions

F1–F5 (changeset `## Decisions & Trade-offs`) decided by the developer 2026-10-09, all as recommended.

## References

### Affected Features (Complete List)
- [Rust code restructuring](../rust-code-restructuring.md)

### Related Documentation
- Changeset: `docs/dev/1-WIP/2026-10-09-reshape-tests-follow.md`
- Discovery: [2026-10-09-reshape-tests-follow-initial-discovery.md](../../../dev/1-WIP/2026-10-09-reshape-tests-follow-initial-discovery.md)
- Todo this closes: [cluster-move-strands-test-modules-of-the-moved-code](../../../dev/todo/2026-10-08-restructure-cluster-move-strands-test-modules-of-the-moved-code.md)
- Package docs: `packages/tddy-code-restructuring/docs/path-survey.md`, `packages/tddy-code-restructuring/docs/facades.md`.
