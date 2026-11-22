# 2026-11-21 — `tddy-tools restructure` ignores `#[path]`, so one such module makes every plan in its crate "malformed"

**Category:** Known defect — `tddy-code-restructuring` engine
**Source:** `/pr-wrap` step 3.5 on `#orchestra` 2/9 (PR #980), the split of
`tddy-agent-launch/src/remote_hook_follower.rs`
**Consent:** the developer chose to defer this on 2026-11-22 at the `#orchestra` stack's TODO gate.

## What happened

```bash
./dev target/debug/tddy-tools restructure check <plan.jsonl> --deep    # against the warm index daemon
```

The plan is one `move_item` that never touches the module named in the refusal:

```json
{"op":"move_item","anchor":{"kind":"items","file":"packages/tddy-agent-launch/src/remote_hook_follower.rs","items":["tddy_agent_launch::remote_hook_follower::HookReportDelivery","tddy_agent_launch::remote_hook_follower::ActivityHookSink","tddy_agent_launch::remote_hook_follower::<ActivityHookSink>","tddy_agent_launch::remote_hook_follower::<ActivityHookSink as HookSink>"],"fingerprints":["…"]},"name":"remote_hook_delivery","to":"tddy_agent_launch","reexport":"outside"}
```

```text
   indexing (+34ms): move_item: surveying 8 member(s) of the types the move splits
   …
0: plan is malformed: `mod tests;` leads to neither `tests.rs` nor `tests/mod.rs` in packages/tddy-agent-launch/src/opencode_cli_spawn/restore_tabs
Error: 1 finding(s) — see above. Nothing was written.
```

The cause is `packages/tddy-agent-launch/src/opencode_cli_spawn/restore_tabs.rs`. It declares its tests as

```rust
#[cfg(test)]
#[path = "restore_tabs_tests.rs"]
mod tests;
```

It has four more modules declared the same way (`sandbox_tests`, `sandbox_agent_tests`,
`target_tests`, `target_agent_tests`). The engine's module-file resolution looks only for
`restore_tabs/tests.rs` or `restore_tabs/tests/mod.rs` and does not read the `#[path]` attribute. It
refuses the whole plan, although rustc compiles the crate.

## Minimal reproduction

A crate `app` with:

```rust
// src/lib.rs
pub mod a;
pub mod b;
// src/a.rs
pub fn moved() {}
// src/b.rs
#[cfg(test)]
#[path = "b_tests.rs"]
mod tests;
// src/b_tests.rs
#[test]
fn t() {}
```

`restructure anchors src/a.rs --items moved`, then a plan
`{"op":"move_item", …, "name":"c", "to":"app", "reexport":"none"}`. Expected: `a::moved` moves to
`src/c.rs`. Observed (by analogy with the refusal above, not yet run on the fixture):
`plan is malformed: \`mod tests;\` leads to neither \`tests.rs\` nor \`tests/mod.rs\` in src/b`.

## What it cost

`#980` split three `tddy-agent-launch` files by hand, with the developer's consent (2026-11-21). Each
seam carries `TODO(restructure-path-attr)`:

- `remote_hook_follower.rs` → `remote_hook_delivery.rs`: **refused**, as above.
- `cli_agent_steps.rs` → `cli_agent_steps/resumed_wiring.rs` and `opencode_cli_spawn.rs` →
  `opencode_cli_spawn/start_steps.rs`: **not run through the engine**. They were split by hand on the
  inference that the same refusal would hit any move in this crate.

## What would close it

Module-file resolution honours `#[path = "…"]`, relative to the declaring file's directory as rustc
does. Only modules the plan touches are resolved strictly. Add the reproduction above as an engine
test. Then the `TODO(restructure-path-attr)` markers can be removed, and a re-run of
`restructure verify` over those commits can confirm the hand moves.

## Recorded instance — `#orchestra` sweep (#999), 2026-11-22

Moving `tddy_daemon_rpc::exec_tool::base_freshness::orchestrator_caller` into `tddy_daemon_rpc::exec_tool::project_home::caller` (`move_item`, `reexport: none`) was refused by `restructure check --deep`: "plan is malformed: `mod project_stacks_unit_tests;` leads to neither `project_stacks_unit_tests.rs` nor `project_stacks_unit_tests/mod.rs` in packages/tddy-daemon-rpc/src/pr_stack/project_stacks". The module is declared at `pr_stack/project_stacks.rs` with `#[path = "project_stacks_unit_tests.rs"]`. The developer consented to a hand move (2026-11-22), done as part of the sweep.
