# Research: a restructure operation that expands a macro into real code

**Date:** 2026-09-09
**Question:** can `tddy-tools restructure` gain an operation that expands `#[async_trait]` (and
macros generally) into the code the compiler generates, so later refactoring is unblocked — plus an
inverse operation that folds it back?

**Answer:** it is implementable but should not be built, because **the premise is false**. The
macro was never what blocked the refactor. Recommendation at the end.

## 1. The premise, tested

`extract_method` refused every method of `impl ConnectionServiceTrait for ConnectionServiceImpl`,
which carries `#[async_trait::async_trait]`. The natural reading — rust-analyzer offers no assists
inside proc-macro-expanded code — is wrong.

Four scratch crates, each run through `restructure apply`:

| Probe | Shape | Result |
|---|---|---|
| `probe6` | **native** `async fn` in trait (Rust 1.75+), `.await`, `?`, `match` | ✅ applied |
| `probe7` | **`#[async_trait::async_trait]`** on trait *and* impl, `.await`, `?`, `match` | ✅ applied |
| `probe8` | `#[async_trait]` **plus an early `return`** inside the selected range | ✅ applied |
| `probe4/5` | sync trait impl, tail expression, early return | ✅ applied |

`probe7`'s output, which compiles:

```rust
#[async_trait::async_trait]
impl Svc for Impl {
    async fn go(&self, n: u32) -> Result<Resp, String> {
        let first = fetch(n).await?;
        self.handle_go(first).await          // delegation written by rust-analyzer
    }
}

impl Impl {                                   // block created by rust-analyzer
    async fn handle_go(&self, first: u32) -> Result<Resp, String> { … }
}
```

**So `#[async_trait]` does not block `extract_method`.** Nor do early returns, `?`, `.await`,
`match`, closures, or a tail expression.

The one range shape that *is* genuinely refused is **a method's entire body** — rust-analyzer
declines to wrap a whole body in a function that adds nothing. Leaving the first statement behind
makes the range a proper subset and it applies.

## 2. What was actually blocking it

The refusal message named it, once it was made to:

```
rust-analyzer offers no "extract into function" assist for the given range
  (it offered: Extract into variable)
```

*Extract into variable* is answered from the **syntax tree**. *Extract into function* needs **type
inference** — the backend already records this as `needs_inference: true`. So the server was
answering, from syntax, while it could not yet type that range.

Two compounding causes, both now fixed:

- **`ensure_indexed` is a whole-file flag set from a hover on the file's *first* symbol.** On an
  18,300-line module that says nothing about a body 11,500 lines further down. Once set, every later
  wait dropped to the short settle budget.
- **`SETTLE_BUDGET` was a hardcoded 30s that `--indexing-budget` could not reach** — the same defect
  shape as the 10s request cap. The error told the reader to raise a flag that did not feed the wait
  that fired.

Fixed in this branch: `settle_budget_for(warmup) = max(30s, warmup/20)`, and the terminal message
now probes `textDocument/hover` **at the range** to distinguish "the range does not support this
assist" from "the server cannot type this range yet", reporting the latter as
`IndexingIncomplete` rather than as an absent assist.

**The cost that remains is real and is not a defect:** rust-analyzer re-resolves this workspace
(~1,639 targets) after each edit to a module that size, so each operation costs minutes. That is an
environment cost, and the tooling now reports it as one.

## 3. Could an `expand_macro` operation be built anyway?

Yes, but not within the guarantee the vocabulary rests on.

**No engine-backed assist exists.** The invariant is that *"every operation but one is backed by a
real assist in the engine that claims it"*. rust-analyzer's only macro refactor is **`inline_macro`,
which handles `macro_rules!` only** — not attribute or derive macros. There is nothing to map an
`expand_macro` operation onto.

**The expansion rust-analyzer can show is not source.** The `rust-analyzer/expandMacro` LSP
extension returns a *rendered string* for a read-only view: pretty-printed from the expanded token
tree, with comments gone, original formatting gone, and hygiene artefacts (`'life0`,
`'async_trait`, `__ra_fixup`) present. `cargo expand` has the same problem plus two more — it
expands *every* macro in the module, and it emits compiler-normalised code. Neither produces
something you would commit.

**The inverse has no engine at all.** Folding

```rust
fn go<'life0, 'async_trait>(&'life0 self, n: u32)
  -> Pin<Box<dyn Future<Output = Result<Resp, String>> + Send + 'async_trait>>
  where 'life0: 'async_trait, Self: 'async_trait
{ Box::pin(async move { … }) }
```

back to `async fn go(&self, n: u32) -> Result<Resp, String>` is a bespoke pattern match against one
macro's output shape. It would be hand-authored transformation in the sidecar — the same category as
`extract_class`, which the skill already documents as its single exception and explicitly declines
to extend. It would also be brittle: the shape is `async-trait`'s private contract and changes
between versions.

So the honest cost is: a large, version-coupled, hand-written feature, whose output is worse source
than the input, to solve a problem that does not exist.

## 4. The cheap alternative, if a macro ever *does* block an assist

Do not expand — **detach the attribute, refactor, reattach it**. Two lexical operations
(`detach_attribute` / `reattach_attribute`) that comment an attribute line out and back. No
expansion, no re-sugaring, source-faithful, trivially reversible. Assists do not require the crate
to compile.

One caveat that must be stated rather than discovered: while the attribute is detached, inference is
degraded, and degraded inference is exactly what produces `_` placeholders in an extracted
signature. The backend's existing `refuse_inferred_placeholder` guard is what makes that window safe
— it refuses rather than writing `fn f(x: _) -> (_, _)`.

**And for `async_trait` specifically there is a better long-term move than any tool feature.**
`grep -rn 'dyn ConnectionService'` finds **no trait-object use anywhere in the workspace**, and
native async-fn-in-trait has been stable since Rust 1.75. This trait could drop the macro outright.
That is a one-time source change, not a restructure operation — but `async_trait` appears in **174
files**, so converting it wholesale is its own project and its own decision.

## 5. Recommendation

1. **Do not build `expand_macro` / `collapse_macro`.** The premise is false and the cost is high.
2. **Keep the diagnostics.** The four hours this consumed went to the *absence* of a diagnostic, not
   to any bug: a wrong assist title, an unrefactorable range, a server told nothing about its
   client, and a server that could not yet type the range all produced byte-identical output. Every
   one of those is now named.
3. **If a macro ever genuinely blocks an assist**, add `detach_attribute` / `reattach_attribute` —
   small, lexical, reversible — and rely on `refuse_inferred_placeholder` for the degraded window.
4. **Consider dropping `#[async_trait]` from `ConnectionService`** on its own merits. Nothing uses
   it as a trait object.
