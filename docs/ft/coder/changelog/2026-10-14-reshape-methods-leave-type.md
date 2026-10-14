# 2026-10-14 — a method's body can leave a type that stays: `read_fields_through` and `retarget_impl`'s delegator

Feature: [Rust code restructuring](../rust-code-restructuring.md).

Moving a method's body out of a type that cannot leave its crate meant rewriting every `self.<field>` to
`state.<field>` by hand, and keeping the type's callers compiling meant writing each forwarding method by
hand. `restructure` does both.

- **`read_fields_through`** (new operation, anchored on a range inside a method) inserts
  `let <name> = <expr>;` before the range and rebinds `self` in it to `<name>`. In **field mode** only
  `self.<field>` reads are rewritten, each field is checked against the view's type (across crates), and
  a `&` the view already provides is dropped so clippy's `needless_borrow` does not fire. In **self mode**
  (`expr` is `self`, `&*self` or `&mut *self`) every `self` and `Self` is rewritten, so a following
  `extract_method` writes a free function. Every refusal is raised before anything is written.
- **`retarget_impl` with `variant: "leave_delegator"` and `expr`** leaves a forwarding method in each moved
  method's slot of the old block, with `impl New { … }` following it. Members that cannot be forwarded
  (pattern parameters, associated consts and types, `const fn`) are refused, and a delegator nothing calls
  is noted.
- **`restructure verify`** accounts for both: `--rebind NAME` declares a rebind, and a declared
  `--retarget` excuses the delegators.
- Operations: twenty-five to twenty-six.

Known limit: in self mode a generic `impl`'s `Self::X` is written `W<T>::X`, which is not valid in
expression position; the compile gate refuses it.

Backlog entries resolved: "`restructure` has no operation that makes a method read its fields through a
borrowed state value" (2026-09-25) and "`restructure` cannot re-point a call's receiver or leave a
forwarding delegator" (2026-10-05).
