# 2026-09-25 — `extract_variable` hoists a field read under `&` by value, a move out of `&self`

**Category:** Future enhancement (engine defect; the tree does not compile after the apply)
**Source:** `#carve` 15/15, [#526](https://github.com/uppin/tddy-coder/pull/526), T3 port-move
pilot, second run: plan
`docs/dev/1-WIP/2026-09-23-carve-lifecycle-wiring-plans/21a-agent-def-for-spawn-hoist.jsonl`

## What happened

`DaemonSessionHost::agent_def_for_spawn` opens with a borrow of a non-`Copy` field:

```rust
// before
if let Some(registry) = &self.model_registry {          // model_registry: Option<Arc<ModelRegistryStore>>
```

The plan hoisted the field read. The range covers `self.model_registry` (line 186, columns 34–53), not
the `&` in front of it, because a range starting on the `&` never resolves (see
[the hang todo](2026-09-25-restructure-extract-variable-waits-forever-on-a-range-opening-with-a-borrow.md)).
rust-analyzer bound the place by value and kept the borrow at the use:

```rust
// after the apply
let model_registry = self.model_registry;
if let Some(registry) = &model_registry {
```

```text
svc_resolve_listed_worktree.rs:186:30: error[E0507]: cannot move out of `self.model_registry` which is behind a shared reference: move occurs because `self.model_registry` has type `Option<Arc<ModelRegistryStore>>`, which does not implement the `Copy` trait
```

The build correction was one token on the line the engine wrote, `let model_registry = &self.model_registry;`.
The `if let` needed nothing, because match ergonomics bind `registry` as the same
`&Arc<ModelRegistryStore>` through the extra reference.

The plan schema already says that rust-analyzer "decides between `&self.x` and `self.x` from the
autoref it sees". Here it saw the `&` and still wrote the value.

## What would close it

- When the selected expression is a **place** (a field access or a path) of a non-`Copy` type, and its
  parent expression is `&<place>` or `&mut <place>`, widen the selection to the borrow before asking
  for the assist, so the binding is `let x = &self.field;` and the use becomes `x`.
- Otherwise, when the assist writes `let x = <place>;` for a non-`Copy` place reached through `&self`,
  refuse as `rust-analyzer's answer was unusable:` rather than leaving `E0507` for the compile gate.

A red test: `struct S { v: Option<String> } impl S { fn f(&self) -> usize { if let Some(s) = &self.v { s.len() } else { 0 } } }`,
extracting `self.v`. The result should compile.
