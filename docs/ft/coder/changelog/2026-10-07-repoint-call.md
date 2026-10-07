# 2026-10-07 — `repoint_call` re-points a call's callee or every receiver of a method

Feature: [Rust code restructuring](../rust-code-restructuring.md).

After a method moves to another type, its callers change by a few tokens that no call-site operation
expressed: `self.common_room_slot(x)` had to become `self.peer_routing.common_room_slot(x)`, and
`x.m(..)` had to become `x.agent_roster().m(..)`. The four argument operations edit an argument list
only, so those sites — twenty-eight of them, plus seven wrappers, on one carve — were rewritten by
hand, and `restructure verify` then reported the hand-made re-points as a loss it could not excuse.
`restructure` now has `repoint_call`, and `verify` is taught to account for a declared re-point.

- **One call's callee, or every call of a method.** The **single form** is an `item` anchor on the
  function holding the call, with a relative range over exactly one call, and `callee` = the complete
  new callee (`self.slot(x)` → `self.peer.slot(x)`, `slot(x)` → `lookup::slot(x)`). The **bulk form**
  is an `item` anchor on a method with no range, and `callee` = a `$receiver<hops>.<method>` template;
  the hops are inserted after the receiver of every call of that method the server knows
  (`x.m(..)` → `x.agent_roster().m(..)`). Arguments are never added, removed or reordered.
- **The part in front of the arguments, and nothing else.** The span replaced ends at the argument
  list's `(`: the arguments and the parentheses around them are kept byte for byte, a comment between
  the callee and the `(` is left where it is. In the bulk form the edit is an insertion after each
  receiver, so nested and chained calls (`a.m(b.m(1))`, `a.m().m()`) compose without overlapping.
- **Refused before anything is written.** A `callee` that is not one path or method chain; a range
  that is not exactly one call; an old callee that holds a call (its arguments would be dropped
  silently); a method-call turbofish a field chain cannot restate; a callee equal to the current one;
  a bulk anchor that is not a method. The bulk form refuses, all at once and naming each `file:line`,
  every reference it cannot re-point — a path call, a function pointer, an import. A reference inside
  a comment is left alone; a method nothing calls is a no-op with a note.
- **`verify` accounts for a declared re-point.** `restructure verify --repoint OLD=NEW` (repeatable)
  tells `verify` which callee texts the author changed. It then pairs a lost statement with the gained
  one that differs only by the declared callee, and counts the pairs under `repointed`. A hop nobody
  declared, a replaced hop, a changed argument and a different method stay reported: the declaration
  proves the differences are of the shape a re-point produces, not that the plan produced them.
- **Calls and receivers only.** `self.<field>` → `state.<field>` is a field read, not a call, and
  stays a separate open capability. A caller the bulk form cannot express (a UFCS site, a
  function-pointer use, an import) is refused, not rewritten; pair the operation with the argument
  operations in one `group` when a call's arguments change too.
