# 2026-10-20 — `move_impl_members` does not widen a stayed member that lives in an inline module of an ancestor's file

**Category:** Missing feature (compile-gate failure, `E0624`)
**Source:** `#reshape` 12/24 (`move-impl-members`), `impl_move/survey/stayed.rs`

R6 (b) widens a private member of the moved type that stays behind and that the moved lines name, in the origin's
file and in each ancestor module of the origin below the common ancestor of origin and destination. An ancestor's
members are read with `members_of` over **that file's top-level outline**. A block of the type written inside an
inline `mod` of an ancestor's file is therefore not surveyed, and a private member there that the moved lines name
stays private. The run stops at the compile gate, which refuses it; nothing wrong is written.

**What would close it:** have `stayed::ancestor_stayed` descend into the inline modules of the ancestor's outline, with
`written_in` and `lands_in` set to the inline module's own path, and cover it with a live test whose ancestor file
declares the type's block inside `mod x { … }`.

**Why deferred:** no `#reshape` target has the shape (the origins are files, and their ancestors hold their blocks at
the top level), and the compile gate refuses the unwidened case rather than producing a wrong program.
