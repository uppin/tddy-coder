# Initial discovery: `#host-desktops` sweep, host service and start paths (10/10)

**Date**: 2026-10-21
**Tree measured**: `master` `fa9b42215` — the squash of #660. Its tree is byte-identical to the
`feature/host-desktops/sign-in-form` tip `5f802d90f` (`git diff 5f802d90f fa9b42215` is empty).
**Re-validated on** `master` `ea5ba6fb1` after a fast-forward over `0bee1bf79` (#635, `#satellite`
13/13) and `ea5ba6fb1` (#688, CI only). Neither touches a file any plan names
(`git diff --quiet fa9b42215 ea5ba6fb1 -- config.rs host_tooling.rs service.rs packages/tddy-screen-sharing packages/tddy-screenshare packages/tddy-rdp`
is clean); #635 does change `tddy-host-service/src/peer_routing.rs` and `tddy-daemon-extensions/src`,
so the baselines of those two packages are recorded on `ea5ba6fb1`. Every measurement below holds on
both trees.
**Baselines compared against**: the stack's start `ffe71a895` (the parent of #653, `#host-desktops`
1/8) and the merge base the orchestrator named, `569c65456` (after #657, before #658).

This is the code-discovery record behind
[2026-10-21-host-desktops-sweep-host-service.md](./2026-10-21-host-desktops-sweep-host-service.md). It holds the measurements
and the seam checks; the changeset holds the plan.

> **Split on 2026-10-21 (developer decision 4).** Sections 1–4 are the whole sweep's measurements, copied
> from the sibling PR #692 (`feature/host-desktops/sweep`, `config.rs` only). This PR carries
> `host_tooling.rs`, `service.rs`, the start functions and the stand-in bridges.

## 1. Where the stack stands

All eight nodes are **merged into `master`** (squash merges), not merely open on a line:

| Node | PR | Base it merged into | State |
|---|---|---|---|
| 1 service-split | #653 | `master` | MERGED |
| 2 rdp-nla | #654 | `feature/host-desktops/service-split` | MERGED |
| 3 vnc-security | #655 | `master` | MERGED |
| 4 hosts-mount | #656 | `master` | MERGED |
| 5 viewer-fullscreen | #657 | `feature/host-desktops/hosts-mount` | MERGED |
| 6 bridge-ready | #658 | `master` | MERGED |
| 7 x11-channels | #659 | `feature/host-desktops/bridge-ready` | MERGED |
| 8 sign-in-form | #660 | `feature/host-desktops/x11-channels` | MERGED (2026-10-21 03:37 UTC) |

Consequences for this node:

- **No other node of this stack can conflict with a split.** The `/pr-wrap` stack rule ("never split
  a file another PR of this stack touches") is satisfied trivially. `git diff --name-only
  origin/master...origin/feature/host-desktops/<node>` was run for all eight branches; because every
  node was squash-merged, the three-dot diff still lists each node's own historical files (the
  branches are not ancestors of `master`), so it is evidence of what each node *wrote*, not of an open
  conflict: config.rs (nodes 7, 8 tips), host_tooling.rs (7, 8), service.rs (4, 5), the
  screen-sharing service and `tests/common` (1, 2, 6, 7, 8), `bridge.rs` and `rdp_client.rs` (2, 6, 7, 8).
- **This node is a plain PR on `master`.** The branch is cut from `origin/master` (same tree as the
  `sign-in-form` tip) and its PR's base is `master`: a PR against the merged `sign-in-form` branch would
  close if that branch were deleted.
- **Open-PR overlap (the check that replaces the sibling one).** `gh pr list --state open --limit 100
  --json number,headRefName,isDraft,files` (47 open PRs, 2026-10-21; `files` is capped at 100 per PR by
  the API) filtered for `config.rs`, `host_tooling.rs`, `service.rs`, `screen_sharing_service*`,
  `tests/common/mod.rs`, `tddy-screenshare/src/bridge.rs`, `tddy-rdp/src/rdp_client.rs` and the three
  records: **one hit**. Open draft **#664** (`#provider-route` 3/9, `feature/provider-route/opencode-cli`)
  edits `packages/tddy-daemon-kernel/src/config.rs`: a `DaemonConfig.cli_harnesses` field (`:470`), its
  `Default` line (`:573`), and a new `CliHarnessConfig` struct plus an `impl DaemonConfig { fn
  cli_harness_binary }` inserted directly after `ClaudeCliConfig` (`:809`) — inside the region the
  `agent_cli` seam moves. Whichever of the two lands second has a conflict to resolve: #664's struct
  then belongs in `config/agent_cli.rs`, and its `impl DaemonConfig` block beside it. No other open PR
  touches a planned file.

## 2. File-length measurements (production lines, counted to the first `#[cfg(...test...)]`)

Script: every `src/**/*.rs` of the eight Rust packages the stack touched (`tddy-screen-sharing`,
`tddy-screenshare`, `tddy-vnc`, `tddy-rdp`, `tddy-host-service`, `tddy-daemon-kernel`,
`tddy-daemon-extensions`, `tddy-service`), at three refs. Files over 450 or grown by more than 40:

| File | Stack start `ffe71a895` | Merge base `569c65456` | Now | Who grew it | Record |
|---|---|---|---|---|---|
| `tddy-daemon-kernel/src/config.rs` | 1,557 | 1,557 | **1,575** | #659 (+18) | `oversized-file-config.md` |
| `tddy-host-service/src/service.rs` | 936 | 972 | **972** | #656 (+36) | `oversized-file-service.md` — said 961, corrected in this commit |
| `tddy-host-service/src/host_tooling.rs` | 775 | 775 | **806** | #659 (+31) | `oversized-file-host-tooling.md` |
| `tddy-host-service/src/host_registry.rs` | 596 | 596 | 596 | — | `oversized-file-host-registry.md` (not this stack's) |
| `tddy-host-service/src/ssh_agent.rs` | 500 | 500 | 500 | — | at the line, not over |
| `tddy-screen-sharing/src/screen_sharing_service.rs` | 983 | 421 | 482 | #653 (−562), #658–#660 (+61) | none (closed by #653); **1,378 test lines** follow (`#[cfg(all(test, unix))]` at `:483`) |
| `tddy-rdp/src/rdp_client.rs` | 347 | 357 | 446 | #654, #658, #660 | none |
| `tddy-screen-sharing/src/host_desktop_sign_in.rs` | — | — | 409 | #660 (new) | none |
| `tddy-screen-sharing/src/screen_sharing_service/bridge_lifecycle.rs` | — | 243 | 406 | #653 (new), #658–#660 | none |
| `tddy-screenshare/src/bridge.rs` | 294 | 301 | 399 | #654, #658, #660 | none |

`host_add_key_handler_tests.rs` (1,262) is a test-only module file under `src/`; not a production file.

**Attribution** (`git log --numstat ffe71a895..HEAD`): service.rs is touched only by #656 (48+/12−);
host_tooling.rs only by #659 (167+/3−, most of it test lines); config.rs only by #659 (18+/0−).

**No other record is worse.** `complexity-host-messages-unlock-and-add.md` (`unlock_and_add`, 80
lines, unchanged although #659 touched `host_messages.rs`), `oversized-file-host-registry.md` (596,
untouched), the two `privilege_drop.rs` records (untouched), `tddy-service`'s
`complexity-service-acp-on-event.md` and `oversized-file-build.md` (`build.rs` 743 → 743; the stack
changed only `.proto` files in `tddy-service`).

## 3. Function-length measurements (lines from `fn` to its closing brace)

Brace-matching script over the same packages (strings and `//` comments masked); production regions
only. Thresholds from `/analyze-clean-code`: 41–60 needs attention, over 60 must refactor.

| Function | Start | Merge base | Now | Verdict |
|---|---|---|---|---|
| `tddy-rdp` `RdpClient::connect` (`<RdpClient as ScreenSharingClient>`, `#[async_trait]`) | 107 | 112 | **115** | worse (+8 over the stack) — todo `2026-10-20-rdp-connect-config-builder-extraction.md` |
| `tddy-screen-sharing` `start_host_stream_impl` (`host_targets.rs`, inherent) | 67 (as `start_host_stream`) | 67 | **76** | worse (+9, #659/#660) |
| `tddy-screen-sharing` `start_stream` (`screen_sharing_service.rs`, `#[async_trait]`) | 63 | 63 | **72** | worse (+9, #658) |
| `tddy-screenshare` `start_bridge` (free, generic) | — | — | **58** | new (#658 split it out of `run_bridge`, 77 → 68 at the merge base → now under 41) |
| `tddy-screen-sharing` `build_bridge_spawn_config` | 45 | 45 | 48 | worse by 3, attention band |
| `tddy-vnc` `connect_with_credentials` | — | — | 43 | new (#655/#660), attention band |

Every other function over 60 in these packages is unchanged since the stack started
(`stream_host_stats` 96, `add_host_key` 86, `host_tooling` `probe` 77, `pump_loop` 74, …).

**Not measured: CRAP.** `tddy-tools analyze report` needs `analyze coverage` first, which rebuilds each
crate under llvm-cov instrumentation and runs its whole suite instrumented. On this machine (cold
`target/`, loaded, other worktrees building) that is a second full build per crate, so it was not
produced. Length and the record history are what this targeting rests on; nesting depth and
parameter counts were not computed either.

## 4. Backlog delta of the stack

`git diff --name-status 569c65456..HEAD -- 'packages/*/docs/code-issues/' docs/dev/todo/` adds nine
todo files and no code-issue file. Widening to the stack's start (`ffe71a895..HEAD`) adds four more
written by earlier nodes of this stack:

| Entry | Written by | Kind |
|---|---|---|
| `2026-10-18-screen-share-mid-session-resize.md` | #658 | feature |
| `2026-10-20-endpoint-channel-targets-matched-by-loopback-port.md` | #659 | feature |
| `2026-10-20-screen-sharing-start-functions-over-length.md` | #658 | **code quality — in scope (B)** |
| `2026-10-20-screen-sharing-two-stand-in-bridges.md` | #658 | **test debt — in scope (B)** |
| `2026-10-20-session-start-without-a-bridge-succeeds.md` | #658 | feature (behaviour) |
| `2026-10-20-x11-display-served-on-its-vnc-port-is-not-offered.md` | #659 | feature |
| `2026-10-20-x11-socket-dir-read-failure-lists-no-channels.md` | #659 | feature (behaviour) |
| `2026-10-21-desktop-sign-in-pins-the-advertised-fingerprint.md` | #660 | feature |
| `2026-10-21-host-prompt-offered-auth-methods-is-never-sent.md` | #660 | feature |
| `2026-10-20-hosts-mount-service-rs-grew-while-over-budget.md` | #656 | **deferred split of service.rs — closes with A** |
| `2026-10-20-rdp-connect-config-builder-extraction.md` | #654 | **code quality — measurably worse (C)** |
| `2026-10-20-rdp-test-support-module.md` | #654 | test duplication — not an engine move |
| `2026-10-20-rdp-empty-tls-public-key-fallback.md` | #654 | behaviour change (removing a fallback) |

## 5. Seam reading per file

### `tddy-host-service/src/host_tooling.rs` (806)

| Lines | Contents | Seam |
|---|---|---|
| 1–381 | types, the probe trait, argv builders, parsers, `SubprocessHostToolingProbe`, its two `probe` impls (`unix` / `not(unix)`) | stays |
| 382–482 | `start_desktop_probes`, `desktop_readings_of`, `desktop_probe_failed`, `warn_if_a_desktop_probe_failed` | `desktop_probes` |
| 483–625 | `#[cfg(unix)]` readings: `start_agent_probe`, `agent_status_of`, `not_on_path`, `warn_if_failed`, `git_identity_of`, `git_config_value`, `github_cli_of` | `readings` |
| 626–806 | `#[cfg(unix)]` subprocess plumbing: `ProbeOutput`, `Written`, `StartedProbe`, `RunningProbe`, `start_probe`, `start_running`, `read_both_pipes`, `read_to_end` and the two impls | `subprocess` |
| 807–1274 | `mod tests` | stays |

Crate-internal caller: `github_cli.rs:19` imports `start_probe`, `summarise`, `ProbeOutput`,
`PROBE_TIMEOUT` from `crate::host_tooling` — re-pointed by `reexport: "outside"` (only `summarise`
and `PROBE_TIMEOUT` stay where they are).

### `tddy-host-service/src/service.rs` (972)

| Lines | Contents |
|---|---|
| 1–106 | `use`s, consts, `struct HostServiceImpl` |
| 107–288 | `impl HostServiceImpl`: `new`, the `with_*` builders, accessors, `cached_tooling`, `pending_prompt_pump_count` |
| 289–410 | same `impl`: peer routing (`record_rpc_activity`, `eligible_instance_ids`, `classify_addressed_daemon_route`, `common_room_slot`, `rpc_served_by_peer`, `stream_served_by_peer`) — the record's seam A |
| 412–972 | `#[async_trait] impl HostService for HostServiceImpl` — **561 lines**, nine RPCs; seams B, C, D of the record are bodies in here |

The `#[async_trait]` impl is the whole difficulty: it is 561 lines on its own, larger than the budget,
and a trait impl cannot be split across modules (`E0119`/`E0046`). The record's seams B–D need its
method bodies moved out, which is `extract_method` inside an `#[async_trait]` impl — refused by the
engine on 2026-10-19 (`docs/dev/todo/2026-10-19-restructure-extract-method-refused-under-async-trait.md`).
Re-probed below against today's engine.

### Start functions

- `start_host_stream_impl` (inherent `async fn` in `host_targets.rs`): two cuts — the target lookup
  (`let target = scope.targets.list(..)…?;`, 12 lines) and the bridge start (`bridge_key` →
  `start_signed_in_bridge(..).await?`, 15 lines). Inherent async methods extract (probe P2 of the
  2026-10-19 entry).
- `start_bridge` (free generic `async fn` in `bridge.rs`): the LiveKit join and track publish (the
  input channel → `tokio::spawn(participant.run())`, 29 lines) names no `C`, so it extracts as a
  non-generic helper.
- `start_stream` and `RdpClient::connect` sit in `#[async_trait]` impls: same question as service.rs.

### The two stand-in bridges

`FakeBridge` lives in `#[cfg(all(test, unix))] mod tests` of `screen_sharing_service.rs` (1,378 test
lines, `use super::*;`, reaching the service's private items); `StandInBridge` in
`tests/common/mod.rs` (1,241 lines, shared by three integration binaries). An integration-test crate
cannot see the library's `#[cfg(test)]` code, and none of the engine's operations moves an inline test
module into `tests/` (`move_test_binary_to_crate` moves `tests/*.rs` binaries between crates and
refuses `tests/common/mod.rs` as an anchor; `move_item` refuses a module; `reparent_module` refuses an
inline one). Merging the two also means deleting one copy and giving the other the argv/stdin
recording — an edit, not a move.

## 6. Engine results

See the changeset's `## Restructuring` for the per-plan table; the commands and their output
summaries are recorded here as they ran.

Environment: one `tddy-index-daemon` for this checkout (`./run-index-daemon`, warm), every command
with `TDDY_INDEX_SOCKET` set. The machine ran at a load average of 240–280 throughout; the first
`anchors` call waited 2m40s for the index, later ones a few seconds.

**Anchors.** Every anchor in the plans was emitted by `restructure anchors <file> --items …` or
`--at L:C-L:C` and pasted unchanged; headers by `restructure snapshot <plan>` (v2 for item anchors, v1
for the two range-anchored plans). Two anchor refusals, both worked around with a range anchor:

- `--items HostServiceImpl::host_keypair,…` in `service.rs`: the same refusal, because `host_keypair` is both a field and a method of `HostServiceImpl`. Same gap, different cause.

**`check --deep --budget 500`**, run twice: on `fa9b42215`, and again on `ea5ba6fb1` after the
fast-forward (the index daemon told its server of 136 changed files first). Same answers both times:

| Plan | Result |
|---|---|
| `10`–`12` host_tooling `move_item`s | **no findings**; widenings `desktop_readings_of`, `warn_if_a_desktop_probe_failed`, `git_config_value`, `github_cli_of`, fields `StartedProbe::running`, `RunningProbe::child` → `pub(super)` |
| `13-service-peer-forwards` (`extract_module`, items = six members) | **no findings**; `common_room_slot`, `rpc_served_by_peer`, `stream_served_by_peer` → `pub(crate)` |
| `14-service-builders` (`extract_module`, range `108:5–287:6`) | **no findings**; `cached_tooling` → `pub(crate)` |
| `16-host-targets-start-host-stream` (two `extract_method`s, bottom-up) | **no findings** |
| `17-bridge-start-bridge` (`extract_method`) | **no findings** |
| `probe-a-start-stream` (`extract_method` over `screen_sharing_service.rs:416–449`, `#[async_trait]`) | refused — `this seam cannot be cut here: rust-analyzer offers no "extract into function" assist for the given range (it offered: Extract into variable)` |
| `probe-b-rdp-connector-config` (`extract_method` over the `Config { … }` literal, `rdp_client.rs:155–198`, `#[async_trait]`) | refused — same text |
| `probe-c-service-add-host-key` (`extract_method` over two `let` statements, `service.rs:683–692`, `#[async_trait]`) | refused — same text |

`--budget 500` only reports the file's current size (`host_tooling.rs … 306 over`, `service.rs … 472 over`); it does not project the result.

**Not run:** `restructure apply` (not even as a trial — the tree was never modified by the engine) and
`restructure verify --against HEAD` (it accounts for an applied run's statements; there is none).
`check --deep` is not a compile proof: the first apply's compile gate is.

**Projected sizes** were computed by script from whole-item spans (leading doc comments and attributes
included, closing brace found by brace matching): `host_tooling.rs` 422 out → ~384 + facades; `service.rs` 301 out → ~673 (still over before the consented hand moves; see the changeset's `## Hand moves (consented)`).


## 7. Independence from the sibling PR #692 (`config.rs`)

Recorded 2026-10-21 on `ea5ba6fb1`, for developer decision 4 ("no edges between them"):

1. **Disjoint files.** #692 writes only `packages/tddy-daemon-kernel/src/config.rs`, new files under
   `packages/tddy-daemon-kernel/src/config/`, and its own `docs/dev/1-WIP/2026-10-21-host-desktops-sweep*`
   documents. This PR writes `tddy-host-service`, `tddy-screen-sharing`, `tddy-screenshare` and `tddy-rdp`
   sources, their `docs/`, three `docs/dev/todo/` entries and its own `…-sweep-host-service*` documents.
   No path is in both.
2. **No plan names a `config::` item.** `grep -l 'config::' <this PR's plans>/*.jsonl` is empty; every
   `to` is `tddy_host_service::host_tooling` (plans 10–12) or a new child of the edited file.
3. **Every `config::` path the moved code uses survives #692.** `grep -nE 'config::[A-Za-z_{]'` over the
   six files this PR edits finds `DaemonConfig`, `LiveKitConfig`, `ScreenSharingConfig` and
   `resolve_x11vnc_binary_path` (`service.rs:14`, `host_tooling.rs:212–251` and its tests at `:1055–1057`,
   `screen_sharing_service.rs:25,499`, `host_targets.rs:14`). `DaemonConfig` and `LiveKitConfig` stay in
   `config.rs` under #692. `ScreenSharingConfig` and `resolve_x11vnc_binary_path` move to
   `config/screen_sharing.rs` with `reexport: "outside"`, and these very files are the outside callers
   that make the engine leave them in `config.rs`'s named facade, so `tddy_daemon_kernel::config::…` keeps
   resolving. The `move_item`s here copy those `use` lines verbatim into the new modules; the paths are
   the same either way.
4. **Either order lands.** Neither PR's anchors are in the other's files, and the facade is computed from
   callers that exist on `master` today, so it does not depend on which PR merges first.
