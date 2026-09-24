# 2026-09-24 — `restructure` has no operations that change a function's signature

**Category:** Future enhancement
**Source:** `#carve` 13/15 wrap, [#527](https://github.com/uppin/tddy-coder/pull/527), changeset
[`2026-09-23-restructure-engine-fixes`](../1-WIP/2026-09-23-restructure-engine-fixes.md)

The Rust backend supports ten operations: `extract_method`, `extract_variable`, `extract_module`,
`extract_module_to_file`, `extract_trait`, `inline_method`, `rename_symbol`,
`move_module_to_crate`, `move_cluster_to_crate` and `move_test_binary_to_crate`
(`backends/rust.rs`, `SUPPORTED`). None of them changes a signature, and the plan schema has no
field that could describe a new one.

| Change | Today |
|---|---|
| Rename a parameter | ✅ `rename_symbol` with a **range** anchor on the parameter's name. rust-analyzer's outline does not list parameters, so no symbol anchor names one. Not documented in `plan-schema.md` |
| Remove a parameter | ❌ |
| Add a parameter | ❌ |
| Reorder parameters | ❌ |
| Change a parameter's type | ❌ |
| Change the return type | ❌ |

## Why this was deferred

Raised during #527's wrap (2026-09-24). It is new vocabulary, not an engine defect, and the
developer asked for it to be filed. The developer also asked that **changing the return type be
supported**.

## What rust-analyzer offers

rust-analyzer has no general "change signature" refactoring. What it has, by assist id (check the
ids against the rust-analyzer the dev shell ships; some were renamed in 2024, e.g.
`wrap_return_type_in_result` became `wrap_return_type`):

| Assist | What it does | Callers |
|---|---|---|
| `remove_unused_param` | removes a parameter the body does not use | ✅ rewrites every call site |
| `wrap_return_type` (`Option` / `Result`) | `-> T` becomes `-> Result<T, _>`, with the tail wrapped in `Ok(…)` | ❌ callers are left to the compiler |
| `unwrap_return_type` | the reverse | ❌ |
| `convert_tuple_return_type_to_struct` | `-> (u32, String)` becomes `-> Named { … }`, with a new struct | ✅ destructuring call sites rewritten |
| `bool_to_enum` | a `bool` parameter or local becomes a two-variant enum | ✅ |
| a quick fix on a call passing too many arguments (**unverified**: check the bundled rust-analyzer before relying on it) | adds the parameter to the declaration | only the one call it was offered on |

Nothing adds a parameter together with its callers, reorders parameters, or changes an arbitrary
type.

## What would close it

### 1. `remove_unused_param`, backed by the assist

This is the cheapest. It slots in like `inline_method`: a symbol anchor on the function, plus the
parameter's name.

```jsonl
{"op":"remove_unused_param","anchor":{"kind":"symbol","file":"src/spawn.rs","path":"build_claude_argv"},"name":"initial_prompt"}
```

```rust
// before
pub fn build_claude_argv(binary_path: &str, model: &str, initial_prompt: Option<&str>) -> Vec<String> { … }
let argv = Self::build_claude_argv(binary, model, None);

// after
pub fn build_claude_argv(binary_path: &str, model: &str) -> Vec<String> { … }
let argv = Self::build_claude_argv(binary, model);
```

Refuse when the parameter is used: rust-analyzer does not offer the assist then, and the refusal
should say so.

### 2. `change_return_type` (the developer asked for this)

Two tiers.

- **Wrap or unwrap in `Option`/`Result`**, backed by `wrap_return_type` / `unwrap_return_type`. The
  assist rewrites the declaration and the function's own returns. The callers are not rewritten, so
  every call site the new type breaks is left for `apply`'s compile gate to name. That matches the
  developer's rule from the same day: the engine moves the code, and a mechanical compile fix is done
  by hand.

  ```jsonl
  {"op":"change_return_type","anchor":{"kind":"symbol","file":"src/spawn.rs","path":"spawn_tool"},"to":"Result<_, SpawnError>"}
  ```

  ```rust
  // before
  fn spawn_tool(&self, argv: Vec<String>) -> PtyHandle { … PtyHandle::new(child) }

  // after
  fn spawn_tool(&self, argv: Vec<String>) -> Result<PtyHandle, SpawnError> { … Ok(PtyHandle::new(child)) }
  // callers: `let pty = self.spawn_tool(argv);` now fails with E0308, and the gate names each one
  ```

- **An arbitrary new type** has no assist. The engine would do it itself:
  - rewrite the `-> …` in the declaration;
  - leave the body and callers to the compile gate;
  - report every caller through `textDocument/references`, so the gate's errors can be read against
    a known list.

  `tuple → struct` stays the `convert_tuple_return_type_to_struct` assist.

  ```rust
  // before
  fn resolve_placement(&self, req: &StartSessionRequest) -> (String, Vec<String>) { … }

  // after, `"to": "Placement"`
  fn resolve_placement(&self, req: &StartSessionRequest) -> Placement { … }
  ```

### 3. `change_signature`, for adding, reordering and retyping parameters (no assist behind it)

This is the same shape as `move_module_to_crate`: engine-informed rather than engine-performed.
Every call site comes from a real `textDocument/references` result, never a text search. The plan
states the target parameter list, mapping each new position to an old one or to a default
expression.

```jsonl
{"op":"change_signature","anchor":{"kind":"symbol","file":"src/spawn.rs","path":"build_claude_argv"},
 "params":[{"from":"binary_path"},{"from":"model"},{"new":"resume: bool","default":"false"},{"from":"session_id"}]}
```

```rust
// before
pub fn build_claude_argv(binary_path: &str, model: &str, session_id: &str) -> Vec<String> { … }
let argv = Self::build_claude_argv(binary, model, &sid);

// after: `resume` added with its default at every call, `session_id` moved last
pub fn build_claude_argv(binary_path: &str, model: &str, resume: bool, session_id: &str) -> Vec<String> { … }
let argv = Self::build_claude_argv(binary, model, false, &sid);
```

What it must refuse rather than approximate:
- a call it cannot rewrite, such as a call passed as a function value
  (`iter.map(Self::build_claude_argv)`), a call inside a macro, or a trait method whose other
  impls would diverge;
- a retyped parameter whose callers pass an expression the new type does not accept. That one is
  left to the compile gate and reported as such, not silently converted.

**Changing a parameter's type** is `{"from":"model","type":"ModelId"}` in the same list. The
declaration is rewritten, and the call-site arguments are left to the compile gate, as for a return
type.

### 4. Document parameter renames

`plan-schema.md`'s `rename_symbol` row should say that a parameter is renamed through a **range**
anchor on its name, since no symbol anchor names one.

```jsonl
{"op":"rename_symbol","anchor":{"kind":"range","file":"src/spawn.rs","start":{"line":364,"col":29},"end":{"line":364,"col":40}},"name":"binary"}
```
