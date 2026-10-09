# Changeset: same-crate moves widen what they split, and leave no empty directories

**Date**: 2026-10-09
**Status**: 🚧 In Progress
**Type**: Feature (engine capability of two existing operations; `apply` filesystem sweep; test registration)
**Stack**: `#reshape` 1/19, branch `feature/reshape/widen-same-crate`, PR [#598](https://github.com/uppin/tddy-coder/pull/598) (draft), wave 1. PR title:
`feat(code-restructuring): same-crate moves widen split members and tree reach, and drop emptied dirs (#reshape 1/19)`.
Base in the linear stack: `master` (K=1). **Real edges**: none in (this node consumes nothing); out:
`widen-same-crate -> move-impl-members` (K=13 calls `widen_members`, `members_of`, `member_visibility_edit`,
`root_items`), `widen-same-crate -> oversized-files` (K=15: its `tidy.rs` and `assemble.rs` seams split private
fields and methods from their users), `widen-same-crate -> rust-backend-split` (K=17: `move_item` must widen the
private fields of `Assist`, `Placeholder`, `Produced`, `LspPoint`, `LspEdit` and `PathReached`; `return_type.rs`
builds `Assist` literals).

## Initial Discovery

Full codebase exploration that grounded this plan:
[initial-discovery.md](./2026-10-09-reshape-widen-same-crate-initial-discovery.md) (Exploration 1 is the
whole-work backlog discovery, Exploration 2 is this node's).

State A below is distilled from that file. Do not duplicate grep traces or file dumps here.

## Prerequisites

`grep -rl 'Claimed by:'` over `packages/tddy-code-restructuring` finds one file
(`broken-restructure-anchors-empty-outline.md`, value `none`; `#reshape` 12 claims it). **No 🚧 claimed
issue is in this node's path, so there is no wait-or-proceed decision.**

| Item | Verdict | What this change does about it |
|---|---|---|
| [2026-10-04-restructure-move-item-does-not-widen-fields-or-impl-members.md](../todo/2026-10-04-restructure-move-item-does-not-widen-fields-or-impl-members.md) | ✅ **RESOLVED HERE** (main limit); entry **narrowed** at wrap | Member survey and widening (rules 1-3). Its "Other limits of the first cut" list (nested `use` group, keyword above name, one-line inline destination, relative range, `#[path]` lib+main, fn-body `use`) is untouched and stays as the entry's whole content |
| [2026-10-04-restructure-reparent-module-does-not-widen-what-the-moved-tree-reaches.md](../todo/2026-10-04-restructure-reparent-module-does-not-widen-what-the-moved-tree-reaches.md) | ✅ **RESOLVED HERE** | Bullets 1-2 (rules 4-5). Bullet 3 (an absolute `pub(in crate::host::attachments)` is not respelled) is **stale on `master`**: rust-analyzer reports the module's name inside the visibility as a reference, so the callers' re-pointing rewrites it (acceptance test 12 is a green pin, found by the contract commit). Deleted at wrap. The sibling-module case it does not name becomes a new todo |
| [2026-10-05-restructure-same-crate-moves-limits-found-moving-lifecycle.md](../todo/2026-10-05-restructure-same-crate-moves-limits-found-moving-lifecycle.md) | ⚠ **DURING**, claimed here; **narrowed** at wrap | Emptied directories close (rule 6). The `pub use` chain item moves to a new todo (no reproduction; developer decision 2026-10-09). The aliased `use` and glob `super::Name` items are `feature/reshape/move-item-paths`'s slice and stay. Import placement and one module per `name` stay |
| [2026-10-04-restructure-reparent-module-first-cut-limits.md](../todo/2026-10-04-restructure-reparent-module-first-cut-limits.md) | partial, claimed here; **narrowed** at wrap | Emptied directories close (rule 6). Items 1 (doc wording) and 2 (byte-identical facade test) are **already fixed on `master`** (`plan-schema.md:220`; `reparent_module_acceptance.rs:286-297`) and close. Every other refusal stays |
| [2026-10-06-restructure-item-move-assemble-past-500.md](../todo/2026-10-06-restructure-item-move-assemble-past-500.md) | ⚠ **DURING** (resolved by `feature/reshape/oversized-files`) | `item_move/assemble.rs` (507) gains **no** net line: the member widening is in `item_move/members.rs`, and the one changed call keeps its line count |
| `packages/tddy-code-restructuring/docs/code-issues/complexity-rust-facade-lines.md` | ⚠ **DURING** (resolved by `feature/reshape/fn-sizes-backend`) | `assemble` (82), `visibilities` (71) and `move_items` (73) are on the >60 list: none grows by a net line (one call renamed in each of `assemble` and `move_items`, and a field type changed) |
| `docs/dev/todo/2026-10-09-restructure-moves-do-not-widen-tuple-struct-fields.md` (written by `feature/reshape/move-widen`) | — Referenced | A tuple field split by a move stays a compile-gate failure if the outline does not list it. Test 26 records what the outline says |
| `packages/tddy-code-restructuring/docs/code-issues/oversized-file-backends-rust.md` | — Unrelated | `backends/rust.rs` is not edited |
| Feature doc limit "A rolled-back group leaves an empty directory" (`docs/ft/coder/rust-code-restructuring.md` § Known limitations; no todo file) | ✅ **RESOLVED HERE** | The same sweep runs after `PreImage::restore` removes a file (decision F6) |

## Affected Packages

- **`tddy-code-restructuring`** ([README.md](../../../packages/tddy-code-restructuring/README.md)):
  - new files: `src/backends/rust/item_move/members.rs`, `src/backends/rust/module_reparent/tree_reach.rs`,
    `src/backends/rust/module_reparent/tree_visibility.rs`;
  - edited: `src/backends/rust/item_move.rs` (the reach call and `Reach`), `item_move/assemble.rs` (one
    call renamed, a field type), `item_move/outline.rs` (`Item` and a root-item reader widened to
    `pub(in crate::backends::rust)`), `module_reparent.rs` and `module_reparent/assemble.rs` (the tree
    reach and visibilities wired in), `src/apply.rs` (the sweep), `src/journal/group.rs` (the sweep after
    a removal);
  - new test binary: `tests/same_crate_widening_acceptance.rs`.
  - Docs at wrap: [same-crate-moves.md](../../../packages/tddy-code-restructuring/docs/same-crate-moves.md)
    (module tables, Limits) and [docs/ft/coder/rust-code-restructuring.md](../../ft/coder/rust-code-restructuring.md)
    (§ Same-crate moves **Visibility**; § Known limitations: four close, two narrow).
- **Repo config**: `.config/rust-e2e.filterset` and `.config/nextest.toml` (registration; see rule 7).
- **`tddy-tools`, `tddy-index-daemon`**: no source change.

## Related Feature Documentation

- [PRD-2026-10-09-reshape-widen-same-crate.md](../../ft/coder/1-WIP/PRD-2026-10-09-reshape-widen-same-crate.md) (this PRD)
- [Rust code restructuring](../../ft/coder/rust-code-restructuring.md): § Same-crate moves, § Known limitations

## Summary

`move_item` widens the private fields, methods and associated constants that a move splits from the code
that uses them, in both directions. `reparent_module` widens the old parent's and ancestors' private items
that the moved tree names. It also translates or widens every `pub(…)` written in the moved files whose
meaning the move changes. `apply` removes the directories a `git mv` emptied, and so does a group rollback.
Every live rust-analyzer suite of the package is registered in the e2e filterset and the serial
`rust-analyzer` test group.

## Background

Since #584 both operations widen only module-level items. A struct moved away from its `impl`, or a module
moved away from a parent whose private helper it calls, applies and then stops at the compile gate with
`E0616`/`E0624`/`E0603`. That costs the author a full index and a manual rollback. Every module move also
leaves an empty directory for a hand `rmdir`. The outline already carries the members, the old parent is
already open on the server, and `Scope` already answers "how wide is enough", so nothing new needs to be
asked of rust-analyzer in kind, only in number.

## Responsibility

- **Member survey for `move_item`:** read the members from the outline, ask references, widen to the
  narrowest `Scope` the users need, and report `Type::member`.
- **The shared member widening:** `members::widen_members` is a pure function. `feature/reshape/move-impl-members`
  consumes it.
- **Tree reach for `reparent_module`:** survey the old parent and the ancestors below the common ancestor,
  confirm with references, and widen.
- **Tree visibilities for `reparent_module`:** translate a scope that lies inside the tree, widen one that
  reached outside it until legal, with no server request.
- **The emptied-directory sweep** in `apply` and in the group rollback.
- **Registration:** every live rust-analyzer test binary of the package goes in `.config/rust-e2e.filterset`
  and the `rust-analyzer` group, including this node's new one (developer decision 2026-10-09).

## The rules (the contract)

**1. Which members are surveyed (`move_item`).** The named children of the root symbols of the source
file's outline, read by `members::members_of`:

- a struct's `Field` children;
- an **inherent** `impl` block's `Method`, `Function` and `Constant` children.

These are skipped: children of a trait `impl` (an `impl` whose outline name is `impl <Trait> for <Type>`),
items of a trait, enum variants, and any member written `pub` or `pub(crate)`. A member written with a
relative visibility (`pub(super)`, `pub(in …)`) is skipped too, because `rebase::edits` already respells
it to keep its meaning.

There are two sets:

- **(M) moved:** every private member of a struct or inherent `impl` whose root symbol starts inside the
  run;
- **(K) kept:** every private member of a struct or inherent `impl` outside the run whose name
  `text::identifiers_in` finds in the moved lines.

Each surveyed member costs one `textDocument/references` request, through `sites_of` with the member's
`selectionRange.start`. The progress line reads
`move_item: surveying N member(s) of the types the move splits`.

**2. The widening.** A member's `users` are the modules of its references, placed with `module_of_file` +
`enclosing_modules`:

- for **M**, those outside the run;
- for **K**, those inside the run, which become the destination.

The scope is computed as for items (`assemble.rs:221-238`):

1. `written = Scope::parse(visibility, source)`;
2. `starts_as = Within(destination)` when M and private, else `written`;
3. `scope = users.fold(starts_as, widened_to)`, with K folding `destination`;
4. spell it with `scope.spelled_in(lands_in)`, where `lands_in` is the destination for M and the source
   for K.

A member whose scope does not change gets no edit and no report line. Users are keyed by **(owner type,
member name)**, never by name alone. `users_of` (`assemble.rs:280-300`) filters by name alone, so a field
`name` and a moved `fn name` would mix.

**3. The edit and the report.** `members::member_visibility_edit` writes the keyword directly before the
member's name, after any same-line attributes. For a field that means inserting `pub(crate) ` before
`count`, or replacing an existing keyword. For a method it means replacing or inserting before `fn`, with
the same `declaration_prefix` reading `outline::visibility_edit` uses. It is a separate function because
`outline::visibility_edit` refuses every field (`outline.rs:209-214`: nothing stands between keyword and
name).

- M edits are added to the moved-text edits and their spans are claimed.
- K edits are added to the source edits.
- Report: `VisibilityChange { item: "<Type>::<member>", from, to }`, so a run prints
  `` `Counter::count` private -> pub(crate) ``.

**4. Tree reach (`reparent_module`).** `tree_reach::modules_to_survey(old_parent, new_parent)` returns the
old parent and each ancestor strictly below the **common ancestor** of the old and new parents, nearest
first. For `host` → `split` that is `[host]`; for `a::b::host` → `a::split` it is `[a::b::host, a::b]`.
For each such module whose file is found (`destination::find_module`), and only for the root items of that
module's **own file**:

- `did_open` it (the old parent is open already) and read `settled_outline`;
- take the root items written private or `pub(in …)`/`pub(super)` whose scope does not cover the new
  module path (`Scope::parse` + a covers check);
- keep those whose names `identifiers_in` finds in any moved file;
- confirm each with `references_at`, keeping only items with a reference located in a moved file;
- widen what is left with `scope.widened_to(new_module_path)` and spell it in its own module, with
  `outline::visibility_edit`. That is the `reached` rule `move_item` applies (`assemble.rs:257-274`).

The moved module's own `mod` declaration is excluded; `visibility::landing` owns it. A private `mod`
declaration of the old parent that the tree names (`super::sibling::f`) is a root item like any other and
is widened. The item `f` inside `sibling` is not surveyed (new todo).

**5. Tree visibilities (`reparent_module`).** `tree_visibility::respelled` reads every `pub(` span in each
moved file. It uses the masked text and the same span reading as `rebase::visibility_spans`, made
`pub(in crate::backends::rust)`. Each span is read at its **old** module: the tree path + `MovedFile.below`
+ `enclosing_modules` at the span.

- `pub(crate)` is never touched.
- If `old_scope` lies inside the old tree, the span is **left alone**. A relative one (a child's
  `pub(super)`) reads the same where the tree sits now. An absolute one
  (`pub(in crate::host::attachments)`) is already rewritten to `pub(in crate::split::attachments)` by the
  callers' re-pointing on `master`, because rust-analyzer reports the module name inside it as a
  reference (found by the contract commit: acceptance tests 12-13 are green on `master`). Writing it here
  too would be a second edit of the same bytes, which `text::applied` refuses as an overlap.
- If `old_scope` reaches outside the tree, the new scope is `old_scope.widened_to(new_module)`. Every module
  that could see it still can, and the new spelling is legal: `pub(super) fn materialize` in
  `host::attachments` becomes `pub(crate)` under `split`.
- A span that `Scope::parse` cannot read is refused naming the file and line, the way `read_scope` refuses
  an item (`assemble.rs:197-204`).
- An edit whose text is unchanged is dropped.
- Each widening is a report line: the item is the identifier after the
  declaration keyword, or the field name.

`rebase::edits` keeps skipping visibilities when `travelling` is set (`rebase.rs:47-52`). This rule owns
them, so `rebase.rs` is not edited.

**6. Emptied directories.** `apply::remove_emptied_directories(root, vacated)` runs at the end of
`apply_workspace_edit`, over the `from` of every `Rename`. For each one it walks the parent chain of the
vacated path:

- while the directory exists, is strictly below `root` and `read_dir` yields nothing, it calls
  `std::fs::remove_dir` and goes up;
- it stops at the first directory that is not empty, or at `root`;
- an I/O error other than the directory being absent is returned, with no fallback.

`PreImage::restore` calls the same helper after it removes a file a group had created (F6). Rollback and
re-run need no change: `git_move` and `PreImage::restore` already `create_dir_all` the parent
(`apply.rs:159-161`, `group.rs:39-41`). The dry run (`overlay.rs`) models files, not directories, and is
not edited.

**7. Registration.** Developer decision 2026-10-09: this node registers **every** live rust-analyzer suite
of the package. Other nodes register only the suites they add. A binary is live when it reaches
`harness::a_rust_analyzer_rooted_at` (`tests/harness/mod.rs:540`). That happens through `applying_*`,
`resolving*`, `performing*`, `refusal_from*`, `the_anchor_command_emits`, `what_the_server_answers`,
`with_a_rust_backend`, `rebasing_the_plan_file`, `re_resolving_in_the_store`, `same_crate::*`, or
`checking_the_plan*(…, true)`.

- **Added to `.config/rust-e2e.filterset`** (13): `anchors_package_relative_path`, `move_item_acceptance`,
  `move_item_beyond_the_basics_acceptance`, `move_item_creates_module_acceptance`,
  `move_item_into_an_existing_module_acceptance`,
  `move_item_of_an_item_the_destination_imports_acceptance`, `move_item_outside_facade_acceptance`,
  `reparent_module_acceptance`, `reparent_module_beyond_the_basics_acceptance`,
  `reparent_module_through_the_old_parents_import_acceptance`, `same_crate_deep_check_acceptance`,
  `signature_rewrites_acceptance`, `same_crate_widening_acceptance` (new).
- **Added to the `rust-analyzer` group override** (`.config/nextest.toml:88-106`), 32: the 13 above plus
  `anchors_command_acceptance`, `cluster_move_acceptance`, `escaping_types_acceptance`,
  `extraction_defects_acceptance`, `facade_cycle_acceptance`, `impl_item_anchor_acceptance`,
  `inline_paths_acceptance`, `item_anchor_acceptance`, `live_plans_acceptance`, `move_facades_acceptance`,
  `move_paths_acceptance`, `nested_module_move_acceptance`, `plan_store_acceptance`,
  `plan_store_resume_acceptance`, `prelude_shadow_acceptance`, `relative_visibility_acceptance`,
  `sibling_module_paths_acceptance`, `signature_assists_acceptance`, `transactional_groups_acceptance`.
- **Checked and not live** (they define local `refusal_of`/`resolving` over text): `repoint_call_plan_acceptance`,
  `repoint_facade_imports_acceptance`, `retarget_impl_plan_lines`.
- Green re-derives the list with the same rule before editing (another node may add a suite first), and
  runs `./dev bun test ./scripts/nextest-serial-groups.test.ts`.

## Boundaries

- **No new plan field, operation, flag or wire message.** No change to any refusal either operation makes
  today.
- **`extract_module`'s widening is untouched** (`backends/rust/visibility.rs`, `facade.rs`,
  `relative_visibility.rs`), and so is `backends/rust.rs`.
- **No `check --deep` output change.** The widenings travel in `Resolution.report`.
  `feature/reshape/move-widen` makes the deep check print it.
- **No cross-crate widening** (`feature/reshape/move-widen`), and no `pub(in crate::<origin>)` handling in
  crate moves (`feature/reshape/move-grouped-use`).
- **Same file only.** Members of an `impl` of a moved type that sits in another file are not surveyed.
  Neither is an item of a sibling module the tree reaches (new todo), nor a tuple-struct field the outline
  does not list (`feature/reshape/move-widen`'s todo).
- **No narrowing.** A member or item already wider than needed is never narrowed.
- **The size and function lists don't grow:** `item_move/assemble.rs`, `assemble`, `visibilities` and
  `move_items` gain no net line. `module_reparent/assemble::assemble` stays at or under 60 lines by
  delegating to one helper.
- **`rebase.rs` behaviour is unchanged.** Only `visibility_spans` is widened to
  `pub(in crate::backends::rust)`.

## Dependencies

This node has no parent in the stack: it consumes nothing from another `#reshape` node and builds on
`master` alone.

## Draft PR contract

Published with the wave-2 contract commit (the first push of this PR, not its deliverable). **Owned
surface, new today:**

- `backends::rust::item_move::members` (new module, declared `pub(crate) mod members;` in `item_move.rs`; `item_move` is a child of `backends::rust`, so every sibling module there reaches it). The four functions node 13 calls directly are `pub(crate)`: `members_of`, `widen_members`, `member_visibility_edit` and `outline::root_items`:
  - `pub(crate) struct Member { pub(crate) owner: String, pub(crate) name: String, pub(crate) position: serde_json::Value, pub(crate) visibility: String }`
  - `pub(crate) struct ReachedMember { pub(crate) member: Member, pub(crate) written_in: Vec<String>, pub(crate) lands_in: Vec<String>, pub(crate) users: Vec<Vec<String>> }`
  - `pub(crate) struct MemberWidening { pub(crate) edits: Vec<Edit>, pub(crate) report: Vec<VisibilityChange> }` (`Default`, `PartialEq`)
  - `pub(crate) fn members_of(symbols: &Value, text: &str, starting_in: std::ops::RangeInclusive<u32>, inside: bool) -> Vec<Member>`
  - `pub(crate) fn widen_members(text: &str, reached: &[ReachedMember]) -> Result<MemberWidening>` (the piece `move-impl-members` consumes)
  - `pub(crate) fn member_visibility_edit(text: &str, member: &Member, to: &str) -> Result<Edit>`
- `item_move.rs`:
  - `pub(super) struct Reach { pub(super) items: Vec<Item>, pub(super) moved_members: Vec<ReachedMember>, pub(super) kept_members: Vec<ReachedMember> }`
  - `RustBackend::move_reach(&mut self, uri: &str, workspace: &Workspace<'_>, source_text: &str, symbols: &Value, run: &Run, destination: &Destination) -> Result<Reach>`
    (**published differently from the plan's `reached_by_the_move`**: `reach_of` and the planned name were
    taken or too long for the one-line call that keeps `move_items` from growing, and nine parameters
    fail `clippy::too_many_arguments`. `left` is read inside, the source module comes from the
    workspace, and the destination is the `creation::Destination` `move_items` already holds). It
    wraps `reached_by_the_moved_code`, which stays. Until milestone M4 it returns no members
    (`// TODO(reshape-widen-same-crate)`), which is `master`'s behaviour.
  - `Moving.reached: &'a Reach` (`assemble.rs` reads `moving.reached.items`; net zero lines)
- `module_reparent::tree_reach` (new):
  - `pub(super) fn modules_to_survey(old_parent: &[String], new_parent: &[String]) -> Vec<Vec<String>>`
  - `pub(super) struct ReachedItem { pub(super) file: String, pub(super) module: Vec<String>, pub(super) item: Item }`
  - `RustBackend::reached_by_the_tree(&mut self, workspace: &Workspace<'_>, request: &Reparent, survey: &Survey) -> Result<Vec<ReachedItem>>`
  - `pub(super) type Widened = (Vec<(String, Edit)>, Vec<VisibilityChange>);` (the tuple, named, for `clippy::type_complexity`)
  - `pub(super) fn widenings(texts: &BTreeMap<String, String>, reached: &[ReachedItem], new_module: &[String]) -> Result<Widened>`
- `module_reparent::tree_visibility` (new): `pub(super) fn respelled(texts: &BTreeMap<String, String>, request: &Reparent, survey: &Survey) -> Result<Widened>`.
  `Reparenting.reached: &'a [ReachedItem]` is **not** added by the contract commit: a field nothing reads
  fails `dead_code`, so green adds it with the call (M6).
- `apply.rs`: `pub(crate) fn remove_emptied_directories<'a>(root: &Path, vacated: impl IntoIterator<Item = &'a str>) -> Result<()>`.
- Widened, visibility only: `item_move::outline::{Item, Run}` (and their fields) to `pub(crate)`, the module to
  `pub(crate) mod outline;`, and the new `pub(crate) fn root_items(symbols: &Value, text: &str, outside: Option<&Run>) -> Vec<Item>`,
  **implemented** (split out of `left_behind`, which now delegates to it; called directly by
  `move-impl-members`). `Run` is widened because `root_items` names it. `rebase::visibility_spans` is widened
  by green with its first caller (M5), not here.
- The registration edits of rule 7.
- Failing tests: every test in "Acceptance tests" except 12, 13 and 20 is red; those three are green pins. 25 and 26 are red
  until `members_of` exists (26's outline fixture is the one assumed for `struct Id(u32);`, marked
  `TODO` to be replaced by the captured one at M3).
- Unimplemented bodies are `todo!()` with `// TODO(reshape-widen-same-crate): implement`, and the new
  modules carry a module-level `#![allow(dead_code)]` with a `TODO` naming the milestone that removes it
  (nothing calls them before green wires them).

## Green wave

**Wave:** 1 of 4.
**Greenable independently:** yes. It has no parent and builds on `master`.
**Concurrent with:** every other wave-1 node (`multi-seam-extract`, `tidy-facades`, `extract-method-clean`,
`move-children`, `methods-leave-type`, `move-widen`, `move-grouped-use`, `new-crate`, `apply-robust`,
`move-item-paths`, `anchors-outline`). None is an edge. The line serialises them because of textual
overlap: `apply.rs` with `apply-robust` (`git_move`), `item_move/*` with `move-item-paths`, and
`.config/nextest.toml` / `.config/rust-e2e.filterset` with every node that adds a live suite.
**Blocks:** `feature/reshape/move-impl-members` (wave 2), `feature/reshape/oversized-files` (wave 2), `feature/reshape/rust-backend-split` (wave 3).
Real dependency edges (whole stack): `1→13`, `1→15`, `1→17`, `5→14`, `2→15`, `3→15`, `4→16`, `13→17`, `2→17`, `3→17`,
`17→18`, `6→18`, `4→19`, `17→19`.

## Successor PRs

- `feature/reshape/move-impl-members` calls `members::{widen_members, members_of, member_visibility_edit}` and
  `outline::root_items` directly (with `Member`, `ReachedMember`) to widen the members it moves between `impl` blocks.
- `feature/reshape/oversized-files` relies on `move_item` widening the private fields and methods its `tidy.rs`
  and `assemble.rs` seams split from the code that uses them (rules 1-3).
- `feature/reshape/rust-backend-split` relies on `move_item` widening the private fields of `Assist`,
  `Placeholder`, `Produced`, `LspPoint`, `LspEdit` and `PathReached` (all fields of `Assist` must reach
  `return_type.rs`, which builds `Assist` literals), by rules 1-3.

## Scope

- [ ] **Emptied directories**: `apply::remove_emptied_directories`, `PreImage::restore`
- [ ] **Member survey**: `members_of`, `reached_by_the_move`, progress line
- [ ] **Member widening**: `widen_members`, `member_visibility_edit`, report `Type::member`
- [ ] **Tree reach**: `modules_to_survey`, `reached_by_the_tree`, `widenings`
- [ ] **Tree visibilities**: `respelled`
- [ ] **Registration**: rule 7 in both files; serial-groups script test
- [ ] **New todo files** committed with the planning commit (`pub use` chain; sibling-module items)
- [ ] **Package and feature documentation** at wrap (list under Affected Packages); the four claimed or
  partial entries closed or narrowed as the Prerequisites table says
- [ ] **Testing**: acceptance tests pass; `./test -p tddy-code-restructuring`, scoped; CI for the rest
- [ ] **Code quality**: `cargo check -p tddy-code-restructuring --all-targets`, clippy `-D warnings`,
  `cargo fmt`; no listed function grows; new functions ≤ 40 lines

**Status indicators**: `[ ]` not started · `[~]` in progress · `[x]` complete ✅

## Technical changes

### State A (Current)

- `outline::run_covering` / `left_behind` read root symbols only (`outline.rs:55-90`, `:150-169`).
  `reached_by_the_moved_code` (`item_move.rs:192-225`) and `assemble::visibilities` (`assemble.rs:206-276`)
  widen root items only. No member is surveyed.
- `outline::visibility_edit` refuses a declaration with nothing between keyword and name
  (`outline.rs:209-214`), which is every field.
- `module_reparent::visibility::landing` (`visibility.rs:32-75`) is the only visibility a reparent writes.
  `rebase_the_moved_files` passes `travelling: Some(&old)` (`module_reparent/assemble.rs:203`), and
  `rebase::edits` skips every visibility span in that case (`rebase.rs:47-52`).
- `callers_of_the_module` opens the old parent but reads no outline (`module_reparent.rs:90-111`).
- `apply_workspace_edit` never removes a directory (`apply.rs:18-39`; no `remove_dir` in `src`).
  `PreImage::restore` removes a created file only (`journal/group.rs:47-51`).
- Twelve live suites are missing from `.config/rust-e2e.filterset` and 31 from the `rust-analyzer` group
  (Exploration 2, E2.6).

### State B (Target)

A same-crate move that splits a type from its `impl`, or a module from its parent's private items, applies
and compiles, and reports each widening. No directory a move or a rollback emptied is left behind. Every
live rust-analyzer suite runs in the e2e leg, one server at a time.

### Delta (What's Changing)

#### `tddy-code-restructuring`

- **New** `item_move/members.rs` (~200 lines with unit tests): rules 1-3.
- **`item_move.rs`**: `reached_by_the_moved_code` becomes `reached_by_the_move`, returning `Reach`. The
  call in `move_items` keeps one line, and the member survey's server loop is a new private method.
- **`item_move/assemble.rs`**: `Moving.reached: &Reach`. The `let landing = visibilities(…)` line becomes
  `let landing = landing_of(…)`, where `landing_of` (new, ~15 lines, in `members.rs` or beside
  `visibilities`) calls `visibilities` and then `members::widen_members` for M and K. The two `moving.reached`
  reads become `moving.reached.items`. Net zero lines in `assemble`, `visibilities` and `move_items`.
- **`item_move/outline.rs`**: `Item` and a new `root_items(symbols, text, outside: Option<&Run>)` widened
  to `pub(in crate::backends::rust)`; `left_behind` delegates to it.
- **New** `module_reparent/tree_reach.rs` (~150) and `module_reparent/tree_visibility.rs` (~150): rules 4-5.
- **`module_reparent.rs`**: `reparent_module` calls `reached_by_the_tree` after `callers_of_the_module`
  (+3 lines) and passes it in `Reparenting`.
- **`module_reparent/assemble.rs`**: one call to a new `widen_the_tree(moving, &texts, &mut edits) ->
  Result<Vec<VisibilityChange>>`, whose report extends `landing.report`.
- **`item_move/rebase.rs`**: `visibility_spans` widened to `pub(in crate::backends::rust)`.
- **`apply.rs`**: `remove_emptied_directories` plus one call at the end of `apply_workspace_edit`.
- **`journal/group.rs`**: one call after the `remove_file` in `PreImage::restore`.

#### Repo config

- `.config/rust-e2e.filterset`, `.config/nextest.toml`: rule 7.

## Implementation milestones

- [ ] **M1** emptied-directory sweep and the rollback; tests 16-20, then 14-15
- [ ] **M2** registration (rule 7) and the new live binary skeleton; test 29
- [ ] **M3** `members_of` and the outline probe; tests 25-26
- [ ] **M4** `widen_members`, `member_visibility_edit`, `landing_of`; tests 21-24, then live 1-7
- [ ] **M5** `respelled`; tests 27-28, then live 11-13
- [ ] **M6** `modules_to_survey`, `reached_by_the_tree`, `widenings`; test 29's unit sibling, then live 9-10
- [ ] **M7** docs staged, todo entries narrowed, scoped gate, function-length check

## Testing plan

### Testing Strategy

**Primary, library level with no server:** the member widening, the tree visibilities, the survey bound and
the sweep are functions of texts, outline JSON and paths. They are unit-tested in their modules in
milliseconds, from outline JSON shaped like rust-analyzer's (`Object` 19 with `Method` children, `Struct`
23 with `Field` 8 children).

**One live suite**, `same_crate_widening_acceptance`, applies real plans through the runner against
rust-analyzer. Its oracles are `assert_compiles_with_its_tests` and `assert_lints_clean`, which no edit
that merely looks right can satisfy, plus the run's report lines. Fixtures use `same_crate::an_app_holding`;
anchors come from `the_anchor_over`; runs go through `moving_items` / `reparenting_module`.

#### Option 1 (chosen): unit tests in `members.rs`, `tree_visibility.rs`, `tree_reach.rs`, `apply.rs`, `journal/group.rs`
**Trade-off**: exact and fast, but doesn't prove the tree compiles. The live suite does that.

#### Option 2 (chosen): one new live binary
**Location**: `packages/tddy-code-restructuring/tests/same_crate_widening_acceptance.rs`, registered per rule 7.

#### Option 3 (rejected): grow the existing `move_item_*` / `reparent_module_*` binaries
That would mix this node's red tests into suites other wave-1 nodes edit, and lengthen the slowest binaries.

### Coverage Requirements

- [ ] Happy: field and method in both directions; struct literal; old-parent item; ancestor item;
  `pub(super)` reaching out; absolute `pub(in …)` into the tree
- [ ] Unchanged: child destination; trait `impl` members; `pub`/`pub(crate)`/relative members; a child's
  `pub(super)` inside the tree; an ancestor shared by both locations
- [ ] Refusal: an unreadable `pub(…)` in a moved file names the file and line
- [ ] Directories: emptied chain removed up to the first non-empty; untouched empty directory kept; rollback
- [ ] Registration: every live binary in both files
- [ ] Actual effects: bytes on disk; `cargo check --all-targets`; clippy

## Acceptance tests

Names read as behaviour specifications. Unless marked, each is **red on `master`**, for the reason given.

### `tddy-code-restructuring` — `packages/tddy-code-restructuring/tests/same_crate_widening_acceptance.rs` (new live binary; one `app` package; `assert_compiles_with_its_tests` + `assert_lints_clean`)

1. `widens_a_private_field_of_a_moved_struct_that_the_impl_left_behind_reads`: `struct Counter { count: u32 }`
   moves `pairing` → `answers`; `impl Counter { fn bump(&mut self) { self.count += 1 } }` stays. The field reads
   `pub(crate) count: u32`, and the run reports `` `Counter::count` private -> pub(crate) ``. *Red*: `E0616` at the gate.
2. `widens_a_private_field_of_a_struct_that_stays_when_its_impl_moves`: the reverse; the field is widened
   in `pairing`. *Red*: `E0616`.
3. `widens_a_private_method_that_stays_when_the_moved_code_calls_it`: a moved `fn tally(c: &Counter)` calls
   private `c.peek()`. *Red*: `E0624`.
4. `widens_a_private_method_of_a_moved_impl_that_the_code_left_behind_calls`. *Red*: `E0624`.
5. `widens_a_private_field_that_a_function_left_behind_builds_with_a_struct_literal` (`Counter { count: 0 }`).
   *Red*: `E0451`.
6. `leaves_every_member_private_when_the_destination_is_a_child_of_the_source`: destination `pairing::inner`;
   the struct and field text are byte-identical apart from the move, and no member report line appears.
   *Red*: rust-analyzer resolves, but the test also asserts the progress line `surveying 1 member(s) of the
   types the move splits`, which does not exist yet.
7. `never_touches_the_members_of_a_trait_impl`: a moved `impl Display for Counter` reading a field that
   stays; only the field is widened, and the `fmt` line is byte-identical. *Red*: `E0616`.
8. `surveys_only_the_members_the_moved_lines_mention`: a kept struct with five private fields, one of which
   the moved lines read. One field is widened, and the progress line reads `surveying 1 member(s) of the
   types the move splits`. *Red*: `E0616`, and no member survey.
9. `widens_a_private_item_of_the_old_parent_that_the_moved_tree_names`: `fn host_name` private in `host`;
   `attachments` calls `super::host_name()` and is reparented under `split`. The result is
   `pub(crate) fn host_name`, reported. *Red*: `E0603`.
10. `widens_a_grandparents_private_item_but_not_one_of_an_ancestor_both_places_share`:
    `app::a::b::host::attachments` → `app::a::split`, reaching private items of `b` (widened to the scope `a`, spelled `pub(super)` in `a::b`)
    and of `a` (untouched). *Red*: `E0603`.
11. `widens_a_pub_super_item_of_the_moved_module_that_its_old_parent_names`: `pub(super) fn materialize` called as
    `attachments::materialize()` from `host` becomes `pub(crate)` under `split`, reported. *Red*: `E0603`.
12. `respells_an_absolute_pub_in_path_that_points_into_the_moved_tree`:
    `pub(in crate::host::attachments) fn stage` in `attachments/staging.rs` becomes
    `pub(in crate::split::attachments)`. **Green pin** (found by the contract commit): the callers'
    re-pointing already rewrites the path, since rust-analyzer reports the module name inside the
    visibility as a reference. It guards rule 5 against writing the same bytes a second time.
13. `leaves_a_pub_super_inside_the_moved_tree_as_written`: a child's `pub(super) fn stage` is byte-identical
    after the move. **Green pin**: a relative visibility inside the tree means the same after the move, and
    rule 5 must keep it that way.
14. `removes_the_directories_the_move_emptied`: after `host::attachments` (with child `staging`) moves under
    `split`, `src/host/attachments/` and `src/host/` are gone and `src/` remains. *Red*: both directories
    are still on disk.
15. `keeps_a_directory_that_still_holds_a_file_no_declaration_reaches`: `src/host/attachments/fixture.txt`
    stays, and so does its directory. *Red*: compiles today, but the test also asserts `src/host/attachments/staging/`
    (emptied) is gone.

### `tddy-code-restructuring` — `packages/tddy-code-restructuring/src/apply.rs` (`#[cfg(test)] mod tests`, tempdir + `git init`, the existing pattern)

16. `a_rename_removes_the_directory_it_emptied_and_every_empty_ancestor_below_the_root`. *Red*: symbol
    `remove_emptied_directories` missing, and the directories remain.
17. `a_rename_keeps_a_directory_that_still_holds_a_file`. *Red*: missing symbol.
18. `an_empty_directory_no_rename_vacated_is_left_alone`. *Red*: missing symbol.

### `tddy-code-restructuring` — `packages/tddy-code-restructuring/src/journal/group.rs` (`#[cfg(test)] mod tests`)

19. `restoring_a_file_the_group_created_removes_the_directory_that_emptied`. *Red*: the directory remains.
20. `restoring_a_renamed_file_recreates_the_directory_the_sweep_removed`. A **green pin** of `create_dir_all`
    in `restore`.

### `tddy-code-restructuring` — `packages/tddy-code-restructuring/src/backends/rust/item_move/members.rs` (`#[cfg(test)] mod tests`; outline JSON fixtures)

21. `a_moved_private_field_is_widened_where_it_lands_to_cover_the_module_left_behind`. *Red*: module missing.
22. `a_kept_private_method_is_widened_where_it_stays_to_cover_the_destination`.
23. `a_member_whose_users_all_sit_under_its_new_module_keeps_its_visibility`.
24. `a_field_gets_its_keyword_written_before_its_name_after_any_same_line_attribute` (`count: u32`,
    `#[allow(dead_code)] count: u32`, `pub(super) count: u32` widened to `pub(crate)`).
25. `reads_the_fields_of_a_struct_and_the_members_of_an_inherent_impl_but_nothing_of_a_trait_impl_or_an_enum`.
26. `a_tuple_struct_contributes_the_fields_the_outline_lists`. This is an **outline probe**: the fixture is
    the outline rust-analyzer returns for `struct Id(u32);`, captured at M3 (the contract commit assumes a
    struct symbol with no field children, marked `TODO`). It is red until `members_of` exists, then a green
    record of the limit `feature/reshape/move-widen`'s tuple-field todo describes; if rust-analyzer lists
    `0`, the member is surveyed like a named one and that todo narrows.

### `tddy-code-restructuring` — `packages/tddy-code-restructuring/src/backends/rust/module_reparent/tree_visibility.rs` (`#[cfg(test)] mod tests`)

27. `widens_a_scope_that_reached_outside_the_tree_and_leaves_an_absolute_path_into_it_to_the_re_pointing` (renamed from the plan's `translates_a_scope_inside_the_tree_…`, which would have written the bytes the re-pointing writes). *Red*: `todo!()` in `respelled`.
28. `leaves_pub_pub_crate_and_a_childs_pub_super_untouched_and_refuses_an_unreadable_visibility_naming_its_line`.

### `tddy-code-restructuring` — `packages/tddy-code-restructuring/src/backends/rust/module_reparent/tree_reach.rs` (`#[cfg(test)] mod tests`)

29. `surveys_the_old_parent_and_the_ancestors_below_the_common_ancestor_and_nothing_above_it`
    (`host`→`split`: `[host]`; `a::b::host`→`a::split`: `[a::b::host, a::b]`; `a::host`→`a::host::x`: `[]`).
    *Red*: module missing.

Registration (rule 7) is checked by `./dev bun test ./scripts/nextest-serial-groups.test.ts` (every named
binary exists) and by the e2e leg picking the suites up. It is not a test of this package.

## Technical Debt & Production Readiness

(empty; populated during development)

## Decisions & Trade-offs

Taken by the developer (brief, 2026-10-09): "All 12 wave-1 PRDs approved. Every node's own F-decisions:
take the agent's recommendation unless overridden"; "The unregistered live rust-analyzer suites … → node 1
registers all of them"; "Tuple-struct fields not widened: ONE todo, written by node 7 … node 1 references
it"; "Node 1 defers `pub use` chain widening (no reproduction)".

Decided, following the recommendations in the PRD review:

- **F1, widen or refuse a split member:** widen, as items are widened; `move-impl-members` needs it.
- **F2, a tree `pub(super)` that reached outside the tree:** keep what it covered and widen it until
  legal. It needs no references and can't break a caller the server did not report (a cfg-gated one). The
  narrowest-from-references alternative is rejected.
- **F3, report format:** `Type::member`.
- **F4, reparent survey depth:** the old parent plus the ancestors below the common ancestor.
- **F5, test placement:** one new binary; all unregistered live suites registered (now the developer's decision).
- **F6, the rolled-back group's empty directory:** fixed here with the same sweep.

**OPEN** (each with a recommendation):

- **F7, a member used by an `impl` of the moved type in *another* file.** (a) **leave it to the compile
  gate and record it in the narrowed 10-04 entry**, recommended because the survey stays bound to one file,
  like item reach; (b) survey every `impl` of the type the server finds (one more references request per
  type, plus `goto_implementation`).
- **F8, where `landing_of` lives.** (a) **in `members.rs`**, recommended because `assemble.rs` gains no
  line; (b) beside `visibilities` in `assemble.rs` (+15 lines to a file already over budget).

Decisions taken by this plan: no plan field; `rebase.rs` behaviour unchanged; no narrowing; the dry-run
overlay does not model directories.

## Refactoring Needed

### From @ft-dev (Acceptance Test Creation)
(empty)

### From @red (TDD Red Phase)
(empty)

### From @validate-changes (Change Validation)
(empty)

### From @validate-tests (Test Quality)
(empty)

### From @prod-ready (Production Readiness)
(empty)

### From @analyze-clean-code (Code Quality)
(empty)

### From @refactor (Completed Refactorings)
(empty)

## Validation Results

### Contract commit (2026-10-09)

- **Gates, scoped:** `cargo check -p tddy-code-restructuring --all-targets`, `cargo clippy -p
  tddy-code-restructuring --all-targets -- -D warnings` and `cargo fmt --all --check` are clean.
- **Package baseline before this commit:** `./test -p tddy-code-restructuring` on commit 1 gave
  1334 passed, 0 failed, 1 ignored (a doc-test that is `ignore` by design). No pre-existing failure.
- **Live suite** `tests/same_crate_widening_acceptance.rs` (live rust-analyzer, 88 s): 13 red, 2 green.
  - Red at the compile gate, each naming the error the rule removes: 1 (`E0616`), 2 (`E0616`),
    3 (`E0624`), 4 (`E0624`), 5 (`E0451`), 7 (`E0616`), 8 (`E0616`), 9 (`E0603`), 10 (`E0603`),
    11 (`E0603`).
  - Red on assertion: 6 (no `surveying 1 member(s) of the types the move splits` progress line),
    14 (`src/host/attachments` left on disk), 15 (`src/host/attachments/staging` left on disk).
  - Green pins: 12 and 13. Rust-analyzer reports the module name inside `pub(in crate::host::attachments)`
    as a reference, so the re-pointing already rewrites it on `master`. Rule 5 and test 27 were narrowed
    to match.
- **Unit tests** (`cargo test -p tddy-code-restructuring --lib`): 13 red, 1 green pin.
  - Red at `todo!()`: 17, 18 (`remove_emptied_directories`); 21-26 (`members_of`, `widen_members`,
    `member_visibility_edit`); 27-28 (`respelled`); 29 (`modules_to_survey`).
  - Red on assertion: 16 (`src/host` left on disk after a rename), 19 (`src/split` left on disk after a
    rollback).
  - Green pin: 20.
- **Registration:** 13 binaries added to `.config/rust-e2e.filterset` and 32 to the `rust-analyzer`
  group, as rule 7 lists. `apply_tidy_acceptance` and `apply_compile_gate_acceptance` are in the e2e set
  but not the group: they run `cargo`, not rust-analyzer.

## TODO

- [x] Record initial discovery (`2026-10-09-reshape-widen-same-crate-initial-discovery.md`)
- [x] Cross-check `packages/*/docs/code-issues/` and `docs/dev/todo/` for items this change touches (Step 2b)
- [x] Create/update PRD documentation (`docs/ft/coder/1-WIP/PRD-2026-10-09-reshape-widen-same-crate.md`)
- [x] Create changeset (this document)
- [ ] Add the PRD reference to `docs/ft/coder/1-OVERVIEW.md` **at wrap** (a shared append-point: not edited while planning)
- [x] Create the draft-PR contract surface (commit 2)
- [x] Create failing acceptance tests
- [x] Run acceptance tests (verify they fail)
- [ ] USER REVIEW — acceptance tests
- [x] TDD Red — write failing unit/integration tests
- [ ] TDD Green — implement with quality code
- [ ] Update documentation with progress
- [ ] Repeat Red→Green→Update cycle until feature complete
- [ ] Run the scoped tests (`./test -p tddy-code-restructuring`) — verify 100% pass; CI answers for the rest of the workspace
- [ ] Validate changes (/validate-changes)
- [ ] Refactor issues from change validation
- [ ] USER REVIEW — development complete
- [ ] Validate tests (/validate-tests)
- [ ] Refactor test issues
- [ ] Validate production readiness (/validate-prod-ready)
- [ ] Refactor production readiness issues
- [ ] Analyze code quality (/analyze-clean-code)
- [ ] Refactor code quality issues
- [ ] Final validation (/validate-changes)
- [ ] Linting and formatting (`cargo clippy -p tddy-code-restructuring --all-targets -- -D warnings`, `cargo fmt`)
- [ ] Wrap documentation (/wrap-context-docs) — when the PR is set ready for review. It deletes
  `2026-10-09-reshape-widen-same-crate-initial-discovery.md` and the reparent-reach todo, narrows the
  move_item-widening, lifecycle-limits and reparent-first-cut todos, and closes the feature doc's
  rolled-back-directory limit
- [ ] USER REVIEW — work complete, decide next steps
