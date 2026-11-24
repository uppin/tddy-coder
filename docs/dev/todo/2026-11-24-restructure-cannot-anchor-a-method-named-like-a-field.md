# 2026-11-24 — `move_impl_members` cannot take a method that shares its name with a field of its type

**Category:** Future enhancement (engine gap)
**Source:** `/pr-wrap` step 3.5 on `feature/session-close-and-remove`: the `close_task.rs` split
(`docs/dev/changesets/2026-11-24-session-close-and-remove.md` § Restructuring)

`CloseRun` in `svc_close_session/close_task.rs` has a field `failed` (the steps that failed, in order)
and a method `failed` (stream one FAILED step):

```rust
struct CloseRun {
    …
    failed: Vec<CloseSessionStep>,
}

impl CloseRun {
    fn running(&mut self, …) { … }
    fn done(&mut self, …) { … }
    fn failed(&mut self, kind: CloseSessionStepKind, subject: &str, host: &str, message: &str) { … }
    fn skipped(&mut self, …) { … }
    fn step(&mut self, …) { … }
    …
}
```

The natural seam was all seven step members (`running` … `send`) into `close_task::steps`. Neither
spelling of `failed` reaches the operation:

```text
$ tddy-tools restructure anchors close_task.rs --items '…::close_task::CloseRun::failed'
Error: … plan is malformed: `…::close_task::CloseRun::failed` names 2 items in …/close_task.rs —
qualify it with the trait, as `…::close_task::<CloseRun as Trait>::failed`

$ tddy-tools restructure anchors close_task.rs --items '…::close_task::<CloseRun>::running,…,…::<CloseRun>::failed,…'
{"kind":"items", …}                                   # emitted
$ tddy-tools restructure snapshot plan.jsonl          # a move_impl_members line with that anchor
Error: plan is malformed: `move_impl_members` cannot move `<CloseRun>`: a whole `impl` block moves with `move_item`
```

The first resolver counts the struct field as a second item named `failed`, and its remedy (a trait)
does not apply, since there is no trait impl. `anchors` accepts the `<CloseRun>::failed` spelling, but
`move_impl_members` reads it as the whole block.

**Not worked around.** The split took a narrower seam, `step`, `note` and `send` (with `progress_line`),
and the four one-line state wrappers (`running`, `done`, `failed`, `skipped`) stay in `close_task.rs`.

## What would close it

An item path resolves a member of an inherent `impl` before a field of the same name (a field is never
an item the operations move). Or `move_impl_members` accepts `c::m::<Type>::member` as the member of
that block, which `anchors` already emits.
