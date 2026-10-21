# Initial discovery: `#host-desktops` sweep, `config.rs` (9/10)

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
[2026-10-21-host-desktops-sweep.md](./2026-10-21-host-desktops-sweep.md) (PR #692). It holds the measurements
and the seam checks; the changeset holds the plan.

> **Split on 2026-10-21 (developer decision 4).** Sections 1–4 are the whole sweep's measurements and are
> shared with the sibling PR. This PR carries `config.rs` only; `host_tooling.rs`, `service.rs`, the start
> functions and the stand-in bridges are in the sibling's discovery record
> (`docs/dev/1-WIP/2026-10-21-host-desktops-sweep-host-service-initial-discovery.md`, branch
> `feature/host-desktops/sweep-host-service`).

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

## 5. Seam reading

### `tddy-daemon-kernel/src/config.rs` (1,575)

Top-level layout (line numbers at `fa9b42215`):

| Lines | Contents |
|---|---|
| 1–48 | `use`s, `pub use LiveUsers`, the `DaemonConfig` serde default fns, `blank_as_none` and the LiveKit default (`:23`, `:28`) |
| 49–173 | `AgentWarmupConfig` with its three defaults and `Default`; `SessionRoomConfig` with its defaults, consts, `impl` and `Default` |
| 174–177 | `default_codex_oauth_loopback_proxy_eligible` — a `DaemonConfig` serde default, **stays** |
| 178–370 | `RelayConfig`, `LocalConfig`, `KeyringConfig`, `GitConfig`, `SandboxCgroupConfig`, `IndexDaemonConfig` with their defaults and impls |
| 371–604 | `DaemonConfig`, `SupervisorClientConfig`, `default_max_attachment_bytes`, `impl Default`, first `impl DaemonConfig` |
| 605–615 | `TelegramConfig` |
| 616–688 | `ScreenSharingConfig`, `resolve_vnc_binary_path`, `resolve_rdp_binary_path`, `resolve_x11vnc_binary_path` |
| 689–1006 | Claude/Cursor CLI: the two binary-path defaults, `ClaudeCliConfig`, `CursorCliConfig`, every `resolve_claude_*` / `resolve_cursor_*`, the shim and `PATH` search helpers, the `*_ENV` consts |
| 1008–1078 | sandbox config path: `SANDBOX_CONFIG_ENV`, `SANDBOX_CONFIG_BASENAME`, `sandbox_config_os_token`, `resolve_sandbox_config_path{,_with}` |
| 1079–1168 | `ListenConfig`, `LiveKitConfig` + `Default` + `impl` |
| 1169–1208 | `GitHubConfig`, `UserMapping`, `AllowedTool`, `AllowedAgent` |
| 1209–1434 | second `impl DaemonConfig` — `load`, the accessors, the four `apply_*_env_override(s)`, `validate_for_relay` |
| 1435–1575 | the env merge helpers: `merge_timing_env`, `env_session_room_ms`, `env_u64`, `parse_chat_ids_csv`, `parse_env_bool`, `merge_telegram_env` |
| 1576–2710 | eleven `#[cfg(test)] mod …_tests { use super::*; … }` |

**The serde string constraint.** `#[serde(default = "…")]` names a function by string, and the engine
cannot see that reference (`plan-schema.md`, the attribute-string rule). So each struct moves *with*
its own default fns, and the defaults `DaemonConfig` names (`:11–47`, `:174`, `:537`) stay. Mapped by
script: `AgentWarmupConfig` ↔ `:49–57`, `SessionRoomConfig` ↔ `:93–109`, `RelayConfig` ↔ `:178`,
`IndexDaemonConfig` ↔ `:323–329`, `ClaudeCliConfig` / `CursorCliConfig` ↔ `:689–693`,
`LiveKitConfig` ↔ `blank_as_none` and `default_common_room_set_metadata_timeout_secs` (`:23–37`, not
contiguous with it — which is why `ListenConfig`/`LiveKitConfig` are **not** in the plan).

**Callers outside the crate** (paths through `tddy_daemon_kernel::config::…`): the resolvers are
used from `tddy-agent-launch` (7 files), `tddy-daemon-rpc`, `tddy-host-service` (2),
`tddy-screen-sharing` (3), `tddy-session-lifecycle`, `tddy-session-split`, `tddy-telegram-control`
and the crate's own `tests/x11vnc_binary_resolution.rs`; the moved structs from `tddy-daemon`,
`tddy-daemon-auth`, `tddy-daemon-livekit`, `tddy-host-service`, `tddy-code-restructuring`.
`reexport: "outside"` keeps every one of those paths through a named facade in `config.rs`, so no
file outside `tddy-daemon-kernel` is edited.

**Tests reach moved private helpers.** All eleven test modules `use super::*;` and several call private
helpers directly (`choose_claude_binary`, `find_real_agent_in`, `merge_timing_env`, …). An inline
`mod` cannot be carried by `move_item` (a module among the moved items is `reparent_module`'s, and
`reparent_module` refuses an inline module), so the tests stay and the helpers they call widen to the
narrowest visibility that reaches them. The engine reports each widening; the changeset's
`## Visibility` table is filled from the apply.

## 6. Engine results

See the changeset's `## Restructuring` for the per-plan table; the commands and their output
summaries are recorded here as they ran.

Environment: one `tddy-index-daemon` for this checkout (`./run-index-daemon`, warm), every command
with `TDDY_INDEX_SOCKET` set. The machine ran at a load average of 240–280 throughout; the first
`anchors` call waited 2m40s for the index, later ones a few seconds.

**Anchors.** Every anchor in the plans was emitted by `restructure anchors <file> --items …` or
`--at L:C-L:C` and pasted unchanged; headers by `restructure snapshot <plan>` (v2 for item anchors, v1
for the two range-anchored plans). Two anchor refusals, both worked around with a range anchor:

- `--items …,is_executable_file,…` in `config.rs`: `plan is malformed: tddy_daemon_kernel::config::is_executable_file names 2 items … qualify it with the trait` — the two items are the `#[cfg(unix)]` and `#[cfg(not(unix))]` twins of one free function, not trait members. An engine gap (an `items` anchor cannot name a cfg twin pair); a candidate backlog entry for the wrap.

**`check --deep --budget 500`**, run twice: on `fa9b42215`, and again on `ea5ba6fb1` after the
fast-forward (the index daemon told its server of 136 changed files first). Same answers both times:

| Plan | Result |
|---|---|
| `01-config-agent-cli` (first draft: `move_item` over the items `:728–1005`) | refused — `this seam cannot be cut here: the seam would separate an attribute from the item its string names: default_claude_cli_binary_path, named by the attribute on line 811 and declared on line 689; default_cursor_cli_binary_path, named by the attribute on line 835 …`. Correct: the serde defaults must travel with their structs, and the cfg twins (above) sit between them, so no single `items` run covers both |
| `01-config-agent-cli` (as planned: `extract_module` over the range `689:1–1005:2`, `to_file`, `reexport: named`) | **no findings**; widens `is_wrapper_shim_dir`, `ClaudeBinaryChoice`, `choose_claude_binary`, `find_real_agent_in`, `CursorBinaryChoice`, `choose_cursor_binary` to `pub(crate)` |
| `02-…` (a second `move_item` into `agent_cli`) | dropped: `check` refuses a `to` that does not exist yet, and the range seam made it unnecessary |
| `03`–`09` config `move_item`s | **no findings** each; widenings `resolve_sandbox_config_path_with`, `parse_env_bool`, `merge_telegram_env`, `MIN_WORKTREE_ROOM_INTERVAL_MS`, `SessionRoomConfig::validate` → `pub(super)` |

`--budget 500` only reports the file's current size (`config.rs is 1575 production lines, 1075 over`); it does not project the result.

**Not run:** `restructure apply` (not even as a trial — the tree was never modified by the engine) and
`restructure verify --against HEAD` (it accounts for an applied run's statements; there is none).
`check --deep` is not a compile proof: the first apply's compile gate is.

**Projected sizes** were computed by script from whole-item spans (leading doc comments and attributes
included, closing brace found by brace matching): `config.rs` 1,175 lines out → ~400 + facades.


## 7. Independence from the sibling PR (`feature/host-desktops/sweep-host-service`)

The two PRs write disjoint files: this one only `tddy-daemon-kernel/src/config.rs` and new files under
`src/config/`, plus its own documents. The sibling's moved code uses four `tddy_daemon_kernel::config::…`
items (`DaemonConfig`, `LiveKitConfig`, which stay here, and `ScreenSharingConfig`,
`resolve_x11vnc_binary_path`, which plan 03 moves with `reexport: "outside"`, the sibling's files being the
outside callers that keep them in the named facade), so its paths resolve before and after this PR. The
full proof is in the sibling's discovery record, § 7.
