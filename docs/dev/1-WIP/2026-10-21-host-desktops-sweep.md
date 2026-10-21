# Changeset: Split `config.rs` into per-concern modules, under the 500-line budget

**Date**: 2026-10-21
**Status**: 🚧 In Progress — planned and reviewed (`6e`, 2026-10-21); plans proven with `restructure check --deep`; not applied
**Type**: Refactor
**Complexity**: 3/5 — one crate (`tddy-daemon-kernel`), eight engine plans moving ~1,175 lines out of one file inside the crate, eleven in-file test modules re-pointed, a wide consumer fan-out kept stable by named facades (eleven crates to `cargo check`), and a known rebase conflict for an open PR (#664)
**Stack**: `#host-desktops` 9/10 — first of the two sweep PRs, a plain PR. Base: `master` `ea5ba6fb1`. Sibling, independent: `feature/host-desktops/sweep-host-service` (`#host-desktops` 10/10)

## Initial Discovery

[2026-10-21-host-desktops-sweep-initial-discovery.md](./2026-10-21-host-desktops-sweep-initial-discovery.md):
the sweep's measurements, the `config.rs` seam reading, the engine's answers and the independence proof.

## Developer decisions

Recorded from the developer's review of the plan (`6e`), relayed by the orchestrator on 2026-10-21:

- **2026-10-21 — PR shape:** "TWO PRs, independent siblings both based on master, no edges between them,
  each with its own changeset, discovery and plans." This PR is **PR A, `config.rs` only** (plans 01 and
  03–09). Everything else of the sweep moved to the sibling `feature/host-desktops/sweep-host-service`.
- **2026-10-21 — #664 conflict:** "the sweep lands FIRST. Plan 01 stays." The merge note for #664's owner
  is under `## Merge note for #664` below, and in this PR's description.
- **2026-10-21 — hand moves (consented)** and **stand-in bridges (leave)**: both concern the sibling PR
  only. This PR has no hand move: every line it moves is moved by the engine.

## Prerequisites

| Item | Verdict | Note |
|---|---|---|
| [`oversized-file-config.md`](../../../packages/tddy-daemon-kernel/docs/code-issues/oversized-file-config.md) — 1,575 production lines | **✅ RESOLVED HERE** — projected **~440–460** (≤ 500) | Plans 01, 03–09. Unclaimed (checked 2026-10-21) and left unclaimed in the record until the apply. If the applied count ends above 500, the record is **narrowed** with the measurement instead, and the wrap does not delete it |
| [`2026-10-19-restructure-extract-method-refused-under-async-trait.md`](../todo/2026-10-19-restructure-extract-method-refused-under-async-trait.md) | not in this PR's path | the sibling carries it |

## Responsibility

The module layout of `packages/tddy-daemon-kernel/src/config.rs`: `config.rs` plus eight modules under
`src/config/`, cut by `tddy-tools restructure`, with every `tddy_daemon_kernel::config::…` path a caller
outside the library uses kept stable through named facades.

## Boundaries

- **No behaviour change**, no new symbol beyond module paths, no test semantics changed, no new dependency.
- **Every move is the engine's.** No `git mv`, no hand-moved code. A refusal at apply stops the run and goes
  to the developer.
- Does not edit any file outside `tddy-daemon-kernel` (consumers compile through the facade), and does not
  touch `tddy-host-service`, `tddy-screen-sharing`, `tddy-screenshare` or `tddy-rdp` (the sibling PR's).
- `ListenConfig` / `LiveKitConfig` stay in `config.rs`: their serde defaults (`blank_as_none`,
  `default_common_room_set_metadata_timeout_secs`, `:23–37`) are not contiguous with them, and the budget is
  met without them.

## Dependencies

Every node of `#host-desktops` is **MERGED into `master`**; this PR consumes nothing unmerged, only the code
they left. The node that grew this file is #659.

| Node | PR — squash on `master` | What it left in this PR's path | This PR does NOT |
|---|---|---|---|
| n1 service-split | #653 — `9d43b58d8` | nothing in `config.rs` | — |
| n2 rdp-nla | #654 — `7d9a7d74e` | nothing in `config.rs` | — |
| n3 vnc-security | #655 — `36fd1e9e9` | nothing in `config.rs` | — |
| n4 hosts-mount | #656 — `300c3da4c` | nothing in `config.rs` | — |
| n5 viewer-fullscreen | #657 — `17e240baa` | nothing in `config.rs` | — |
| n6 bridge-ready | #658 — `34c046a99` | nothing in `config.rs` | — |
| n7 x11-channels | #659 — `1be99ce05` | `ScreenSharingConfig.x11vnc_binary_path`, `.x11_socket_dir`, `resolve_x11vnc_binary_path` (+18) | change them — plan 03 moves them as they are |
| n8 sign-in-form | #660 — `fa9b42215` | nothing in `config.rs` | — |
| sibling 10/10 | `feature/host-desktops/sweep-host-service` (unmerged) | nothing — **no edge** (discovery § 7) | consume or edit anything of it |

## Draft PR contract

A restructure publishes no surface ahead of its moves. The draft is **the plan and the green baseline**:
the eight plan files under [`2026-10-21-host-desktops-sweep-plans/`](./2026-10-21-host-desktops-sweep-plans/),
each proven with `restructure check --deep`, the module layout below, and the baseline the applied tree is
held to by failing-test name.

## Green wave

**Wave:** 3 of 3 — the tail of `#host-desktops`
**Greenable independently:** yes — every node below is merged and the sibling has no edge to it; its oracle
is `tddy-daemon-kernel`'s own suite plus a `cargo check` of the facade consumers.
**Concurrent with:** the sibling `feature/host-desktops/sweep-host-service` (10/10), fully parallel
**Blocked by:** nothing. **Blocks:** nothing in this stack. Outside it, #664 rebases across plan 01 (see the merge note).

## Merge note for #664

For the owner of **#664** (`#provider-route` 3/9, `feature/provider-route/opencode-cli`). The developer
decided this PR lands first. After it does, #664's rebase conflicts in `config.rs`, and its additions go
where the code they sit beside now lives:

- the `DaemonConfig.cli_harnesses` field (#664's `config.rs:470`) and its `Default` line (`:573`) **stay in
  `packages/tddy-daemon-kernel/src/config.rs`**, beside the other `DaemonConfig` fields and the `impl Default
  for DaemonConfig`, which this PR does not move;
- the new `CliHarnessConfig` struct and its `impl DaemonConfig { fn cli_harness_binary … }` (#664's
  `config.rs:817–840`, inserted after `ClaudeCliConfig`) **belong in `packages/tddy-daemon-kernel/src/config/agent_cli.rs`**,
  after `ClaudeCliConfig`, where `ClaudeCliConfig`, `CursorCliConfig` and every `resolve_*_binary_path` for
  the agent CLIs now live. Add `CliHarnessConfig` to the `pub use agent_cli::{…}` facade in `config.rs` so
  `tddy_daemon_kernel::config::CliHarnessConfig` keeps resolving.

## Affected Packages

- **tddy-daemon-kernel**: [README.md](../../../packages/tddy-daemon-kernel/README.md) — `config` becomes a module directory; every `tddy_daemon_kernel::config::…` path keeps resolving through named facades
  - [daemon-kernel.md](../../../packages/tddy-daemon-kernel/docs/daemon-kernel.md) — module rules (`:31`) and the `#keyring` note (`:191`)

## Related Feature Documentation

None — behaviour-preserving restructure. No PRD.

## Summary

`config.rs` (1,575 production lines) is cut into eight topic modules by `tddy-tools restructure`. Every path
a caller outside the library uses keeps resolving through a named facade (`reexport: "outside"`, and
`"named"` on the one `extract_module`), so no file outside `tddy-daemon-kernel` is edited.

## Background

`config.rs` was over budget before the stack (1,557) and #659 grew it by 18, deferred with consent "until the
stack lands" because a sibling node touched it. The stack has landed.

## Scope

- [x] **Plan**: layout proposed, seam order fixed, plans snapshotted and proven with `check --deep`
- [x] **Consent** (`6e`, 2026-10-21): see `## Developer decisions`
- [ ] **Apply**: per plan, dry run then apply, `verify --against HEAD`, one commit per plan
- [ ] **Baseline after**: `tddy-daemon-kernel` back to the recorded names; `cargo check -p` the eleven facade consumers
- [ ] **Code Quality**: `cargo fmt --check`; `cargo clippy -p tddy-daemon-kernel --all-targets -- -D warnings`
- [ ] **Documentation**: Final Checklist executed at wrap

## Restructuring

Production lines counted to the first `#[cfg(...test...)]`. Projected = whole-item spans (doc comments and
attributes included, measured by script) taken out, plus an estimate of the facade lines. The applied
figures replace these.

### State A

| File | Production lines | Items |
|---|---|---|
| `tddy-daemon-kernel/src/config.rs` | 1,575 (2,710 total) | 19 structs, 11 `resolve_*`, 5 `TDDY_*` env consts, 2 `impl DaemonConfig`, 11 test modules |

### State B — module layout

| Module | Holds | Projected production lines |
|---|---|---|
| `config.rs` | `DaemonConfig` + `Default` + its first `impl`, `SupervisorClientConfig`, `TelegramConfig`, `ListenConfig`, `LiveKitConfig`, the serde defaults those name, the `is_executable_file` twins, eight `mod` lines and their facades, the eleven test modules | **1,575 → ~400 + facades ≈ 440–460** |
| `config/agent_cli.rs` | `ClaudeCliConfig`, `CursorCliConfig`, their defaults, every `resolve_claude_*` / `resolve_cursor_*`, `CLAUDE_*` / `CURSOR_*` env consts, the shim and `PATH` search | ~317 |
| `config/daemon_methods.rs` | the second `impl DaemonConfig` (`load`, accessors, `apply_*_env_override(s)`, `validate_for_relay`) | ~222 |
| `config/sections.rs` | `RelayConfig`, `LocalConfig`, `KeyringConfig`, `GitConfig`, `SandboxCgroupConfig`, `IndexDaemonConfig` with their defaults | ~190 |
| `config/env_overrides.rs` | `merge_timing_env`, `merge_telegram_env` and their parsers | ~136 |
| `config/timings.rs` | `AgentWarmupConfig`, `SessionRoomConfig` with their defaults and consts | ~125 |
| `config/screen_sharing.rs` | `ScreenSharingConfig`, `resolve_{vnc,rdp,x11vnc}_binary_path` | ~75 |
| `config/sandbox_paths.rs` | `SANDBOX_CONFIG_*`, `sandbox_config_os_token`, `resolve_sandbox_config_path{,_with}` | ~69 |
| `config/access.rs` | `GitHubConfig`, `UserMapping`, `AllowedTool`, `AllowedAgent` | ~41 |

### Delta — plans in apply order

Plan 01 is range-anchored (a v1 plan refuses on any hash drift of its file), so it runs first; 03–09 are
item-anchored. One commit per plan.

| # | Plan | Operation | Anchor | `check --deep` (2026-10-21, `ea5ba6fb1`) |
|---|---|---|---|---|
| 1 | `01-config-agent-cli` | `extract_module` `agent_cli`, `to_file`, `reexport: named` | range `config.rs:689:1–1005:2` | ✅ no findings |
| 2 | `03-config-screen-sharing` | `move_item` + `name: screen_sharing`, `reexport: outside` | items (4) | ✅ no findings |
| 3 | `04-config-sandbox-paths` | `move_item` + `name: sandbox_paths` | items (5) | ✅ no findings |
| 4 | `05-config-env-overrides` | `move_item` + `name: env_overrides` | items (6) | ✅ no findings |
| 5 | `06-config-timings` | `move_item` + `name: timings` | items (12) | ✅ no findings |
| 6 | `07-config-sections` | `move_item` + `name: sections` | items (12) | ✅ no findings |
| 7 | `08-config-access` | `move_item` + `name: access` | items (4) | ✅ no findings |
| 8 | `09-config-daemon-methods` | `move_item` + `name: daemon_methods` | item `<DaemonConfig>#2` | ✅ no findings |

There is no plan 02: a second `move_item` into `agent_cli` became unnecessary once plan 01 took the range.
Engine answers that shaped plan 01 (both verbatim in the discovery record § 6): the cfg twins of
`is_executable_file` cannot be named by an `items` anchor (`… names 2 items …`), and a `move_item` that left
the serde defaults behind was correctly refused (`… the seam would separate an attribute from the item its
string names …`).

## Callers

| | |
|---|---|
| External importers | 17 files across `tddy-agent-launch`, `tddy-daemon-rpc`, `tddy-host-service`, `tddy-screen-sharing`, `tddy-session-lifecycle`, `tddy-session-split`, `tddy-telegram-control` and the crate's own `tests/`; the moved structs also from `tddy-daemon`, `tddy-daemon-auth`, `tddy-daemon-livekit`, `tddy-code-restructuring` |
| Facade | `reexport: "outside"` on every `move_item`; `reexport: "named"` on the `extract_module` |
| Consequence | no file outside `tddy-daemon-kernel` changes. The eleven in-file test modules (`use super::*;`) are re-pointed by the engine |

**The apply's compile gate does not see the facade consumers**: it checks only the package it wrote. After
the last plan, `cargo check -p` each of the eleven consumer crates above, one at a time.

## Visibility

Widenings `check --deep` reported (completed from the apply):

| Item | Was | Is | Why |
|---|---|---|---|
| `is_wrapper_shim_dir`, `ClaudeBinaryChoice`, `choose_claude_binary`, `find_real_agent_in`, `CursorBinaryChoice`, `choose_cursor_binary` | private | `pub(crate)` | `config.rs` tests call them; `extract_module` widens to `pub(crate)` |
| `resolve_sandbox_config_path_with` | private | `pub(super)` | `config.rs` tests |
| `parse_env_bool`, `merge_telegram_env` | private | `pub(super)` | called from the `impl DaemonConfig` and tests |
| `MIN_WORKTREE_ROOM_INTERVAL_MS`, `SessionRoomConfig::validate` | private | `pub(super)` | `DaemonConfig` validation |

None of these privacies enforced an invariant: each is a helper whose callers now sit one module up.

## Baseline

Recorded 2026-10-21 on `master` `ea5ba6fb1` (still `master`'s tip at this revision: `git fetch origin
master` shows no new commit), with `./dev cargo test --no-fail-fast -p tddy-daemon-kernel -- --test-threads=1`,
the test command `./test -p` runs, without its prerequisite build of unrelated binaries (disk). A `./test -p
tddy-daemon-kernel` run on `fa9b42215` gave the same count.

| Gate | Before | After |
|---|---|---|
| `tddy-daemon-kernel` — unit 115; `common_room_forwarding` 12, `first_login_enrolment_acceptance` 4, `kernel_surface_acceptance` 4, `peer_calls` 3, `relay_mode_acceptance` 4, `spawn_as_user_github_tokens` 1, `telegram_extraction_shape` 9, `x11vnc_binary_resolution` 2 | **154 passed / 0 failed / 0 ignored** | |
| facade consumers — `cargo check -p` each of the eleven crates in `## Callers` | not run at planning | |

**No failing test**; the failing set the applied tree is held to is empty. No known-flaky test in this
crate's suite.

## Plan

[`2026-10-21-host-desktops-sweep-plans/`](./2026-10-21-host-desktops-sweep-plans/) — eight JSONL files,
numbered in apply order. The host-service, start-function and probe plans moved to the sibling PR.

## Decisions & Trade-offs

1. **Eight modules, not the record's one.** The record's single `binary_paths` seam (426 lines) leaves
   ~1,150; the budget needs ~1,075 out, so the serde sections, the env merge and the second `impl
   DaemonConfig` go too. Names (`timings`, `sections`, `daemon_methods`) were reviewed at `6e` without change.
2. **The `agent_cli` seam is an `extract_module` over a range**, not a `move_item`, because the cfg twins of
   `is_executable_file` sit between the serde defaults and the structs they serve, and no `items` anchor can
   name the twins. A range-anchored plan refuses on any drift of `config.rs`, so it runs first.
3. **Widenings are the tests' doing** (table above).
4. **`reexport: "outside"` hides consumers from the compile gate**, hence the explicit `cargo check` step.
5. **#664 rebases across plan 01**, by decision; the note above is what its owner needs.

## Final Checklist

Tasks executed by `/wrap-context-docs` before this changeset is deleted.

- [ ] `packages/tddy-daemon-kernel/docs/changesets/2026-10-21-config-split.md` — release-note file: what moved where, what stayed and why (`ListenConfig`/`LiveKitConfig`), before/after production lines and the 154-test baseline
- [ ] `docs/dev/changesets/2026-10-21-host-desktops-sweep-config.md` — cross-package history entry with the record's final measurement
- [ ] Delete `packages/tddy-daemon-kernel/docs/code-issues/oversized-file-config.md` if the applied count is ≤ 500 (narrow it otherwise)
- [ ] `packages/tddy-daemon-kernel/docs/daemon-kernel.md` § module rules (`:31`) and the `#keyring` note (`:191`) — "`config.rs` is the first exception … far over its size budget" → `config` is a module directory under budget; name the eight modules and what each holds
- Triaged as still true (no item): `daemon-kernel.md:271–273` (`resolve_vnc_binary_path` / `resolve_rdp_binary_path` / `resolve_x11vnc_binary_path` keep their `tddy_daemon_kernel::config` path through the facade)
- [ ] Re-run the doc triage (`grep -rn` of every moved symbol and old path over the package's README/docs) against what landed
