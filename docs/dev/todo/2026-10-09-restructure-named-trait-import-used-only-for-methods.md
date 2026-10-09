# 2026-10-09 — `extract_module` does not carry a named trait import that the seam uses only for its methods

**Category:** Future enhancement
**Source:** #reshape 4/19 (`extract-method-clean`), scoping of gap I of
[the lifecycle-destructure apply gaps](./2026-09-24-restructure-apply-gaps-from-the-lifecycle-destructure-run.md)

#reshape 4 carries every `use <path> as _;` of the parent into a module `extract_module` creates, because such an import
can only be there for a trait's methods and binds no name that could clash. A **named** trait import (`use prost::Message;`)
that the moved code needs only for method resolution (`x.encode_to_vec()`, `T::decode(…)`) is still not carried:

- `prelude_shadow::shadowed_imports` carries a parent binding only when the moved code *names* it, and a method call does
  not name the trait;
- the server-driven import pass keys on `unresolvedReference` semantic tokens, and an unresolved method is not one.

The result is `E0599` at the compile gate, fixed by hand with one `use` line.

## Why deferred

No reproduction on the current tree: #524's gap I and the lint gate's N3 were both `as _` imports. Carrying every named
parent import the moved code does not name would over-import types too (the tidy would prune them, but each also risks a
name clash with an item the module declares). A sound signal is needed first: rust-analyzer's unresolved-method
diagnostic if the dev shell's server emits one, a parent import that goes unused after the cut, or
[the compiler-guided repair](./2026-10-09-restructure-compiler-guided-import-repair.md).
