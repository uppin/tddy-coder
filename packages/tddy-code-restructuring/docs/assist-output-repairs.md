# What the Rust backend does to an assist's output

rust-analyzer's "extract into module" assist moves the text; it does not leave a crate that builds.
Between the assist and the rename of its placeholder module, `RustBackend` runs three passes over
the produced text: the **import pass** restores the names the cut stranded, and two lexical repairs
undo rewrites the assist writes that are not Rust. Further steps shape `extract_method` and
`extract_variable`: a function-local `use` the new function needs is carried into it, the function
`extract_method` writes is cleaned up (its comments, signature and return type), and the range
`extract_variable` hands the assist is chosen so the assist binds a borrow as a borrow. Each pass
either makes the result correct or refuses the operation by name; none writes a result it cannot
vouch for.

| Module (`src/backends/rust/`) | Holds |
|---|---|
| `imports.rs` (with `imports/bound_names.rs`, `imports/local_uses.rs`) | `restore_imports`, `next_import`, the verified reconstructions, the seam-lost filter, `names_bound`, `rebased_for_child`, the function-local `use` carry |
| `prelude_shadow.rs` | the lexical carry of a parent's `use`-bound name the moved code uses, even when it shadows a prelude name, and of every `use <path> as _;` the parent has |
| `relative_visibility.rs` | rebasing a relative visibility one module deeper |
| `inline_paths.rs` | re-rooting inline `super::` / `self::` paths and paths through a module the parent declares |
| `projection.rs` | the files an earlier operation of the run wrote, and showing them to the server before the next operation's first document |
| `impl_seam.rs` | `refuse_impl_sibling_references`, `is_inherent_impl`, `with_method_calls_restored`, `reached_through_the_type` |
| `nested_modules.rs` | `with_nested_references_restored`, and the brace-depth module reader behind it |
| `introduced.rs` | finding the declaration an assist introduced and named itself (a `let` for `extract_variable`, a `fn` for `extract_method`), so it can be renamed to the plan's `name` |
| `selection.rs` | the position an extraction probes, the widening of a borrowed place to its borrow, and the by-value refusal |
| `extracted_fn.rs` | `cleaned_extraction`, which runs the rules below over the function `extract_method` wrote; `extract_method_findings` and `refuse_unliftable_returns`, the static `check` side |
| `extracted_fn/span.rs`, `signature.rs` | where the introduced function sits, and its parameters and return type, read over masked text |
| `extracted_fn/comments.rs`, `lints.rs`, `ptr_args.rs`, `respell.rs` | the comment carry, the lexical lint clean-ups, the `ptr_arg` narrowing, the respelling of an unimported signature type |
| `extracted_fn/guards.rs`, `tail.rs`, `return_type.rs` | the guard lift, tail position, and the return type spelled as the caller spells it |

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

**A trait imported only for its methods.** `use prost::Message as _;` binds no name, so moved code
that calls `frame.encode_to_vec()` never names it and nothing reports it unresolved, yet the child
does not compile without it (`E0599`). `anonymous_trait_imports` returns every `use <path> as _;` of
the parent, a member `<path> as _` of a group included (flattened to its own line), rebased for the
child like any other carried import, and `shadowed_imports` carries each unless the child already has
that line. It is written in the same pass as the prelude-shadowing imports, so a seam that calls no
trait method receives an import it does not use, which the tidy removes. A **named** trait import
(`use prost::Message;`) that the moved code needs only for method resolution is not carried; see the
known limitations.

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

## The function `extract_method` writes

rust-analyzer's "extract into function" compiles and still leaves CI's lint job red, loses comments
that say *why*, and names types the file does not import. `RustBackend::cleaned_extraction` runs after
the rename and cleans up **the introduced function only**: the `fn <name>` the rename produced, from its
keyword to its closing brace, read over text with comments and literals masked. The rest of the file,
and the function the range came from, are untouched except for the one call line the assist wrote.
The steps run in this order:

1. the function-local `use` carry (above);
2. the guard lift, when the range holds `return`s (below);
3. the comment carry;
4. the respelling of an unimported signature type;
5. the `ptr_arg` narrowing;
6. the lexical clean-ups and the shape notes;
7. the caller's return-type spelling.

`check` and `check --deep` run the static half before any server starts (`extract_method_findings`):
the refusal of a range holding a `return` no rule carries, the function-local `use` items that will be
carried, and the comment count below, each on the progress line. Those lines are information, not
findings.

### Comments

The assist rebuilds the body's statement list when the range holds a `?`, and loses the comments
between statements and after them; a comment inside a statement survives. `comments::dropped` lists
the `//` and `/* */` comments of the range that the produced function lacks, as a multiset of trimmed
text. Each is put back by pairing the range's top-level statements (split on `;` and on a closing `}`
at depth zero, over masked text) with the produced body's, **in order**: the assist keeps the order
and appends one tail statement when the range has outputs.

| Comment | Written |
|---|---|
| a full-line comment, or a run of them | before the produced statement paired with the range statement it preceded, at that statement's indentation |
| a trailing comment | after one space, at the end of the last line of the produced statement paired with its own |

A statement count that does not match, or a comment anchored to the range's last statement when the
produced last statement is the assist's tail, cannot be paired. The operation is refused as
`rust-analyzer's answer was unusable:`, naming every comment it would lose, and nothing is written.
`check` counts the comments between or after top-level statements of a range holding a `?` and says
`the extracted function will keep N comment(s) rust-analyzer drops`.

### Signature and body clean-up

| What the assist wrote | What the function has | How it is known to be safe |
|---|---|---|
| `p: &PathBuf`, `s: &String`, `v: &Vec<T>`, qualified or not | `&Path`, `&str`, `&[T]`; `Path` is bare where the file binds it by `use`, `std::path::Path` otherwise. In the body `p.clone()` becomes `p.to_owned()`. No `use` is added | the server's pull diagnostics, below |
| the tail `Ok(if …)`, `Ok(match …)`, `Ok(loop …)`, `Ok({ … })` (and `unsafe {`, `while`, `for`) of a function returning `Result<(), _>`; `Some(…)` for `Option<()>` | the expression as a statement, then `Ok(())` / `Some(())` | the return type carries `()`, so the wrapped expression is `()` (`clippy::unit_arg`) |
| `field: field` inside a struct literal | `field` | the same binding by definition (`clippy::redundant_field_names`) |
| more than seven parameters, or a tuple return of three or more | unchanged, plus a note: `` `<name>` takes N parameters; clippy::too_many_arguments fires above 7 `` / `` `<name>` returns a N-tuple; clippy::type_complexity may fire `` | grouping parameters is a naming decision, so it stays the plan's |

`&mut PathBuf`, `&mut String` and `&mut Vec<T>` are not narrowed: they need the owned type.

**Narrowing is verified.** `verified_narrowings` sends the narrowed text to the server (`didChange`)
and pulls diagnostics. A new type mismatch (`E0308`) inside the introduced function or on the call line
undoes the narrowing of the parameter that line names, and adds a note: `` kept `p: &PathBuf`: the body
needs the type rust-analyzer wrote ``. The diagnostics are pulled once for all narrowings and once more
after undoing; if the second answer still holds a new mismatch, every narrowing is undone, with one note
per parameter. The server's own output compiles, so keeping its type is always sound.

An unused `mut` the assist copies into the callee (or leaves in the caller) is not handled here:
rust-analyzer reports none, so it is [the tidy's](readiness-and-gates.md#the-tidy).

### A type the signature names that the file does not import

When the signature of the introduced function names a type the server reports unresolved, and that
resolved before the assist, `respelled_signature_types` respells it the way the function the range
came from does. It collects the distinct qualified paths ending in `::<name>` in that function's text
(`respell::spellings`):

- **exactly one**: every unresolved occurrence in the signature is replaced with it, and the unresolved
  count for that name must drop to zero, or the operation is refused;
- **none or several**: the operation is refused as `this seam cannot be cut here:`, naming the type and
  the spellings found, and advising to import it or cut the range elsewhere.

No `use` is ever added. Only the signature is respelled; the body names what it named.

### A run of guards, and tail position

A range holding a `return` extracts a function whose return type is not the caller's, so the result
does not compile unless the return still returns from the caller. `guards::classify` reads the range
and the enclosing function's header over masked text (a `return` inside a closure, an `async` block or
a nested `fn` does not count) and decides:

| The range holds | Result |
|---|---|
| no `return` | the ordinary path |
| `return`s, and it runs to the function's tail, or is the whole body of a match arm or `if`/`else` branch whose `match`/`if` is itself in tail position, recursively up to the function body (`tail::in_tail_position`) | extracted with its `return`s verbatim; the assist writes the call as the arm's value. A function returning `()` has no tail value, so this does not apply to it |
| only `return Err(<e>)` exits (at any depth inside `if`, `match`, loops), in a function returning `Result<…>`, a path ending `::Result<…>`, or a one-argument alias the file binds | **lifted** |
| only `return None` exits, in a function returning `Option<…>` | **lifted** the same way |
| a value `return` (`return Ok(v)`, `return v`, `return;`) in the middle of the function | refused with the existing early-return refusal, naming the line |
| error guards and a value return together | refused, naming the value return: ``the range holds error guards and a return of a value, on line N (`…`). Only a run whose every return is `return Err(..)` can become a function called with `?`; end the range before line N.`` |
| error guards in a function that returns no `Result` (or `None` guards where it returns no `Option`) | refused, saying there is nothing for `?` to return |
| guards beside a labelled `break` or `continue` | refused: the extracted function could not carry the label |
| guards, and a binding the range declares at its top level that the code after it reads | refused, naming the binding: ``the range's guards are followed by code that reads `<binding>`, which the range declares. Cut the run so it ends before `let <binding>`.`` |

All refusals are `this seam cannot be cut here:`, raised by `check`, `check --deep` and `apply` alike.

**The lift.** A lifted range becomes `fn <name>(..) -> Result<()>` whose returns are the guards' own
`return Err(<e>);`, ending `Ok(())`, called as `<name>(..)?;`. The return type is the caller's spelling
with its first argument replaced by `()` (`Result<()>`, `crate::Result<()>`, `Result<(), String>`).
`?` on `Result<(), E>` inside a function returning `Result<T, E>` is the identity `From`, so the early
exit returns the same value from the same call. `None` guards lift to `Option<()>` ending `Some(())`.

rust-analyzer writes one of two shapes for such a range, and `guards::lifted` accepts exactly these:

- **A range that also holds a `?`.** The server writes the lifted form itself (`-> Result<(), E>`, the
  guards verbatim, the tail `Ok(())`, the call `<name>(..)?;`). It is kept as written; only the return
  type spelling below applies.
- **A range with no `?`.** The server writes `Option<Result<T, E>>`, `return Some(Err(<e>))` in the
  new function, a tail `None`, and `if let Some(value) = <name>(..) { return value; }` at the call. The
  call becomes `<name>(..)?;`, each `return Some(Err(<e>))` becomes `return Err(<e>)`, the tail
  becomes `Ok(())`, and the return type is rewritten as above (`Option<Option<T>>` and `Some(None)` for
  `None` guards).

Any other shape (no `if let Some` and no `?` call, a return not wrapped in `Some(`, a different tail)
is refused as `rust-analyzer's answer was unusable:`, naming what was expected, and nothing is written.

### The return type is spelled as the caller spells it

This applies to **every** `extract_method`, lifted or not, including a plain range that propagates with
`?` and holds no `return`. When the caller's return type is spelled `<path>Result<T>` with one generic
argument (this crate's `crate::Result`, `anyhow::Result`, a module's own `Result<T>`), and rust-analyzer
wrote the new function's as `<any path>Result<X, E>`, the new type is `<path>Result<X>`. An alias under
another name (`Res<T>`) is read the same way when the function propagates with a `?` or a `return`,
because the server prints every alias expanded. A caller written `Result<T, E>`, an `Option`, or any
return type that is not a `Result` changes nothing.

The reason is `E0107`: where the alias is **imported**, the two-argument form names a type that takes
one argument. Where the alias is defined in the same module, the server writes
`std::result::Result<X, E>` or `Result<X, E>`, which fails the same way; both are respelled. `E` is not
compared: the function's `?`s and returns propagate into the caller's error type, so the server's `E`
is the alias's.

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

## A seam after a seam

The passes above read the tree around the anchor: `reach_of` counts a reference in another file as
one from outside, `restore_imports` skips a name already unresolved before the cut, and
`restore_visibility` keeps an item widened that something outside the new module reaches. They are
only as good as the server's picture of that tree, and the server is shown the anchor's file alone. A
seam cut after another one is about a tree whose first seam's file the server has never been shown: in
a rehearsal that file exists only in the overlay, and in an apply rust-analyzer's own watcher does not
see it within one request.

`projection.rs` closes that. `RustBackend` holds a `PlanProjection`, the paths its resolutions created,
changed or renamed into, in the order the run first wrote each. After a successful `resolve`, the
edit is folded in: a created or changed path is added once, and a rename removes the old path and adds
the new. A refused operation records nothing. At the start of `resolve`, every recorded path is read
through the workspace (so the overlay's text in a rehearsal and the disk's in an apply) and staged. The
first `did_open` of the operation sends `textDocument/didOpen` for every staged document except the
one it is opening. A later `did_open` of a uri already open from the projection sends the requested
text as `textDocument/didChange` rather than a second `didOpen`. A recorded path the workspace cannot
read is an error naming it. `closing_what_it_opens` closes the projected documents with the rest and
clears the staging, so nothing staged for one entry point reaches the next on a shared server.

Nothing in the passes changes for it: they get a server that answers for the projected tree.
`restore_visibility` reports a widening only for an item that was not already `pub(crate)`
(`widening_of`), since a change from `pub(crate)` to `pub(crate)` is not one.

## Naming what an extraction introduced

Most assists write a fixed placeholder the engine renames to the plan's name (`modname`, `NewTrait`).
Two write none. rust-analyzer's "extract variable" names the binding from the expression
(`suggest_name::for_variable`, so a read of `self.clones` becomes `let clones`), and its "extract into
function" names the function `fun_name` only when the range is a statement run: a range that is a
`let`'s initializer gets the binding's name (`fn managed`). So the placeholder of both operations has
no name (`Placeholder::name` is `None`: the server names the symbol), and `introduced.rs` finds the
declaration by what the assist added (`introduced_declaration`, keyed on the placeholder's keyword,
`let` or `fn`):

- a declaration of that keyword whose name is declared more often after the assist than before,
  counted over text with comments and literals masked, so a `let` the assist only rewrote does not
  count;
- sitting in the span the assist changed, which picks the right declaration when the new one shadows
  an older one of the same name.

That declaration is renamed to the plan's `name` with the same server rename the other placeholders
use, and left as it is when rust-analyzer already chose that name. The operation is refused as
`rust-analyzer's answer was unusable:` only when the assist introduced no such declaration
(``rust-analyzer's extraction introduced no `fn` to name``), or more than one. The leftover-placeholder
check then runs against the name actually renamed. Two limits are
rust-analyzer's: one operation replaces **one** occurrence of the expression, and the assist decides
between `&self.x` and `self.x` from the autoref it sees (see
[plan-schema.md](../../../.agents/skills/code-restructuring/references/plan-schema.md)).

## Where an extraction asks, and what `extract_variable` binds

Two defects came from taking the range exactly as written.

**The probe position.** The type probe of an extraction that needs inference (`extract_method`,
`extract_variable`) hovers at the first position of the range that can carry a hover
(`hover_bearing_position`): past a leading `{`, `&`, `&mut`, `*`, `!`, `-` and `(`, past `//` and
`/* … */` comments, and past whitespace, across lines. On a `&`, a `{` or inside a comment the hover is
`null` however long the index has been ready. The position is never past the range's end; a range of
nothing but such tokens keeps its start. The range handed to the assist is unchanged. The wait is
bounded; see [readiness-and-gates.md](readiness-and-gates.md#the-probe-is-bounded).

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
- **The projection is not compiled.** `check --deep` shows the server the tree the plan would produce
  and answers its questions for it, but does not type-check that tree; only `apply`'s gate does.
- **A file an earlier operation renamed away is not hidden from the server** in a rehearsal, where it
  is still on disk.
- **Widening is to `pub(crate)`**, where `pub(super)` would suffice for a reference from the original
  parent and below.
- **A `return` a macro expands to is not seen** by the guard classification (`bail!`), and neither is a
  `break` or `continue` that leaves the range; the compile gate reports what they leave.
- **A guard run that declares a binding the code after it reads is refused**, not lifted with an output.
- **A named trait import used only for its methods is not carried** by `extract_module`; only
  `use … as _;` is.
- **Three shapes rust-analyzer writes for some ranges do not build** (a `*` before a macro statement, a
  value moved while a reference into it is passed, an expression range after `return`); the compile gate
  names the error.
- Attribute macros and other gaps the compile gate reports are recorded in the backlog, not here.
