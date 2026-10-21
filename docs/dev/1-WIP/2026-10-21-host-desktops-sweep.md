# Changeset: Split the over-budget files the desktop stack grew, and shorten the start paths it wrote

**Date**: 2026-10-21
**Status**: 🚧 In Progress — planned; plans proven with `restructure check --deep`; awaiting the developer's review (`6e`)
**Type**: Refactor
**Complexity**: 5/5 — four crates (`tddy-daemon-kernel`, `tddy-host-service`, `tddy-screen-sharing`, `tddy-screenshare`), fifteen engine plans over three oversized files, eleven test modules and a crate-internal caller re-pointed, a 561-line `#[async_trait]` impl the engine cannot cut (an unsettled design: needs the developer's consent), and a cross-stack conflict with an open PR (#664)
**Stack**: `#host-desktops` 9/9 — the sweep node, now a plain PR. Base: `master` `ea5ba6fb1` (every node 1–8 has merged; see Dependencies)

## Initial Discovery

Full codebase exploration that grounded this plan:
[2026-10-21-host-desktops-sweep-initial-discovery.md](./2026-10-21-host-desktops-sweep-initial-discovery.md).

State A below is distilled from that file. The measurements, the per-file seam reading and the
engine's answers are there.

## Prerequisites

Backlog entries and code-issue records in this node's path. **✅ RESOLVED HERE** marks what this node's
wrap deletes; nothing is deleted at planning time.

| Item | Verdict | Note |
|---|---|---|
| [`oversized-file-config.md`](../../../packages/tddy-daemon-kernel/docs/code-issues/oversized-file-config.md) — 1,575 production lines | **✅ RESOLVED HERE** | Plans 01, 03–09: eight `move_item` / `extract_module` seams take `config.rs` under 500. Unclaimed (checked 2026-10-21); stays unclaimed until `6e` approves this plan |
| [`oversized-file-host-tooling.md`](../../../packages/tddy-host-service/docs/code-issues/oversized-file-host-tooling.md) — 806 | **✅ RESOLVED HERE** | Plans 10–12: three `move_item` seams under `host_tooling/` |
| [`oversized-file-service.md`](../../../packages/tddy-host-service/docs/code-issues/oversized-file-service.md) — 972 (the record said 961; **corrected in this commit**) | ⚠ **NARROWED — needs developer consent to close** | The engine reaches only the inherent `impl` (plans 12–13 in apply order, files `13-…` and `14-…`: 972 → ~673). The other 561 lines are one `#[async_trait] impl HostService`, which `extract_method` refuses inside (re-probed today, verbatim below). Closing it needs the trait bodies moved by hand into inherent `*_impl` methods, the way #653's seam C was. See Decisions |
| [`2026-10-20-hosts-mount-service-rs-grew-while-over-budget.md`](../todo/2026-10-20-hosts-mount-service-rs-grew-while-over-budget.md) | ✅ RESOLVED HERE **if** the `service.rs` record closes; otherwise ⚠ narrowed with it | the deferral entry #656 left for the same file; its 961 corrected to 972 in this commit |
| [`2026-10-20-screen-sharing-start-functions-over-length.md`](../todo/2026-10-20-screen-sharing-start-functions-over-length.md) | **✅ RESOLVED HERE** for `start_host_stream_impl` (76) and `start_bridge` (58); ⚠ `start_stream` (72) needs consent | plans 16, 17 by the engine; `start_stream` is in the `#[async_trait]` impl and refused. The entry's "do it together with the no-bridge entry" is not followed: that entry changes behaviour and is out of scope (D) |
| [`2026-10-20-screen-sharing-two-stand-in-bridges.md`](../todo/2026-10-20-screen-sharing-two-stand-in-bridges.md) | ⚠ **needs developer consent** — no engine operation reaches it | merging `FakeBridge` into `tests/common` moves a 1,378-line inline `#[cfg(test)]` suite that reads the library's private items into an integration-test crate, and deletes one stand-in. No operation moves an inline test module into `tests/`; it is an edit, not a move. Proposed: **Leave it**, with this reason recorded in the entry at wrap |
| [`2026-10-20-rdp-connect-config-builder-extraction.md`](../todo/2026-10-20-rdp-connect-config-builder-extraction.md) — `RdpClient::connect` 107 → 115 over the stack | ⚠ **needs developer consent** | measurably worse (C); in an `#[async_trait]` impl, the extraction of the `Config` literal is refused |
| `packages/tddy-screenshare/docs/changesets/2026-10-20-bridge-ready.md` dead link (`../bridge-readiness.md`) | **✅ RESOLVED HERE** at wrap | a new correction entry in `packages/tddy-screenshare/docs/changesets/` (Final Checklist); the wrapped entry is not edited |
| [`2026-10-20-rdp-test-support-module.md`](../todo/2026-10-20-rdp-test-support-module.md) | Leave it | test-helper deduplication across two integration binaries — an edit, not an engine move; not named in the backlog the developer scoped |
| [`2026-10-19-restructure-extract-method-refused-under-async-trait.md`](../todo/2026-10-19-restructure-extract-method-refused-under-async-trait.md) | ⚠ DURING — engine gap, reconfirmed | three new refusals today (`add_host_key`, `start_stream`, `RdpClient::connect`) added as evidence at wrap |
| The nine feature entries (`2026-10-18-screen-share-mid-session-resize`, `…-endpoint-channel-targets-matched-by-loopback-port`, `…-session-start-without-a-bridge-succeeds`, `…-x11-display-served-on-its-vnc-port-is-not-offered`, `…-x11-socket-dir-read-failure-lists-no-channels`, `2026-10-21-desktop-sign-in-pins-the-advertised-fingerprint`, `…-host-prompt-offered-auth-methods-is-never-sent`, `2026-10-20-rdp-empty-tls-public-key-fallback`) | Out of scope (D) | behaviour changes; they go to the developer at the TODO gate |

## Responsibility

The module layout of three files and the length of three functions, with no behaviour change:

- `packages/tddy-daemon-kernel/src/config.rs` → `config.rs` + eight modules under `config/`.
- `packages/tddy-host-service/src/host_tooling.rs` → `host_tooling.rs` + three modules under `host_tooling/`.
- `packages/tddy-host-service/src/service.rs` → `service.rs` + `service/builders.rs` + `service/peer_forwards.rs`
  (and, only with consent, the trait bodies — see Decisions).
- `start_host_stream_impl` (`tddy-screen-sharing`) and `start_bridge` (`tddy-screenshare`) shortened by
  `extract_method`.
- At wrap: the corrected dead link in `tddy-screenshare`'s change history (a new entry).

## Boundaries

- **No behaviour change**, no new symbol beyond module paths and the extracted helpers' names, no test
  semantics changed, no new dependency, no new Cargo feature.
- **Every move is the engine's** (`tddy-tools restructure`). No `git mv`, no hand-moved code. Any seam
  the engine refuses is listed with its refusal and waits for the developer's consent.
- Does not touch any of the nine feature entries (D), the bridges' protocols, the proto, the web.
- Does not split `screen_sharing_service.rs` (482 production lines, within budget) beyond what a stand-in
  merge would need — and that merge is not planned without consent.
- `ListenConfig` / `LiveKitConfig` stay in `config.rs`: their serde defaults (`blank_as_none`,
  `default_common_room_set_metadata_timeout_secs`, `:23–37`) are not contiguous with them, and the
  budget is met without them.

## Dependencies

What each node delivers that this one inherits. **All eight are MERGED into `master`** (squash commits
below; the stack landed on 2026-10-21, #658–#660 at 03:37Z). This node consumes nothing unmerged —
only the code they left — and its base is `master` (`ea5ba6fb1`).

| Node | PR (squash on `master`) | What it delivered | How this node meets it | This node does NOT |
|---|---|---|---|---|
| n1 service-split | #653 — MERGED `9d43b58d8` | `screen_sharing_service/{bridge_lifecycle,host_targets}.rs`, `bridge_config.rs` | extracts inside `host_targets.rs` | re-split the service module |
| n2 rdp-nla | #654 — MERGED `7d9a7d74e` | CredSSP + domain in `RdpClient::connect` | probe only (refused) | change the connector config |
| n3 vnc-security | #655 — MERGED `36fd1e9e9` | `tddy-vnc` security handshakes | nothing | touch `tddy-vnc` |
| n4 hosts-mount | #656 — MERGED `300c3da4c` | `ToolingCache` wiring in `service.rs` | moves `cached_tooling` with the builders | change the cache |
| n5 viewer-fullscreen | #657 — MERGED `17e240baa` | web viewer | nothing | touch `tddy-web` |
| n6 bridge-ready | #658 — MERGED `34c046a99` | `start_bridge` split, readiness wait in `start_stream` | extracts `publish_to_livekit` from `start_bridge` | change the readiness protocol |
| n7 x11-channels | #659 — MERGED `1be99ce05` | x11vnc resolver in `config.rs`, X11 channels in `host_tooling.rs` | moves them with their seams | change their behaviour |
| n8 sign-in-form | #660 — MERGED `fa9b42215` | desktop sign-in in `host_targets.rs` | extracts around `start_signed_in_bridge` | change sign-in |

## Draft PR contract

A restructure has no surface to publish ahead of its moves — the move is the implementation. The
draft publishes **the plan and the green baseline**: the plan files under
[`2026-10-21-host-desktops-sweep-plans/`](./2026-10-21-host-desktops-sweep-plans/), each proven with
`restructure check --deep`, the module layout below, and the baseline the applied tree is held to by
failing-test **name**.

## Green wave

**Wave:** 3 of 3 — the tail
**Greenable independently:** yes — nothing sits above it, and every node below is merged; its oracle is
the existing suites of the four packages it edits.
**Concurrent with:** none (the last node; every sibling is merged)
**Blocked by:** nothing. **Blocks:** nothing in this stack. Outside it: **#664** (`#provider-route` 3/9, draft) adds
`CliHarnessConfig` beside `ClaudeCliConfig` in `config.rs`, inside the `agent_cli` seam; whichever lands
second rebases across the move (see Decisions).

## Affected Packages

- **tddy-daemon-kernel**: [README.md](../../../packages/tddy-daemon-kernel/README.md) — `config` becomes a module directory; every `tddy_daemon_kernel::config::…` path keeps resolving through named facades
- **tddy-host-service**: [README.md](../../../packages/tddy-host-service/README.md) — `host_tooling` and `service` become module directories
- **tddy-screen-sharing**: [README.md](../../../packages/tddy-screen-sharing/README.md) — two helpers in `host_targets.rs`
- **tddy-screenshare**: [README.md](../../../packages/tddy-screenshare/README.md) — one helper in `bridge.rs`; the change-history link correction

## Related Feature Documentation

None — behaviour-preserving restructure. No PRD.

## Summary

`config.rs`, `host_tooling.rs` and `service.rs` are cut into topic modules by `tddy-tools restructure`;
`start_host_stream_impl` and `start_bridge` lose a helper each. Every path a caller outside the
library uses keeps resolving through a named facade (`reexport: "outside"`), so no file outside the
four packages is edited.

## Background

The stack grew three files that were already over the 500-production-line budget (config.rs +18 in
#659, host_tooling.rs +31 in #659, service.rs +36 in #656), each deferred with consent "until the stack
lands" because a sibling node touched it. It has landed (all eight merged on 2026-10-21), so the
`/pr-wrap` stack rule no longer forbids the splits. Its own start paths crossed the 60-line line.

## Scope

- [x] **Plan**: layout proposed, seam order fixed, plans snapshotted and proven with `check --deep`
- [ ] **Consent** (`6e`): the decisions below
- [ ] **Apply**: per plan, dry run then apply, `verify --against HEAD`, one commit per plan
- [x] **Baseline recorded** (480 passed, 0 failed across six packages)
- [ ] **Baseline after**: the same six packages back to the recorded names; `cargo check -p` the facade consumers
- [ ] **Code Quality**: `cargo fmt --check`; `cargo clippy -p <four packages> --all-targets -- -D warnings`
- [ ] **Documentation**: Final Checklist executed at wrap

## Restructuring

Production lines counted to the first `#[cfg(...test...)]`. "Projected" figures are this plan's
arithmetic over whole items plus the facade lines; "trial" figures, where given, are from a trial apply
that was fully reverted (see the discovery file § 6).

### State A

| File | Production lines | Items |
|---|---|---|
| `tddy-daemon-kernel/src/config.rs` | 1,575 (2,710 total) | 19 structs, 11 `resolve_*`, 5 `TDDY_*` env consts, 2 `impl DaemonConfig`, 11 test modules |
| `tddy-host-service/src/host_tooling.rs` | 806 (1,274 total) | probe trait + subprocess probe, desktop probes, unix readings, unix subprocess plumbing |
| `tddy-host-service/src/service.rs` | 972 (972 total) | struct, one inherent impl (304), one `#[async_trait]` trait impl (561) |
| `host_targets.rs` `start_host_stream_impl` | 76-line function | |
| `bridge.rs` `start_bridge` | 58-line function | |

### State B — module layout

| Module | Holds | Projected production lines |
|---|---|---|
| `tddy-daemon-kernel/src/config.rs` | `DaemonConfig` + `Default` + its first `impl`, `SupervisorClientConfig`, `TelegramConfig`, `ListenConfig`, `LiveKitConfig`, the serde defaults `DaemonConfig`/`LiveKitConfig` name, the `is_executable_file` twins, eight `mod` lines and their facades, the eleven test modules | **1,575 → ~400 + facades ≈ 440–460** |
| `config/agent_cli.rs` | `ClaudeCliConfig`, `CursorCliConfig`, their defaults, every `resolve_claude_*`/`resolve_cursor_*`, `CLAUDE_*`/`CURSOR_*` env consts, the shim and `PATH` search | ~317 |
| `config/daemon_methods.rs` | the second `impl DaemonConfig` (`load`, accessors, `apply_*_env_override(s)`, `validate_for_relay`) | ~222 |
| `config/sections.rs` | `RelayConfig`, `LocalConfig`, `KeyringConfig`, `GitConfig`, `SandboxCgroupConfig`, `IndexDaemonConfig` with their defaults | ~190 |
| `config/env_overrides.rs` | `merge_timing_env`, `merge_telegram_env` and their parsers | ~140 |
| `config/timings.rs` | `AgentWarmupConfig`, `SessionRoomConfig` with their defaults and consts | ~125 |
| `config/screen_sharing.rs` | `ScreenSharingConfig`, `resolve_{vnc,rdp,x11vnc}_binary_path` | ~75 |
| `config/sandbox_paths.rs` | `SANDBOX_CONFIG_*`, `sandbox_config_os_token`, `resolve_sandbox_config_path{,_with}` | ~69 |
| `config/access.rs` | `GitHubConfig`, `UserMapping`, `AllowedTool`, `AllowedAgent` | ~41 |
| `tddy-host-service/src/host_tooling.rs` | types, probe trait, argv builders, parsers, `SubprocessHostToolingProbe` and its `probe` impls, tests | **806 → ~384 + facades ≈ 395** |
| `host_tooling/subprocess.rs` | `ProbeOutput`, `StartedProbe`, `RunningProbe`, `start_probe` and the pipe plumbing (`#[cfg(unix)]`) | ~181 |
| `host_tooling/readings.rs` | `start_agent_probe`, `git_identity_of`, `github_cli_of`, `warn_if_failed`, … (`#[cfg(unix)]`) | ~147 |
| `host_tooling/desktop_probes.rs` | `start_desktop_probes`, `desktop_readings_of`, `desktop_probe_failed`, `warn_if_a_desktop_probe_failed` | ~94 |
| `tddy-host-service/src/service.rs` | struct, consts, the `#[async_trait] impl HostService` (561) | **972 → ~673 — still over** (≈ 180 only with the consented hand step, Decision 1) |
| `service/builders.rs` | `new`, the twelve `with_*`, accessors, `cached_tooling`, `pending_prompt_pump_count` | ~180 |
| `service/peer_forwards.rs` | `record_rpc_activity`, `eligible_instance_ids`, `classify_addressed_daemon_route`, `common_room_slot`, `rpc_served_by_peer`, `stream_served_by_peer` | ~121 |
| `host_targets.rs` `start_host_stream_impl` | + `start_bridge_for_desktop`, `find_host_target` | **76 → ~53** |
| `bridge.rs` `start_bridge` | + `publish_to_livekit` | **58 → ~33** |

Projected = whole-item line spans (doc comments and attributes included, measured by script) taken out of
the file, plus an estimate of the `pub use` facades `reexport: "outside"`/`"named"` leave (one grouped
line per seam, wrapped by `rustfmt`). `restructure check --budget 500` reports only the *current* size;
the real figures come from the apply, and the State B row is corrected then. No trial apply was run.

### Delta — plans in apply order

Definitions before their users; v1 (range-anchored) plans of a file before its v2 ones, because a v1
plan refuses on any hash drift of its file. One commit per plan.

| # | Plan | Operation | Anchor | `check --deep` (2026-10-21, `ea5ba6fb1`) |
|---|---|---|---|---|
| 1 | `01-config-agent-cli` | `extract_module` `agent_cli`, `to_file`, `reexport: named` | range `config.rs:689:1–1005:2` | ✅ no findings; widens 6 private helpers the tests call to `pub(crate)` |
| 2 | `03-config-screen-sharing` | `move_item` + `name: screen_sharing`, `reexport: outside` | items (4) | ✅ no findings |
| 3 | `04-config-sandbox-paths` | `move_item` + `name: sandbox_paths` | items (5) | ✅ no findings |
| 4 | `05-config-env-overrides` | `move_item` + `name: env_overrides` | items (6) | ✅ no findings |
| 5 | `06-config-timings` | `move_item` + `name: timings` | items (12) | ✅ no findings |
| 6 | `07-config-sections` | `move_item` + `name: sections` | items (12) | ✅ no findings |
| 7 | `08-config-access` | `move_item` + `name: access` | items (4) | ✅ no findings |
| 8 | `09-config-daemon-methods` | `move_item` + `name: daemon_methods` | item `<DaemonConfig>#2` | ✅ no findings |
| 9 | `10-host-tooling-desktop-probes` | `move_item` + `name: desktop_probes`, `reexport: outside` | items (4) | ✅ no findings |
| 10 | `11-host-tooling-readings` | `move_item` + `name: readings` | items (7) | ✅ no findings |
| 11 | `12-host-tooling-subprocess` | `move_item` + `name: subprocess` | items (10, incl. `<StartedProbe>`, `<RunningProbe>`) | ✅ no findings |
| 12 | `14-service-builders` | `extract_module` `builders`, `to_file`, `reexport: none` | range `service.rs:108:5–287:6` | ✅ no findings |
| 13 | `13-service-peer-forwards` | `extract_module` `peer_forwards`, `to_file`, `reexport: none` | items (6 members) | ✅ no findings |
| 14 | `16-host-targets-start-host-stream` | 2 × `extract_method`, bottom-up | item + relative ranges | ✅ no findings |
| 15 | `17-bridge-start-bridge` | `extract_method` `publish_to_livekit` | item + relative range | ✅ no findings |
| — | `probe-a-start-stream` | `extract_method` `start_session_bridge` | `screen_sharing_service.rs:416–449` | ❌ refused (below) |
| — | `probe-b-rdp-connector-config` | `extract_method` `connector_config` | `rdp_client.rs:155–198` | ❌ refused |
| — | `probe-c-service-add-host-key` | `extract_method` | `service.rs:683–692` | ❌ refused |

Refusal text, identical for all three probes:

```
this seam cannot be cut here: rust-analyzer offers no "extract into function" assist for the given range (it offered: Extract into variable)
```

Two more engine answers shaped the plan (not refusals of a seam, but of an anchor or a cut):

```
plan is malformed: `tddy_daemon_kernel::config::is_executable_file` names 2 items in packages/tddy-daemon-kernel/src/config.rs — qualify it with the trait, as `tddy_daemon_kernel::<config as Trait>::is_executable_file`
```
(`restructure anchors --items` on the two `#[cfg(unix)]`/`#[cfg(not(unix))]` twins — worked around with a
range-anchored `extract_module`, plan 1.)

```
this seam cannot be cut here: the seam would separate an attribute from the item its string names: `default_claude_cli_binary_path`, named by the attribute on line 811 and declared on line 689; …
```
(the first `move_item` draft of plan 1, which left the serde defaults behind — correct refusal; the range
seam carries them.)

```
plan is malformed: `tddy_host_service::service::HostServiceImpl::host_keypair` names 2 items in packages/tddy-host-service/src/service.rs — qualify it with the trait, …
```
(`anchors --items` on a method that shares its name with a field — worked around with a range anchor,
plan 12.)

`restructure verify --against HEAD` applies after each apply; nothing has been applied, so it has not run.

## Callers

| | |
|---|---|
| External importers | `config`: 17 files across `tddy-agent-launch`, `tddy-daemon-rpc`, `tddy-host-service`, `tddy-screen-sharing`, `tddy-session-lifecycle`, `tddy-session-split`, `tddy-telegram-control` and the crate's own `tests/`; the moved structs also from `tddy-daemon`, `tddy-daemon-auth`, `tddy-daemon-livekit`, `tddy-code-restructuring`. `host_tooling`: `github_cli.rs` (same crate). `service`: none (methods reached through the type) |
| Facade | `reexport: "outside"` on every `move_item` (a named `pub use` in the old parent for what something outside the library reaches); `reexport: "named"` on the `config` `extract_module`; `none` on the two `service` `extract_module`s |
| Consequence | no file outside the four packages changes. In-crate callers (the eleven `config` test modules, `github_cli.rs`) are re-pointed by the engine |

**The apply's compile gate does not see the facade consumers**: it checks the packages owning a changed
file. After the last plan, `cargo check -p` each consumer crate listed above, once.

## Visibility

Widenings `check --deep` reported (the apply will report the final set; this table is completed then):

| Item | Was | Is | Why |
|---|---|---|---|
| `is_wrapper_shim_dir`, `ClaudeBinaryChoice`, `choose_claude_binary`, `find_real_agent_in`, `CursorBinaryChoice`, `choose_cursor_binary` | private | `pub(crate)` | `config.rs` tests call them; `extract_module` widens to `pub(crate)` |
| `resolve_sandbox_config_path_with` | private | `pub(super)` | `config.rs` tests |
| `parse_env_bool`, `merge_telegram_env` | private | `pub(super)` | called from `config.rs` (impl `DaemonConfig`) and tests |
| `MIN_WORKTREE_ROOM_INTERVAL_MS`, `SessionRoomConfig::validate` | private | `pub(super)` | `DaemonConfig` validation in `config.rs` |
| `desktop_readings_of`, `warn_if_a_desktop_probe_failed` | private | `pub(super)` | the two `probe` impls in `host_tooling.rs` |
| `git_config_value`, `github_cli_of` | private | `pub(super)` | `host_tooling.rs` tests |
| `StartedProbe::running`, `RunningProbe::child` (fields) | private | `pub(super)` | `host_tooling.rs` tests |
| `common_room_slot`, `rpc_served_by_peer`, `stream_served_by_peer` | private | `pub(crate)` | the trait impl in `service.rs` calls them |
| `cached_tooling` | private | `pub(crate)` | `get_host_tooling` in the trait impl |

None of these privacies enforced an invariant that the moves weaken: each is a helper whose callers now
sit one module up.

## Baseline

Recorded 2026-10-21 on `master` `ea5ba6fb1` (after the fast-forward), one package at a time, with
`./dev cargo test --no-fail-fast -p <pkg> -- --test-threads=1` — the exact test command `./test -p` runs,
without its prerequisite build of the unrelated binaries (`tddy-coder-cli`, `tddy-livekit`, the sandbox
runner, …): this worktree's `target/` was removed mid-session for disk, and that build would have taken
the machine under the 15 GB floor. A first set of `./test -p` runs on `fa9b42215` (before the
fast-forward) gave the same counts for `tddy-daemon-kernel` (154), `tddy-host-service` (199),
`tddy-screen-sharing` (79), `tddy-screenshare` (9) and `tddy-daemon-extensions` (28 — 22+6 then; #635
moved one of its tests, 23+5 now).

| Gate | Before | After |
|---|---|---|
| `tddy-daemon-kernel` — 9 binaries (unit 115; `common_room_forwarding` 12, `first_login_enrolment_acceptance` 4, `kernel_surface_acceptance` 4, `peer_calls` 3, `relay_mode_acceptance` 4, `spawn_as_user_github_tokens` 1, `telegram_extraction_shape` 9, `x11vnc_binary_resolution` 2) | **154 passed / 0 failed / 0 ignored** | |
| `tddy-host-service` — unit 177; `askpass_prompt_rpc` 11, `clone_grants` 5, `github_cli_environment` 1, `peer_routing` 2, `stream_host_prompts_rpc` 3 | **199 / 0 / 0** | |
| `tddy-screen-sharing` — unit 19 (the inline `FakeBridge` suite); `bridge_readiness_acceptance` 6, `dead_vnc_service_removed` 3, `host_desktop_sign_in_acceptance` 15, `screen_sharing_record_propagation` 3, `screen_sharing_records_unit` 10, `screen_sharing_service_acceptance` 8, `session_vault_target_store_acceptance` 6, `x11_channels_acceptance` 9 | **79 / 0 / 0** | |
| `tddy-screenshare` — unit 9 | **9 / 0 / 0** | |
| `tddy-daemon-extensions` — unit 23; `livekit_is_a_satellite` 5 | **28 / 0 / 0** | |
| `tddy-rdp` (no plan edits it; consumer of `tddy-screenshare`, and the probe-B file) — `rdp_client_integration` 5, `rdp_inspect_integration` 2, `rdp_nla_integration` 4 | **11 / 0 / 0** | |
| facade consumers (`cargo check -p` each, after the last plan) | not run at planning | |

**No failing test.** The failing set the applied tree is held to is therefore empty.

**Known load-sensitive tests** (green here, under a load average of 260–280; a failure of one of these
alone, under load, is re-run alone before it is called a regression):

- `tddy-host-service` probe tests that end in `… did not answer within 5s` (`host_tooling.rs:754`, the
  `PROBE_TIMEOUT` kill-and-reap path; also `ssh_agent.rs:358`'s agent timeout): a real `gh`/`git`/`ssh-add`
  child that is slow to start under load is killed at the deadline and reported as unanswered.
  Workaround: re-run the single test with `--test-threads=1` on a quieter machine.
- `tddy-screen-sharing` `tests/x11_channels_acceptance.rs` (nine tests, `StandInX11vnc` polled every
  20 ms, `AN_X11VNC_LOGS_ITS_LAUNCH_WITHIN` 10 s) and `tests/bridge_readiness_acceptance.rs` (six tests,
  `StandInBridge` readiness lines, `A_START_FINISHES_WITHIN` 20 s): `/bin/sh` stand-ins whose deadlines
  a starved scheduler can miss. Workaround: the same — alone, `--test-threads=1`.

## Plan

[`2026-10-21-host-desktops-sweep-plans/`](./2026-10-21-host-desktops-sweep-plans/) — one JSONL file per
seam, numbered in apply order; the `probe-*` files are the refused seams, kept as the evidence.

## Decisions & Trade-offs

1. **`service.rs` cannot close by the engine alone — needs consent.** 561 of its 972 lines are one
   `#[async_trait] impl HostService`, which can neither be split across modules (`E0119`/`E0046`) nor
   have its bodies extracted (probe C, refused today exactly as on 2026-10-19). Plans 12–13 take it to
   ~673. **Option A (recommended):** consent to the #653 seam-C precedent — the nine trait bodies moved
   **by hand** into inherent `pub(crate) async fn *_impl` methods (byte-identical, script-compared, each
   trait method left as a one-line delegator), then three more engine `extract_module`s group them
   (`service/host_keys.rs` ~200: `add_host_key`, `list_host_key_candidates`, `list_ssh_config_hosts`;
   `service/prompts.rs` ~110; `service/listings.rs` ~135 with `stream_host_stats` ~96 beside it or in
   `service/stats.rs`) → `service.rs` ≈ 180. **Option B:** land plans 12–13 only and **narrow** the record
   to ~673, leaving the trait impl for the engine gap's fix. The same consent covers `start_stream` (probe
   A) and `RdpClient::connect` (probe B): with A, each gets an inherent `*_impl` by hand and then a plain
   `extract_method`; with B, both stay over 60 and their todo entries are narrowed.
2. **The two stand-in bridges: Leave it (recommended), or consent to a hand edit.** No operation moves
   an inline `#[cfg(test)]` suite into `tests/`, and merging means deleting one stand-in and extending the
   other — test-code edits, not moves. The record of why is written into the entry at wrap.
3. **`config/` is eight modules, not the record's one.** The record's single `binary_paths` seam
   (426 lines) leaves ~1,150; the budget needs ~1,075 out, so the serde sections, the env merge and the
   second `impl DaemonConfig` go too. Names are a review point (`timings`, `sections`,
   `daemon_methods` are the weakest). `ListenConfig`/`LiveKitConfig` stay — their serde defaults sit at
   `:23–37`, and moving them is two more plans for ~90 lines the budget does not need.
4. **#664 (`#provider-route` 3/9, open draft) conflicts with plan 1.** It inserts `CliHarnessConfig` and an
   `impl DaemonConfig` directly after `ClaudeCliConfig`. Whichever lands second moves that block into
   `config/agent_cli.rs` on rebase. Alternative: hold plan 1 until #664 lands (config.rs then ends ~760
   without it — still over budget, so the record would only be narrowed).
5. **Widenings are the tests' doing.** Every reported widening is a private helper a `config.rs` /
   `host_tooling.rs` test module (or the `service.rs` trait impl) calls from one module up; none guarded
   an invariant. `extract_module` widens to `pub(crate)`, `move_item` to `pub(super)`.
6. **`reexport: "outside"` hides consumers from the compile gate.** The apply checks only packages it
   wrote; after the last plan `cargo check -p` the eleven consumer crates in `## Callers` once.
7. **Score 5 is a warning that the boundary may be wrong** (`changeset-doc`): this is four crates and
   fifteen plans in one PR. A split by crate (config / host-service / start functions) is three
   reviewable PRs with no edges between them; kept as one node because it is the stack's single
   planned sweep. The developer may prefer the split.

## Final Checklist

Tasks executed by `/wrap-context-docs` before this changeset is deleted.

- [ ] `packages/tddy-daemon-kernel/docs/changesets/2026-10-21-config-split.md` — release-note file with before/after numbers
- [ ] `packages/tddy-host-service/docs/changesets/2026-10-21-host-tooling-and-service-split.md` — release-note file
- [ ] `packages/tddy-screen-sharing/docs/changesets/2026-10-21-start-host-stream-helpers.md` — release-note file
- [ ] `packages/tddy-screenshare/docs/changesets/2026-10-21-start-bridge-helper.md` — release-note file
- [ ] `packages/tddy-screenshare/docs/changesets/2026-10-21-bridge-ready-link-correction.md` — correction to `2026-10-20-bridge-ready`: its "Bridge start report" link now lives at `../../../tddy-daemon-extensions/docs/bridge-readiness.md` (the wrapped entry is not edited)
- [ ] `docs/dev/changesets/2026-10-21-host-desktops-sweep.md` — cross-package history entry, with each closed record's final measurement
- [ ] Delete the closed code-issue records and todo entries marked ✅ RESOLVED HERE above (after their final rows are in the history entry); narrow the ones marked ⚠
- [ ] `packages/tddy-daemon-kernel/docs/daemon-kernel.md` § module rules (`:31`) and the `#keyring` lifetime note (`:191`) — "`config.rs` is the first exception … far over its size budget" → `config` is a module directory under budget; name the eight modules and what each holds
- [ ] `packages/tddy-host-service/docs/host-service.md` file table (`:12`) — `service.rs` no longer holds the `with_*` builders or the peer routing → add rows for `service/builders.rs` and `service/peer_forwards.rs`; add `host_tooling/{desktop_probes,readings,subprocess}.rs` beside the `host_tooling.rs` entry (`:159`)
- [ ] `packages/tddy-host-service/docs/host-add-key.md` (`:29`) — "`service.rs` | the four handlers, their routing and their auth" → the routing is `service/peer_forwards.rs`
- Triaged as still true (no item): `daemon-kernel.md:271–273` and `host-tooling-probe.md:476/558` (`resolve_*_binary_path` keep their `tddy_daemon_kernel::config` path through the facade); `host-tooling-probe.md` prose naming `host_tooling.rs` (the module, and its tests stay in that file) and `desktop_readings_of` / `start_desktop_probes` by name only
- [ ] Re-run the doc triage (`grep -rn` of every moved symbol and old path over the four packages' README/docs) against what landed
