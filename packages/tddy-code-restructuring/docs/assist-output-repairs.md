# What the Rust backend does to an assist's output

rust-analyzer's "extract into module" assist moves the text; it does not leave a crate that builds.
Between the assist and the rename of its placeholder module, `RustBackend` runs three passes over
the produced text: the **import pass** restores the names the cut stranded, and two lexical repairs
undo rewrites the assist writes that are not Rust. Two further steps shape `extract_method` and
`extract_variable`: a function-local `use` the new function needs is carried into it, and the range
`extract_variable` hands the assist is chosen so the assist binds a borrow as a borrow. Each pass
either makes the result correct or refuses the operation by name; none writes a result it cannot
vouch for.

| Module (`src/backends/rust/`) | Holds |
|---|---|
| `imports.rs` (with `imports/bound_names.rs`, `imports/local_uses.rs`) | `restore_imports`, `next_import`, the verified reconstructions, the seam-lost filter, `names_bound`, `rebased_for_child`, the function-local `use` carry |
| `prelude_shadow.rs` | the lexical carry of a parent's `use`-bound name the moved code uses, even when it shadows a prelude name |
| `relative_visibility.rs` | rebasing a relative visibility one module deeper |
| `inline_paths.rs` | re-rooting inline `super::` / `self::` paths and paths through a module the parent declares |
| `impl_seam.rs` | `refuse_impl_sibling_references`, `is_inherent_impl`, `with_method_calls_restored`, `reached_through_the_type` |
| `nested_modules.rs` | `with_nested_references_restored`, and the brace-depth module reader behind it |
| `introduced.rs` | finding the `let` binding `extract_variable`'s assist introduced, so it can be renamed to the plan's `name` |
| `selection.rs` | the position `extract_variable` probes, the widening of a borrowed place to its borrow, and the by-value refusal |

`imports.rs` also holds the carry of function-local `use` items (`carry_function_local_uses`).

`backends/rust.rs` wires these in. It keeps `choose_import`, `already_bound`, `expand_use` and
`collect_aliases`.

## The import pass

### Which names it weighs

The pass is driven by rust-analyzer's `unresolvedReference` semantic tokens, and it weighs only the
names **the seam lost**:

- every unresolved occurrence inside the produced module;
- an unresolved occurrence in the **parent**, but only for a name the file resolved everywhere before
  the cut.

The second rule is what restores a name the parent loses to the cut, such as a trait moved out while
the parent still writes `impl Named for Thing`, which the assist does not rewrite. The baseline is
read once, with one `semanticTokens` request against the original text. A name that was already
unresolved before the cut is ignored, so an item the server cannot see at all (code behind
`#[cfg(not(rust_analyzer))]`, or `OUT_DIR` code a failed build script never produced) does not send
the pass looking for an import it cannot find.

### What a `use` binds

`names_bound` reads a `use` declaration the way the compiler does: `use a::B as C;` binds `C`,
`use a::B as _;` binds nothing, and aliases inside nested groups (`use a::{b::{C as D}}`) bind their
alias. The pruning pass reads bindings through the same function, so the two cannot disagree about
whether a name is already in scope.

### Choosing and verifying an import

1. **An offered import.** rust-analyzer's `Import` code action at the unresolved name. Several
   offers are settled by `choose_import`'s tiers (an exact binding the file already has, then the
   module it imports from, then the crate it binds that same name from), else refused naming the
   candidates. The evidence is the **pre-assist file and the current text read together**
   (`Seam::evidence_with`): the assist removes a grouped binding such as `mpsc` from
   `use tokio::sync::{…, mpsc, …};` when the seam held its only use, and the current text alone would
   then offer `std::sync::mpsc` and `tokio::sync::mpsc` as equal evidence. The original holds the
   binding the assist removed; the current text holds what earlier passes restored.
2. **A reconstruction from the parent.** Where no import is offered (an alias, a module binding),
   the pass rebuilds the parent's own declaration for the name, in the alias form or the plain
   parent-binding form.
3. **Rebased for the child.** The produced module is the parent's child, so a relative path is one
   level off. `rebased_for_child` rewrites `super::X` as `super::super::X` and `self::X` as
   `super::X`; `crate::`, `::` and extern-crate paths are left alone. Grouped trees are read one flat
   path per member, so a group member is rebased the same way.

Every candidate, offered or reconstructed, is **verified**: it is applied only if the unresolved
occurrences of its name drop. A reconstruction writes into the module, so it answers only for
occurrences inside it. When a name's count does not drop, the name is marked unimportable and the
pass ends with a `SeamRefused` naming the name and the declaration it tried, for example *"left 3
unresolved occurrence(s) of it, where there were 3"*. The pass is bounded by `IMPORT_PASSES` (512),
which a pass that makes progress on every round never reaches.

A bare path through an item the parent declares (`sibling::X`, 2018 uniform paths) cannot be told
from an extern crate by reading, so it is left as written, and verification refuses it by name.

## Code that changes meaning one module deeper

The assist moves text into a child of the module that held it. Three things written in the parent mean
something else in the child, and none of them is something rust-analyzer reports unresolved.

**A parent's `use` that shadows a prelude name.** A parent that binds `Result` to the crate's
one-generic alias gives moved code the alias; in the child the prelude's two-generic
`std::result::Result` resolves the same word, so the child silently means a different type and the
compiler is the first to say so. `prelude_shadow.rs` reads the parent's `use` declarations and the
moved text, and carries across every binding the moved code names that the module does not already
have. It over-imports on purpose; [the tidy](readiness-and-gates.md#the-tidy) removes what is unused.
Alongside it, a type the move widened stays widened when a widened item's signature names it.

**Relative visibility.** `pub(super)` written in `imports` means "visible in the parent"; the same text
in the child means "visible in `imports`". `rebased_for_child` rewrites it one level deeper:
`pub(super)` becomes `pub(in super::super)`, `pub(self)` becomes `pub(super)`, and a relative
`pub(in …)` path gains a `super::`. Absolute forms are unchanged. The rewrite applies to a moved item
and to the fields of a moved struct.

**Inline paths.** `inline_paths::rerooted_module` re-roots the `super::` and `self::` paths written
inline in the moved code (calls, types, struct literals, patterns), and writes a path whose first
segment is a module the parent declares as `super::<module>::…`, unless the moved code could bind that
name itself. Only code is read: strings, characters and comments are masked, and a `use` item is left
to the imports pass. The run reports `paths: N path(s) re-rooted for the new module`.

## A function-local `use` the extracted function needs

rust-analyzer writes an extracted function beside the one it came from, where that function's own
`use` items are not in scope. `carry_function_local_uses` runs for an `extract_method` whose new
function is not nested in its origin: each `use` inside the origin function that binds a name the
range mentions is written, as written, at the top of the new function's body, so it stays local and
means what it meant. A `use` the range itself holds travelled with it and is not copied again. The
words of the range are read over text with comments and strings masked, and a `use` is recognised
only at the start of a statement, so one in a comment or a string is not one.

The origin's own `use` is left as written. When nothing in the origin names the binding any more, it
is an unused import, which the compiler reports as a warning, not an error. `check` reports the
items it would carry on its progress line; that is information, not a finding.

## Cutting through an `impl`

`refuse_impl_sibling_references` decides whether a seam may cut an `impl`:

| Cut | Outcome |
|---|---|
| Through an **inherent** `impl`, with `self.method()` calls to members that stay | allowed. A method call resolves through the type wherever its `impl` lives, and rust-analyzer writes `mod m { use super::T; impl T { … } }` |
| Through a **trait** `impl` that siblings reference | refused, naming the `impl` and the reason (E0119 / E0046: a trait is implemented once, whole) |
| Through an `impl` the outline survey did not name | refused: the backend cannot tell which kind it is |

The outline names the block as `impl Meter for Gauge`, which is how a trait `impl` is told apart. A
trait `impl` cut with **no** sibling reference is not refused (see the known limitations below).

### The assist's `modname::` rewrites

Once an inherent member moves, the assist rewrites every call left behind by inserting its
placeholder module name right before the member's name. None of these forms is Rust, and the
rename cannot reach them:

| What the assist writes | Written by | Undone to |
|---|---|---|
| `self.modname::doubled()` | a method call from a member left behind | `self.doubled()` |
| `Self::modname::doubled(self.level)` | an associated-function call from a member left behind | `Self::doubled(self.level)` |
| `Gauge::modname::doubled(2)`, `Meter::modname::doubled(2)` (type alias) | a call through the type, for example from the file's `mod tests` | `Gauge::doubled(2)`, `Meter::doubled(2)` |

`with_method_calls_restored` removes the placeholder wherever it directly follows a `.`, or an
identifier qualifier other than `super`, `self` and `crate`, and precedes a moved inherent member's
name as a whole word. Those three qualifiers reach a moved **free** item and are left to the rename.
An associated function is reached through its type wherever its `impl` lives, and a module path
cannot name one (`reached_through_the_type`). A bare `modname::f` for an associated function is left
alone, because nothing says which type it was called through.

The assist does not rewrite a reference inside a macro call (`assert_eq!(Gauge::doubled(2), 4)`), so
those need no repair.

A private method keeps the assist's `pub(crate)`. `impl_widenings` reports it as a widening, and
nothing narrows it back.

## A reference inside a module the file already had

When the file has its own module that imports the moved item:

```rust
mod tests {
    use super::base;
    fn reads() { let read = base(); }
}
```

the assist repoints the import to `use super::modname::base;` **and** rewrites the call to
`modname::base()`. `modname` is a child of the file's module, not of `tests`, so the call names
nothing. `with_nested_references_restored` removes the placeholder from a path it starts, in a module
other than the placeholder's own whose `use` declarations bind the moved name. The assist rewrites
only references to what it moved, so that binding is the one the call resolved through. With
`use super::*;` instead, the rewritten call resolves through the glob and the rename finishes it.

The module blocks are read lexically, by brace depth over text whose comments, strings and character
literals are masked with the same lexer the test-binary move uses (`readable_spans`, through
`early_return::masked_to_code`).

When a placeholder is still left over after the repairs, the refusal names both cases it cannot tell
apart lexically: reorder the plan for a module an earlier operation extracted; for a module the file
already had, reach the item through `use super::*;` or cut the seam elsewhere.

## Naming the binding `extract_variable` introduced

Most assists write a fixed placeholder the engine renames to the plan's name (`fun_name`, `modname`,
`NewTrait`). rust-analyzer's "extract variable" writes none: it names the binding from the expression
(`suggest_name::for_variable`, so a read of `self.clones` becomes `let clones`). So the placeholder
for this operation has no name (`Placeholder::name` is `None`: the server names the symbol), and
`introduced.rs` finds the binding by what the assist added:

- a `let` whose name is declared more often after the assist than before, counted over text with
  comments and literals masked, so a `let` the assist only rewrote does not count;
- sitting in the span the assist changed, which picks the right declaration when the new binding
  shadows an older one of the same name.

That binding is renamed to the plan's `name` with the same server rename the other placeholders use,
and left as it is when rust-analyzer already chose that name. The operation is refused as
`rust-analyzer's answer was unusable:` only when the assist introduced no `let` binding, or more than
one. The leftover-placeholder check then runs against the name actually renamed. Two limits are
rust-analyzer's: one operation replaces **one** occurrence of the expression, and the assist decides
between `&self.x` and `self.x` from the autoref it sees (see
[plan-schema.md](../../../.agents/skills/code-restructuring/references/plan-schema.md)).

## Where `extract_variable` asks, and what it binds

Two defects came from taking the range exactly as written.

**The probe position.** The type probe hovers at the first position of the range that can carry a
hover (`hover_bearing_position`): past leading `&`, `&mut`, `*`, `!`, `-` and `(`, and whitespace
after them. On a `&` the hover is `null` however long the index has been ready. The position is read
on the range's first line and never past the range's end; a range of nothing but such punctuation
keeps its start. The wait is bounded; see
[readiness-and-gates.md](readiness-and-gates.md#the-probe-is-bounded).

**A borrowed place is bound as the borrow.** The assist binds what is selected by value, so `self.v`
selected inside `&self.v` becomes `let x = self.v;`, a move out of the borrow (`E0507`) whenever the
type is not `Copy`. When the selection is a place (a field access or a path) that is the whole
operand of a unary `&` or `&mut` written on the same line, the selection is widened to include the
borrow, and the binding is `let x = &self.v;`. Where the borrow is on another line, or the range
spans lines, it is not widened, and `refuse_by_value_hoist` refuses the selection as
`rust-analyzer's answer was unusable:`, naming both positions and advising to select the borrow
including its `&`.

That refusal is **structural**. It asks whether a place sits directly under a `&` or `&mut` the
selection omits, from the text alone, and does not ask whether the type is `Copy`: there is no type
information at that point, and a by-value hoist of a borrowed place is never what the borrow meant.
So a `Copy` place under a borrow is refused too; selecting the borrow, `&` included, is always
accepted. A place that is the receiver of a call, an index, a method or a `?` is not the borrow's
operand and is not refused.

## Known limitations

- **A generic qualifier is not undone.** `Foo::<T>::modname::f` is left as written and refused.
- **A free item and an inherent member with the same name, moved together**, could let the
  type-qualifier rule strip a legitimate `file_module::modname::f`. Not seen, not guarded.
- **The seam-lost baseline is name-level.** A name already unresolved anywhere before the cut is not
  weighed in the parent either, even where the seam strands a new occurrence of it. A per-name count
  would close that.
- **The parent-binding reconstruction's `super::` rebase is not proven live.** In a fixture,
  rust-analyzer offers `super::super::X` itself, so the pass never reaches that fallback. Only the
  alias form is reproduced against a live server; the plain form's test is a guard.
- **A trait `impl` cut with no sibling reference is not refused**, though it is E0119 all the same.
- **A borrow on another line is refused, not widened.** The widening is single-line; a selection
  whose `&` is on an earlier line is refused by the by-value check, so the plan selects the borrow.
- **The carried `use` is removed from the origin only if rustc reports it unused**, by [the
  tidy](readiness-and-gates.md#the-tidy); a `use` it cannot prove unused stays.
- **The module reader is lexical.** It counts braces over masked text rather than asking the server
  for the module tree, so a shape the lexer does not know (a brace a macro produces, for example)
  could mislead it. The compile gate on `apply` (see [readiness-and-gates.md](readiness-and-gates.md))
  catches what that leaves.
- Attribute macros, `use Trait as _;` and other gaps the compile gate reports are recorded in the
  backlog, not here.
