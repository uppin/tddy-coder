# 2026-10-20 — `move_impl_members` moves a run of `impl` members into another module

`#reshape` 12/24. A plan can move a contiguous run of methods, associated functions and associated constants
out of one inherent `impl T` and into an `impl T` block of any other module of the same crate, existing or
created by the line with `name`. The members travel byte for byte and join the destination's one identical
block, or open a new one; callers are not edited, and each private member, field or item the split puts out of
reach is widened as little as it needs and reported. A destination that binds a reached name to something else
is refused before anything is written. See
[`move_impl_members`](../rust-code-restructuring.md#move_impl_members).
