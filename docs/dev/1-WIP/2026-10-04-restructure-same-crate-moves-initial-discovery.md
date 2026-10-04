# Initial Discovery: restructure same-crate moves, and the lifecycle moves they unblock

**Changeset**: [2026-10-04-restructure-same-crate-moves.md](./2026-10-04-restructure-same-crate-moves.md)
**Date**: 2026-10-04
**Passes**: 3 (all parent-driven; see "Agent" lines)

## Combined Conclusions

**What this change is.** `#carve` 16a (#531, merged as `98a9686c`) deferred four items because the
`tddy-tools restructure` engine cannot do them, and filed the gaps as TODOs. This change fixes the
engine, then applies the deferred items **with the new operations**, so the moves are the real-world
test of the operations.

**The engine has no same-crate move, and none of its pieces is shaped for one.**

- `RefactorKind` (`packages/tddy-code-restructuring/src/plan.rs:147`) has `ExtractModule`,
  `ExtractModuleToFile`, `MoveModuleToCrate`, `MoveClusterToCrate`, `MoveTestBinaryToCrate`, and a
  **TypeScript-only `MoveSymbol` / `MoveFile`** ("rust-analyzer has no whole-symbol move",
  `plan.rs:154`). The Rust backend's supported set (`backends/rust.rs:67-73`) holds none of the
  `Move*` kinds except the three cross-crate ones.
- `extract_module`'s anchor is one `file` plus a contiguous `items` run (`plan-schema.md:76-99`), and
  the new module becomes a child of that file's module. It cannot gather items of several files, and it
  cannot put the module under a different parent.
- The cross-crate moves are "engine-informed, not engine-performed" (`crate_move.rs:1-30`): callers
  come from `textDocument/references`, and the module authors the mechanical half itself (`git mv`,
  manifest edits, a `pub use`, the qualifier at the head of the moved file's `use` lines). Its path
  survey (`crate_move/survey.rs`), facade writer (`crate_move/moving/facade_writer.rs`) and the nested
  module handling (`tests/nested_module_move_acceptance.rs`: parent declared by `<parent>.rs` or
  `<parent>/mod.rs`) are the pieces a same-crate move reuses.
- The Rust backend file is already oversized (`backends/rust.rs`, 5,385 lines including tests; its code
  issue `oversized-file-backends-rust.md` is open, and the 2026-10-03 backlog entry
  `restructure-rust-backend-grows-with-every-live-plan-node` records that every live-plan node grew
  it). New operations must live in modules of their own and only dispatch from `rust.rs`
  (`rust.rs:1151-1160` is where the cross-crate moves dispatch).
- The vocabulary rule (`plan.rs:141-144`): "a vocabulary that advertises what cannot be performed is
  worse than a smaller one", and `No create_file` (`SKILL.md` Rules): files appear only through an
  assist or an authored move. So a destination module must already exist, or be created by
  `extract_module`/an authored `mod` line, never by an arbitrary-content op.

**What the lifecycle items need, measured on the current tree** (master `98a9686c`):

| Item | Reality |
|---|---|
| **M0.1** `peer_session_answer` | 4 free items in 4 files: `peer_has_no_such_session` (`connection_service/split_start.rs:43`), `split_pairing` (`split_session.rs:80`), `resolve_worktree_root_for_session` (`workspace_session.rs:267`, 6 callers across 6 files), free `resolve_exec_tool_worktree` (`connection_service/svc_resolve_os_user.rs:155`; a host method of the same name sits at `:54`). They need re-pointing callers (`reexport: none`), not facades: T3 callers must name the defining module (acceptance checks A2/A4 of the carve) |
| **M0.6** `seeded_clone_guard.rs` split | `SessionStdioEndpoint` (`:116`) and `ExecToolRoute` (`:124`) are both `pub(crate)` in a 136-line file. `ExecToolRoute` is used by `local_exec_tools.rs` (`:20`, `:63-185`), `SessionStdioEndpoint` by `connection_service.rs:135` and `svc_start_claude_cli_session.rs:233` |
| **M0.2 T4 half** | `write_claude_hooks_settings` (`hooks_and_urls.rs:12`, `pub(crate)`) and `resolve_start_session_claude_binary` (`:37`, `pub`); callers in `svc_spawn_split_agent.rs:239,255`, `claude_cli_spawn.rs:174`, `claude_cli_spawn_steps.rs:138`, and two `*_binary_resolution_tests.rs` |
| **M0.4** re-parent | The TODO's nine-row table is **stale in three rows**: `relay_idle`, `session_admission_service` and the session catalog left the crate in #526, so `rpc_activity`, `first_admission_token` and `session_dir_lookup` have no in-crate destination: they go straight to their receiver crates in node 17 with `move_module_to_crate` (which moves a nested module alone). Six rows still have an in-crate destination (`split_claude_cli_start` to `split_start`; `session_attachment_materialization` to `svc_materialize_staged_attachment`; `local_exec_tool_dispatch` to `local_exec_tools`; `session_room_opening` to `svc_ensure_session_room_for_agents`; `jail_env_builders` into the sandboxed-launch family; `presenter_observer_spawn` to `presenter_observer_task`), plus `svc_host_builders.rs` (51 lines, sits under `svc_resolve_tddy_tools_path/` only for historical reasons) |

**The three ergonomics TODOs, re-read against the code.**

1. `owning_package` (`item_anchor.rs:79-92`) walks up from the given file looking for a `Cargo.toml`
   declaring `[package]` and says "is in no package" when a package-relative path is given. Fix the
   **message** only: `CLAUDE.md` forbids fallbacks without consent, so resolving the path silently is out.
2. The `Warm` RPC already exists (`packages/tddy-index-daemon/src/warm.rs`: waits on `GraphLoad` and
   streams server phases) but **nothing calls it**: `run-index-daemon` starts the daemon and stops, the
   daemon holds zero roots (`--ping` prints "N warm workspace(s)", `ping.rs:27-31`), and the first
   real request loads the crate graph (minutes). There is no CLI subcommand for `Warm`
   (`cli.rs:331-385` lists check/apply/anchors/status/verify/load/unload/plans only).
3. The `restructure snapshot` crash is **not explained by the empty `files` map**. The text
   `lsp server exited` is `tddy_lsp::error` (`packages/tddy-lsp/src/error.rs:31`), and the existing code
   issue `broken-restructure-anchors-empty-outline.md` documents the same text on the **cold path** of
   `anchors`. But the 2026-09-17 changeset says `RestructureCommand::Snapshot` needs no LSP client
   (`needs_lsp_client` false). So either snapshot is reaching for a server it should not need, or the
   agent's run was cold for a different reason. A pure test (a v2 header with an empty `files`, no
   server) settles it.

**Conflicts and claims.**

- `docs/dev/1-WIP/2026-09-17-restructure-refusal-truth-and-authoring-gates.md` (🚧 In Progress, with
  its PRD) is an active changeset on the same engine. It overlaps in the `snapshot` subcommand only and
  is, by the code, mostly shipped. It is **not wrapped**, which is its own debt; this change does not
  edit it.
- `packages/tddy-code-restructuring/docs/code-issues/broken-restructure-anchors-empty-outline.md` is
  `Claimed by: #537`, which **merged 2026-10-02**: the claim is stale, not in flight. Its remainder is
  "unowned". This change owns the cold-path part only if the snapshot investigation lands there.
- No open PR touches `packages/tddy-code-restructuring` or `packages/tddy-session-lifecycle` was found
  by the by-hand pass (the delegated `gh pr list` pass failed; re-check at `/green`).

**Constraints that carry into the plan.** Tests for engine operations are live-rust-analyzer
acceptance tests over generated fixture workspaces (`tests/harness/mod.rs`, 2,603 lines, one server at a
time, `assert_compiles` as the oracle). The comment-dropping defect of `extract_module`
(`2026-09-24-restructure-extract-drops-comments-...`) must not be inherited: a move authored by this
package copies byte ranges. The harness builders for new fixtures belong in a new harness module, not in
`mod.rs`.

**Open questions for the developer** are Decisions D1–D6 in the changeset.

## Exploration 1: the restructure engine — 2026-10-04

**Agent**: parent Grep/Glob/Read. Two Explore subagents (Sonnet 5.5, then Opus 5.5) were launched for
this pass and both ended with an API safeguard error before returning, so the pass was redone by hand.
**Scope**: how an operation is declared, validated, dispatched and tested; what a same-crate move can
reuse.

Sequence and findings:

1. `wc -l packages/tddy-code-restructuring/src/*.rs`: ~9.9k lines; largest `plan.rs` 1,497,
   `plan_store.rs` 1,128, `journal.rs` 1,043, `crate_move.rs` 992, `item_anchor.rs` 925. Subdirs:
   `crate_move/` (cluster, destination, header, manifest_edits, module_home, moving, preconditions,
   reexports, refusals, source_scan, survey, test_binary), `backends/rust/` (27 files, 14.3k lines with
   `rust.rs`'s 5,385), `runner/`, `verify/`, `plan/`.
2. `plan.rs:147-258`, the `RefactorKind` enum, read in full. Excerpt: `/// TypeScript Move to file.
   rust-analyzer has no whole-symbol move. MoveSymbol,` and, for the cross-crate ops, "engine-informed
   rather than engine-performed". `plan.rs:293`: `moves_across_crates` is `MoveModuleToCrate |
   MoveClusterToCrate` only.
3. `plan.rs:301-320`, `Reexport { Glob, Named, None }`: "rust-analyzer has no 'move item to another
   module' assist, so there is no engine to delegate the facade to and this package authors the `use`
   line itself".
4. `plan.rs:337-410`, `RefactorOp`: `deny_unknown_fields`; `to`, `name`, `reexport`, `also` fields exist;
   an unknown `op` string is a serde error before any server starts.
5. `plan/codec.rs:344-397`: validation rules tying `reexport`/`also`/`to_file` to the operations that
   take them. A new kind needs entries here.
6. `backends/rust.rs:67-73` (supported kinds) and `:1128-1175` (resolve dispatch). Excerpt: `if op.op ==
   RefactorKind::MoveModuleToCrate { ... crate_move::resolve(self, workspace, op)? }`. The text-answerable
   refusals run **before** a server is spawned (`:1128-1140`).
7. `crate_move.rs:1-60`: the design note quoted in the conclusions; `crate_move/` file list in step 1.
8. `item_anchor.rs:60-92`: `owning_package` and the "is in no package" error.
9. `tests/` directory: 38 acceptance files; `nested_module_move_acceptance.rs` (122 lines) read in
   full as the representative: `#[tokio::test(flavor = "multi_thread")]`, `a_workspace_whose_...()`
   fixture builders, `performing(&workspace, a_move_of(...)).await`, `assert_compiles(&workspace)`.
10. `tests/harness/mod.rs`: helpers `a_workspace_holding_files(&[(path, text)])` (`:2520`), `a_plan_of`
    (`:223`), `applying_the_plan_at` / `_with` (`:2350`, `:2359`), `checking_the_plan(fixture, plan, deep)`
    (`:2383`), `rewriting`, `holds`, `read`, `assert_compiles`, `assert_compiles_with_its_tests`,
    `assert_lints_clean`, `an_extract_module_of` (`:1634`), `an_item_anchor` (`:2312`).
11. `docs/code-issues/` of the package: `broken-restructure-anchors-empty-outline` (Claimed by #537,
    merged), `complexity-rust-facade-lines` (unclaimed), `dead-code-plan-filehint-modified`,
    `oversized-file-backends-rust` (partially fixed by #539), `oversized-file-test-binary`.
12. `.claude/skills/code-restructuring/references/plan-schema.md:76-121,193`: the op table (what must be
    updated per op) and the sentence "Rust has no whole-symbol move" that this change makes false.

## Exploration 2: the lifecycle targets — 2026-10-04

**Agent**: parent Grep/Glob/Read (the Explore subagent for this pass also failed on the same API error).
**Scope**: the four deferred #531 items on the current tree.

1. `grep -rnE "fn <name>\b"` and `grep -rnw <name>` over `packages/tddy-session-lifecycle/src` for the
   four M0.1 items: results in the conclusions table (definitions and call sites, e.g.
   `peer_has_no_such_session`: 1 definition, 6 call sites in `svc_provision_agent_clone.rs` and
   `svc_paired_codebase_teardown.rs`).
2. `grep -nE "pub(\(crate\))? (struct|enum|fn|trait) (SessionStdioEndpoint|ExecToolRoute|...)"`: the
   definitions in `seeded_clone_guard.rs:30,36,116,124` and `local_exec_tools.rs:29`.
3. `grep -nE "fn (write_claude_hooks_settings|resolve_start_session_claude_binary)"` and their callers.
4. `docs/dev/todo/2026-09-24-lifecycle-modules-to-re-parent-by-hand.md` read to line 60: the nine-row
   table and the "Not contiguous, so left where it is" note on `resolve_exec_tool_worktree`.
5. `find . -mindepth 2 -type d` under `src/`, then `find -name` for the nine destination files:
   `relay_idle.rs` and `session_admission_service.rs` are absent (they left in #526); `split_start.rs`,
   `svc_materialize_staged_attachment.rs`, `local_exec_tools.rs`, `svc_ensure_session_room_for_agents.rs`,
   `presenter_observer_task.rs` (crate root), `svc_host_builders.rs` and `svc_activity_ports.rs` exist.
   The session catalog left via commit `15374089` (`tddy-session-activity`).

## Exploration 3: the deferred-work cross-check and the ergonomics sites — 2026-10-04

**Agent**: parent Grep/Glob/Read.
**Scope**: backlog and code-issue scan; the daemon, snapshot and anchors error sites.

1. `ls docs/dev/todo | grep -iE "restructure|lifecycle|extract|re-parent|reparent|carve"`: 33 entries,
   classified in the changeset's Prerequisites.
2. Code issues: `packages/tddy-code-restructuring/docs/code-issues/` (5 records, one claimed, by a
   merged PR); `packages/tddy-session-lifecycle/docs/code-issues/` (11 records, none claimed).
3. `packages/tddy-index-daemon/src/{ping,warm,cli}.rs`: `--ping` prints the warm workspace count and
   does not distinguish loading from ready; `Warm` is an RPC with no CLI; `run-index-daemon` never warms.
4. `grep -rn "lsp server exited"`: only `tddy-lsp/src/error.rs:31`. `grep -rn "is in no package"`: only
   `item_anchor.rs:90`.
5. `docs/ft/coder/rust-code-restructuring.md` (728 lines; sections "Rust operations (v1)", "Item
   anchors", "Known limitations") is the feature document to update; `docs/ft/coder/changelog/` holds the
   per-entry files; no `docs/ft/` area exists for the lifecycle crate (package docs only).
6. `docs/dev/1-WIP/`: the 2026-09-17 restructure changeset and PRD (conflict note above).
