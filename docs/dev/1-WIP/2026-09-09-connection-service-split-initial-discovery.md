# Initial Discovery: `connection_service.rs`

**Date**: 2026-09-09
**Target**: `packages/tddy-daemon/src/connection_service.rs` — 24,858 lines
**Goal as stated**: every resulting file under 500 lines

## 1. Composition

197 top-level items. Measured by walking column-0 item starts to their matching `^}`:

| Region | Lines | Share |
|---|---:|---:|
| Header + `use` block (L1–235) | 235 | 1% |
| Free fns / structs / stream adapters / proto converters | ~5,000 | 20% |
| 29 inline test modules | ~6,300 | 25% |
| 4 inherent `impl ConnectionServiceImpl` blocks | 7,130 | 29% |
| `impl ConnectionServiceTrait for ConnectionServiceImpl` | 6,483 | 26% |
| Small impls (`Stream`, `Unpin`, `Drop`, handler traits) | ~1,700 | 7% |

## 2. The four impl blocks

| Block | Lines | Methods | Largest member |
|---|---:|---:|---|
| `impl ConnectionServiceImpl` L2004–3154 | 1,151 | 46 | `new` 146, `start_claude_cli_session` 105, `pr_status_for_caller` 84 |
| `impl ConnectionServiceImpl` L4163–4623 | 461 | 11 | `ensure_project_available_for_start` 168 |
| `impl ConnectionServiceImpl` L4679–8163 | 3,485 | 49 | **`start_sandboxed_claude_cli_session` 609**, `start_sandboxed_cursor_cli_session` 460, `relaunch_sandboxed_runner` 279, `split_context_from_codebase_host` 213 |
| `impl ConnectionServiceImpl` L9279–11311 | 2,033 | 24 | **`start_session_core` 830**, `spawn_split_agent` 228, `start_split_claude_cli_session` 148 |
| `impl ConnectionServiceTrait` L11480–17962 | 6,483 | 90 fns + 21 assoc types | `resume_session` 232, `prompt_agent_conversation` 167 |

## 3. Seam checks — what the vocabulary can and cannot reach

Three findings decide the whole plan.

### 3.1 An `impl` block moves whole, or not at all

`extract_module` is the only Rust splitting operation, and **an `impl` body cannot hold a `mod`**. There is
no operation that cuts one `impl` block into several. A whole `impl` moves to any module in the crate with
no caller churn (methods are reached through their type), so each of the five blocks *can* get its own
file — but each arrives at its current size. Four of the five are over 500 lines.

Cutting a *member* out of an `impl` into a sibling module is refused whenever a sibling in that same
`impl` calls it (`plan-schema.md`, third geometry). Every one of these blocks is densely
self-calling, so that route is closed regardless.

### 3.2 Two single methods already exceed 500 lines

`start_session_core` (830) and `start_sandboxed_claude_cli_session` (609) are individually over budget. A
method cannot be split across files either, so **`extract_method` must run before any block split** —
it is engine-backed (rust-analyzer *extract into function*) and is the one operation that reduces a
member's size. Note it does not reduce the file: bodies relocate within the same `impl` and each
extraction adds a signature, so total lines grow slightly.

### 3.3 The trait impl has an arithmetic floor of ~680 lines

`impl ConnectionServiceTrait for ConnectionServiceImpl` carries **111 members** — 90 `async fn` and 21
`type …Stream` associated types — whose **signatures alone occupy 385 lines**. With every method body
reduced to a single delegating line the block still measures:

```
385 signature lines + 90×(1 body + 1 close) + 111 separators + impl header/footer ≈ 680 lines
```

**Under 500 is unreachable for this block at 111 members.** It would require splitting
`ConnectionService` into several gRPC services in the `.proto` — a protocol change, not a
behaviour-preserving restructure. Recorded as a refusal, not attempted.

The floor *is* reachable by the tool, though — see §3.4.

### 3.4 `extract_method` inside a trait impl writes a new inherent `impl` block

Probed rather than assumed (§6), and it reverses the conclusion this document first recorded.
Asked to extract a body out of `impl Trait for Type`, rust-analyzer does **not** put the new
function in the trait impl's own assoc-item list — which would be `E0407` — and does not decline.
It writes a **new `impl Type { … }` block after the trait impl** and puts the function there,
rewriting the original body into a `self.extracted(…)` call:

```rust
impl Svc for Impl {
    fn one(&self, n: u32) -> u32 {
        let mut acc = 0;
        self.sum_up(n, &mut acc);   // ← delegation written by rust-analyzer
        acc
    }
}

impl Impl {                          // ← block created by rust-analyzer
    fn sum_up(&self, n: u32, acc: &mut u32) { … }
}
```

Two consequences decide the plan:

1. **The 90 delegations need not be hand-written.** The tool generates each one, so taking the
   trait impl to its ~680-line floor is a plan, not a hand-edited commit.
2. **Successive extractions accumulate in one block.** Two operations in one plan produced one
   new `impl Impl` block holding both functions, not two blocks. So the bodies leaving the trait
   impl land in a single ~5,800-line inherent impl, which still has to be cut — and cutting an
   `impl` block is the one thing the vocabulary cannot do (§3.1).

Both operations composed in a single plan, confirming the schema's note that Rust operations other
than `extract_module` + `extract_module_to_file` share a plan.

## 4. Blast radius

```
grep -rn 'connection_service::' --include='*.rs' .
```

93 files outside the target reference the module — 12 crate-internal (`sandbox_session.rs` 8,
`cursor_cli_spawn.rs` 7, `runtime.rs` 3, plus 9 single-reference modules) and 81 integration tests
under `packages/tddy-daemon/tests/`.

17 distinct symbols are reached, and the distribution is extremely skewed:

| Symbol | Refs |
|---|---:|
| `ConnectionServiceImpl` | 62 |
| `AgentActivityHub` | 9 |
| `now_unix_ms` | 3 |
| `HOST_DOCUMENT_FRAME_BYTES` | 3 |
| `validate_repoint_target`, `started_roster_rev`, `SpawnStackParent`, `SessionUserResolver`, `SessionsBaseResolver`, `SeededAgentClones`, `SeedCodebase`, `roster_replacement_pairs`, `push_new_branch_to_origin_if_requested`, `materialize_session_attachments`, `local_daemon_hook_url`, `EXEC_TOOL_FRAME_BYTES`, `add_planned_pr` | 1 each |

`reexport: "glob"` on every extracted module keeps all 17 paths resolving. **Projected caller
changes: zero.** No file outside the target appears in the diff.

## 5. Test modules

29 inline `mod …tests` blocks, ~6,300 lines. All reach the code under test through `use super::*`, which
keeps resolving from a child module file (a child sees its parent's private items, and the parent's glob
re-exports cover items that moved to siblings). Two exceed 500 lines on their own and need a nested split,
which a `mod` body *does* permit:

| Module | Lines |
|---|---:|
| `host_add_key_handler_tests` | 1,266 |
| `agent_activity_unit_tests` | 1,236 |
| `stack_child_link_tests` | 400 |
| `host_stats_handler_unit_tests` | 378 |
| `host_tooling_handler_unit_tests` | 302 |
| 24 others | 24–254 each |

## 6. Probe — and three tooling defects it turned up first

Three scratch crates were run through `tddy-tools restructure apply` to settle §3.3 and §3.4.
Every one of them first reported:

```
plan is malformed: rust-analyzer offers no "extract into function" assist for the given range
```

**That message was false, for all of them, and the restructure tooling was the reason.** It was
reported for an inherent impl and a trait impl alike, and for `extract_trait` as well, which is how
it became clear the range was not the subject. Chasing it found four defects on the *bridge*
transport — the path `tddy-tools restructure` actually uses, where the backend talks through a
shared `tddy-lsp` client rather than spawning rust-analyzer itself:

| | Defect | Effect |
|---|---|---|
| D1 | `tddy-lsp`'s response dispatch read `result` and defaulted to null, discarding any JSON-RPC `error` | Every server error arrived as a successful empty answer. `ContentModified` — rust-analyzer's "ask again" — became "the server had nothing to offer", and the backend's `request_settled` retry loop was dead on this path |
| D2 | `map_lsp_error` classified every failure, timeouts included, as `MalformedPlan` | A slow index reported as a defective plan, sending the reader to rewrite anchors that were never wrong. Only `ServerCatchingUp` is retried, so the classification also skipped the very loop `--indexing-budget` governs |
| D3 | `tddy-lsp` capped every request at a hardcoded 10s, unreachable from `--indexing-budget` | A `codeAction` against a cold index cannot answer in 10s, so the documented remedy for a slow machine was inert |
| D4 | The bridge's client was initialized with `"capabilities": {}` — the backend's own `client_capabilities()` is only sent on the self-spawned path, which `start()` skips when a bridge is present | **The blocker.** rust-analyzer returns *no code actions at all* to a client that advertised no `codeAction` support. It also left the server on the LSP default of utf-16 positions while this client counts utf-8 bytes, so every column was silently wrong on any line with a non-ASCII character — and this file's comments are full of em dashes. The self-spawned path *refuses* that mismatch; the bridge never checked |

All four are fixed, with the fix for D4 also carrying `initializationOptions` (pinned import
granularity) and the position-encoding refusal onto the bridge path. A fifth change makes the
failure legible rather than merely correct: an expired budget now reports `IndexingIncomplete` when
the server never answered and an absent assist only when it answered, and **names the titles it did
offer** — which is the evidence that separates a wrong assist title from an unrefactorable range,
and was not obtainable at all before.

With those fixed, all three probes succeed, and §3.4 is the result.

## 7. Conclusion

| | |
|---|---|
| Reaches <500 with the plan vocabulary alone | ~11,500 lines (free items + all 29 test modules) |
| Reaches <500 once `extract_method` has cut oversized members and trait-method bodies | a further ~12,300 lines |
| Needs hand-written lines | **only `impl` block boundaries** — `}` + `impl ConnectionServiceImpl {` pairs, no body touched. §3.4 removed the 90 hand-written delegations this document first assumed |
| **Cannot reach <500 at any cost short of a `.proto` change** | `impl ConnectionServiceTrait`, floor ~680 lines |

Largest achievable resulting file: **~680 lines**, and it is the trait impl.
