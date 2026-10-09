# 2026-10-09 — same-crate moves do not widen a private item of the origin's ancestors

**Category:** Missing feature (compile-gate failure, `E0603`)
**Source:** `#reshape` 13/19 (`move-impl-members`)

`move_item` and `move_impl_members` widen a private root item of the **origin module** that the moved code names, when
the destination no longer sees it. An item of an origin **ancestor** is different. Moved code can reach one through
`super::super::x` or through a `use` of it. When the destination lies outside that ancestor's subtree, nothing widens
the item, and the run stops at the compile gate. `reparent_module` widens this case after `#reshape` 1 (its tree-reach
survey).

**What would close it:** give `move_item` and `move_impl_members` `#reshape` 1's ancestor survey: the ancestors
strictly below the common ancestor of origin and destination, filtered by the names in the moved text and confirmed by
references.

**Why deferred:** there is no reproduction, and the `#reshape` targets all move within one subtree (`backends::rust`
into its children), which needs no ancestor widening.
