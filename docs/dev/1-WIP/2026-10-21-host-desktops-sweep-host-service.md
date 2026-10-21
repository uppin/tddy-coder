# Changeset: Split `host_tooling.rs` and `service.rs` under the 500-line budget, and shorten the desktop start paths

**Date**: 2026-10-21
**Status**: 🚧 In Progress — applied (plans 10–14, 16, 17, 20–25, 27, plan 26 reduced; hand moves H1–H11); validated (`/validate-changes`, 2026-10-21); wrap pending
**Type**: Refactor
**Complexity**: 5/5 — four crates (`tddy-host-service`, `tddy-screen-sharing`, `tddy-screenshare`, `tddy-rdp`); eleven consented hand moves of `#[async_trait]` trait-method bodies (byte-identity proven by script) followed by engine plans that can only be anchored after them; seven engine plans proven now; two oversized files and four over-length functions; a crate-internal caller re-pointed
**Stack**: `#host-desktops` 10/10 — second of the two sweep PRs, a plain PR. Base: `master` `ea5ba6fb1`. Sibling, independent: #692 (`feature/host-desktops/sweep`, `config.rs`, 9/10)

## Initial Discovery

[2026-10-21-host-desktops-sweep-host-service-initial-discovery.md](./2026-10-21-host-desktops-sweep-host-service-initial-discovery.md):
the sweep's measurements, the seam reading per file, the engine's answers (refusals verbatim) and the proof
that this PR and #692 are independent (§ 7).

## Developer decisions

Recorded from the developer's review of the plan (`6e`), relayed by the orchestrator on 2026-10-21:

- **2026-10-21 — hand moves consented:** "HAND MOVES CONSENTED for service.rs: the #653 seam-C precedent.
  The nine `#[async_trait]` trait bodies move by hand into inherent `*_impl` methods (byte-identical bodies,
  delegators left behind in the trait impl), THEN engine `extract_module` plans cut the inherent methods
  out … The same consent covers `start_stream`, `RdpClient::connect` and `add_host_key` … Everything else
  stays engine-driven." Specified in `## Hand moves (consented)`.
- **2026-10-21 — stand-in bridges:** "LEAVE, record the reason." The entry stays open; this PR only appends
  the reason and the decision to it (done in this commit). No hand edit of that test code.
- **2026-10-21 — #664 conflict:** "the sweep lands FIRST." Concerns #692 only; this PR does not touch `config.rs`.
- **2026-10-21 — PR shape:** "TWO PRs, independent siblings both based on master, no edges between them."
  This is **PR B**: `host_tooling.rs`, `service.rs`, the start functions, the dead-link correction.

## Prerequisites

**✅ RESOLVED HERE** marks what this PR's wrap deletes; nothing is deleted at planning time.

| Item | Verdict | Note |
|---|---|---|
| [`oversized-file-host-tooling.md`](../../../packages/tddy-host-service/docs/code-issues/oversized-file-host-tooling.md) — 806 | **✅ RESOLVED HERE** — projected ~395 | plans 10–12. Unclaimed (checked 2026-10-21) |
| [`oversized-file-service.md`](../../../packages/tddy-host-service/docs/code-issues/oversized-file-service.md) — 972 (its 961 corrected in this commit) | **✅ RESOLVED HERE** — projected ~245 | plans 14, 13, hand moves H1–H9, plans 20–23. Unclaimed |
| [`2026-10-20-hosts-mount-service-rs-grew-while-over-budget.md`](../todo/2026-10-20-hosts-mount-service-rs-grew-while-over-budget.md) | **✅ RESOLVED HERE** | #656's deferral of the same split; corrected to 972 in this commit |
| [`2026-10-20-screen-sharing-start-functions-over-length.md`](../todo/2026-10-20-screen-sharing-start-functions-over-length.md) | **✅ RESOLVED HERE** | `start_host_stream_impl` 76 → ~53 (plan 16), `start_bridge` 58 → ~33 (plan 17), `start_stream` 72 → ~43 (H10 + plans 24–25). Its "do it together with the no-bridge entry" is not followed: that entry changes behaviour (out of scope) |
| [`2026-10-20-rdp-connect-config-builder-extraction.md`](../todo/2026-10-20-rdp-connect-config-builder-extraction.md) | **⚠️ NARROWED HERE** (was ✅ RESOLVED HERE) | `RdpClient::connect` 115 → `connect_impl` **72** (H11 + plan 26 reduced): `connector_config` extracted; the finalize step was refused by the engine (E0121, `server_cert` has a type the assist cannot spell), so the entry is narrowed to that remainder, not deleted |
| `packages/tddy-screenshare/docs/changesets/2026-10-20-bridge-ready.md` dead link (`../bridge-readiness.md`) | **✅ RESOLVED HERE** at wrap | a new correction entry (Final Checklist); the wrapped entry is not edited |
| [`2026-10-20-screen-sharing-two-stand-in-bridges.md`](../todo/2026-10-20-screen-sharing-two-stand-in-bridges.md) | **Leave it** (decision 2) | stays open; the engine-refusal reason and the decision are appended to it in this commit |
| [`2026-10-19-restructure-extract-method-refused-under-async-trait.md`](../todo/2026-10-19-restructure-extract-method-refused-under-async-trait.md) | ⚠ DURING — engine gap, reconfirmed | three refusals today (probes a–c); the wrap adds them as evidence and records that the gap was worked around by consented hand moves again |
| [`2026-10-20-rdp-test-support-module.md`](../todo/2026-10-20-rdp-test-support-module.md) | Leave it | test-helper deduplication across two integration binaries — an edit, not a move |
| The feature entries (`2026-10-18-screen-share-mid-session-resize`, `…-endpoint-channel-targets-matched-by-loopback-port`, `…-session-start-without-a-bridge-succeeds`, `…-x11-display-served-on-its-vnc-port-is-not-offered`, `…-x11-socket-dir-read-failure-lists-no-channels`, `2026-10-21-desktop-sign-in-pins-the-advertised-fingerprint`, `…-host-prompt-offered-auth-methods-is-never-sent`, `2026-10-20-rdp-empty-tls-public-key-fallback`) | Out of scope | behaviour changes; they go to the developer at the TODO gate |

## Responsibility

- `packages/tddy-host-service/src/host_tooling.rs` → `host_tooling.rs` + three modules under `host_tooling/`.
- `packages/tddy-host-service/src/service.rs` → `service.rs` (struct, consts, the trait impl as nine
  one-line delegators) + six modules under `service/`.
- The length of `start_host_stream_impl`, `start_stream` (`tddy-screen-sharing`), `start_bridge`
  (`tddy-screenshare`), `RdpClient::connect` (`tddy-rdp`) and `add_host_key`.
- At wrap: the corrected dead link in `tddy-screenshare`'s change history (a new entry).

## Boundaries

- **No behaviour change**, no new symbol beyond module paths, the `*_impl` methods and the extracted helpers;
  no test semantics changed, no new dependency, no new Cargo feature.
- **Hand-written code is limited to the eleven consented hand moves** (H1–H11): bodies copied byte for byte,
  signatures and one-line delegators written by hand. Every other move is the engine's; no `git mv`.
- Does not touch `tddy-daemon-kernel` (#692's), the stand-in bridges (decision 2), the feature entries,
  the protocols, the proto or the web.
- `stream_host_stats` (96 lines) and `host_tooling`'s `probe` (77) stay over 60: unchanged by the stack and
  outside this backlog.

## Dependencies

Every node of `#host-desktops` is **MERGED into `master`**; this PR consumes nothing unmerged.

| Node | PR — squash on `master` | What it left in this PR's path | This PR does NOT |
|---|---|---|---|
| n1 service-split | #653 — `9d43b58d8` | `screen_sharing_service/{bridge_lifecycle,host_targets}.rs`; the seam-C hand-move precedent | re-split the service module |
| n2 rdp-nla | #654 — `7d9a7d74e` | CredSSP + domain in `RdpClient::connect` | change the connector config's values |
| n3 vnc-security | #655 — `36fd1e9e9` | nothing in this path | — |
| n4 hosts-mount | #656 — `300c3da4c` | `ToolingCache` wiring in `service.rs` (+36) | change the cache |
| n5 viewer-fullscreen | #657 — `17e240baa` | nothing in this path | — |
| n6 bridge-ready | #658 — `34c046a99` | `start_bridge` split; the readiness wait in `start_stream` | change the readiness protocol |
| n7 x11-channels | #659 — `1be99ce05` | X11 channels in `host_tooling.rs` (+31), X11 in `start_host_stream_impl` | change their behaviour |
| n8 sign-in-form | #660 — `fa9b42215` | desktop sign-in in `host_targets.rs`, `rdp_client.rs` | change sign-in |
| sibling 9/10 | #692 (**merged** before this wrap; `master` `e0256b346` carries it and changed none of this PR's four crates since its base) | nothing — **no edge** (discovery § 7) | consume `config/` module paths; every `tddy_daemon_kernel::config::…` path used here resolves with or without #692 |

## Draft PR contract

The draft is **the plan and the green baseline**: the seven engine plans proven today, the eleven hand
moves with their proof method, the engine plans that follow them (named here, anchored at implementation
time because their anchors do not exist before the hand moves), and the baseline the applied tree is held
to by failing-test name.

## Green wave

**Wave:** 3 of 3 — the tail of `#host-desktops`
**Greenable independently:** yes — every node below is merged and #692 has no edge to it; its oracle is the
existing suites of the four packages it edits.
**Concurrent with:** #692, fully parallel
**Blocked by:** nothing. **Blocks:** nothing.

## Affected Packages

- **tddy-host-service**: [README.md](../../../packages/tddy-host-service/README.md) — `host_tooling` and `service` become module directories
  - [host-service.md](../../../packages/tddy-host-service/docs/host-service.md) — file table (`:12`, `:159`)
  - [host-add-key.md](../../../packages/tddy-host-service/docs/host-add-key.md) — `:29`
- **tddy-screen-sharing**: [README.md](../../../packages/tddy-screen-sharing/README.md) — `start_stream` body in `screen_sharing_service/session_stream.rs`; two helpers in `host_targets.rs`
- **tddy-screenshare**: [README.md](../../../packages/tddy-screenshare/README.md) — one helper in `bridge.rs`; the change-history link correction
- **tddy-rdp**: [README.md](../../../packages/tddy-rdp/README.md) — `RdpClient::connect` delegates to `connect_impl` and two helpers

## Related Feature Documentation

None — behaviour-preserving restructure. No PRD.

## Summary

`host_tooling.rs` and `service.rs` are cut into topic modules. The `#[async_trait]` bodies the engine cannot
reach are first moved by hand, byte for byte, into inherent `*_impl` methods behind one-line delegators;
the engine then cuts those inherent methods into modules and extracts the helpers that bring the four
start paths under 60 lines. Every path a caller outside its library uses keeps resolving.

## Background

`host_tooling.rs` (+31 in #659) and `service.rs` (+36 in #656) grew while over budget, deferred with
consent until the stack landed; it has. The stack's own start paths crossed 60 lines. The engine refuses
`extract_method` inside an `#[async_trait]` impl (reconfirmed today), which is why the trait bodies need the
consented hand step.

## Scope

- [x] **Plan**: layout proposed, seam order fixed, engine plans snapshotted and proven with `check --deep`
- [x] **Consent** (`6e`, 2026-10-21): see `## Developer decisions`
- [x] **Apply, in the order of `## Delta`**: engine plans by dry run then apply + `verify --against HEAD`; hand moves by hand + the proof script + `cargo check`; one commit each ✅ (plan 26 reduced to `connector_config`: see `## Applied`)
- [x] **Baseline after**: the four packages back to the recorded names ✅ (see `## Baseline`, After column)
- [x] **Code Quality**: `cargo fmt --check`; `cargo clippy -p <four packages> --all-targets -- -D warnings` ✅
- [ ] **Documentation**: Final Checklist executed at wrap

## Hand moves (consented)

Each hand move copies a trait method's **body** — the lines strictly between the line that opens its body
(`) -> … {`) and its closing `}` — byte for byte into a new inherent method, and replaces the trait body with
one delegator line. The trait method keeps its signature, attributes and doc comments; the inherent method
gets a hand-written signature (same parameters, the stream types spelled out) and a one-line doc comment
`/// Body of [`<Trait>::<name>`]; the trait method delegates here.`. Bodies are at the same indentation in
both places (8 spaces), so no line is re-indented.

| # | File | Trait method | Inherent method (new) | Where it lands | Delegator | Body lines |
|---|---|---|---|---|---|---|
| H1 | `tddy-host-service/src/service.rs` | `HostService::list_eligible_daemons` | `HostServiceImpl::list_eligible_daemons_impl` | a new `impl HostServiceImpl { … }` block directly after the trait impl | `self.list_eligible_daemons_impl(request).await` | 21 |
| H2 | same | `list_known_hosts` | `list_known_hosts_impl` | same block | `self.list_known_hosts_impl(request).await` | 31 |
| H3 | same | `get_host_tooling` | `get_host_tooling_impl` | same block | `self.get_host_tooling_impl(request).await` | 39 |
| H4 | same | `stream_host_prompts` | `stream_host_prompts_impl` → `Result<Response<MpscHostPromptStream>, Status>` | same block | `self.stream_host_prompts_impl(request).await` | 39 |
| H5 | same | `answer_host_prompt` | `answer_host_prompt_impl` | same block | `self.answer_host_prompt_impl(request).await` | 49 |
| H6 | same | `add_host_key` | `add_host_key_impl` | same block | `self.add_host_key_impl(request).await` | 81 |
| H7 | same | `list_host_key_candidates` | `list_host_key_candidates_impl` | same block | `self.list_host_key_candidates_impl(request).await` | 43 |
| H8 | same | `list_ssh_config_hosts` | `list_ssh_config_hosts_impl` | same block | `self.list_ssh_config_hosts_impl(request).await` | 49 |
| H9 | same | `stream_host_stats` | `stream_host_stats_impl` → `Result<Response<MpscHostStatsStream>, Status>` | same block | `self.stream_host_stats_impl(request).await` | 91 |
| H10 | `tddy-screen-sharing/src/screen_sharing_service.rs` | `ScreenSharingService::start_stream` | `ScreenSharingServiceImpl::start_stream_impl` | the existing `impl ScreenSharingServiceImpl` (`:133`), last member | `self.start_stream_impl(request).await` | 67 |
| H11 | `tddy-rdp/src/rdp_client.rs` | `ScreenSharingClient::connect` (associated fn) | `RdpClient::connect_impl(host: &str, port: u16, credentials: &tddy_screenshare::Credentials) -> Result<Self>` | the existing `impl RdpClient` (`:339`), first member | `Self::connect_impl(host, port, credentials).await` | 109 |

Every inherent method is `pub(crate) async fn`, takes `&self` (H1–H10) and the trait method's own
parameters by the same names. All nine `service.rs` methods have `request: Request<…Request>` and return
`Result<Response<…>, Status>`; H4 and H9 spell the stream types (`MpscHostPromptStream`,
`MpscHostStatsStream`) because an inherent impl has no `Self::StreamHostPromptsStream`. No body names
`Self::` (checked by script; H11's body builds `Self { … }`, which is `RdpClient` in the inherent impl too: the only `Self::` hits in the trait impl are the two stream return types and three
intra-doc links in doc comments, which stay on the trait methods).

**Expected production lines across the hand moves** (before the engine plans that follow them):

| File | Before (after plans 14, 13 for `service.rs`) | After the hand moves |
|---|---|---|
| `service.rs` | ~673 | ~1,170 transiently: the trait impl 561 → ~127 (443 body lines out, 9 delegator lines in), the new inherent block ~500 (443 + 9 × 5 signature/doc/brace lines + blanks). Plans 20–23 then take ~500 out |
| `screen_sharing_service.rs` | 482 | ~490 (−67 + 1 in the trait, +72 in the inherent block); plan 25 then takes ~80 out |
| `rdp_client.rs` | 446 | ~458 (−109 + 1, +115 + doc line) |

**Proof the implementer must run, per hand-move commit** (one commit per file: H1–H9 together in
`service.rs`, then H10, then H11, each before the engine plans that follow it):

1. The body comparison — byte for byte, against the commit before the hand move:

   ```bash
   # usage: prove-hand-move.sh <base-ref> <file> <trait-fn>=<impl-fn> ...
   python3 -I - "$@" <<'PY'
   import re, subprocess, sys
   base, path, pairs = sys.argv[1], sys.argv[2], sys.argv[3:]
   def body(text, name):
       lines = text.split("\n")
       start = next(i for i, l in enumerate(lines) if re.match(rf"\s*(pub(\([a-z]+\))?\s+)?async fn {name}\b", l))
       open_ = next(i for i in range(start, len(lines)) if lines[i].rstrip().endswith("{"))
       depth = 0
       for j in range(open_, len(lines)):
           depth += lines[j].count("{") - lines[j].count("}")
           if depth == 0:
               return lines[open_ + 1 : j]
       raise SystemExit(f"{name}: no closing brace")
   old = subprocess.run(["git", "show", f"{base}:{path}"], capture_output=True, text=True, check=True).stdout
   new = open(path).read()
   ok = True
   for pair in pairs:
       trait_fn, impl_fn = pair.split("=")
       before, after = body(old, trait_fn), body(new, impl_fn)
       delegator = [l.strip() for l in body(new, trait_fn)]
       same = before == after
       one_line = len(delegator) == 1 and impl_fn in delegator[0] and delegator[0].endswith(".await")
       print(f"{trait_fn} -> {impl_fn}: body {'identical' if same else 'DIFFERS'} ({len(before)} lines), delegator {'ok' if one_line else 'NOT ONE LINE'}")
       ok &= same and one_line
   sys.exit(0 if ok else 1)
   PY
   ```

   For example `prove-hand-move.sh HEAD~1 packages/tddy-host-service/src/service.rs
   list_eligible_daemons=list_eligible_daemons_impl … stream_host_stats=stream_host_stats_impl`. The brace
   count is plain text; on `ea5ba6fb1` the script's extraction gives exactly the body sizes in the table
   (21 … 109 lines) and no string literal in the eleven bodies holds an unbalanced brace. Every line must print `identical` and `ok`;
   the output is pasted into the commit message.
2. `git diff --color-moved=dimmed-zebra --color-moved-ws=no HEAD~1 -- <file>`: every removed body line shows
   as moved, and the only new lines are the signatures, doc lines, braces and delegators.
3. `./dev cargo check -p <package> --all-targets` after the commit: `tddy-host-service` after H1–H9,
   `tddy-screen-sharing` after H10, `tddy-rdp` after H11. One cargo invocation at a time.

## Restructuring

Production lines counted to the first `#[cfg(...test...)]`. Projected = whole-item spans (doc comments and
attributes included, measured by script) moved, plus estimated facade/`mod`/signature lines.

### State A

| File / function | Production lines |
|---|---|
| `tddy-host-service/src/host_tooling.rs` | 806 (1,274 total) |
| `tddy-host-service/src/service.rs` | 972 (no tests): struct 1–106, inherent impl 107–410 (304), `#[async_trait]` trait impl 412–972 (561) |
| `screen_sharing_service.rs` `start_stream` | 72-line function (file 482) |
| `host_targets.rs` `start_host_stream_impl` | 76-line function |
| `bridge.rs` `start_bridge` | 58-line function |
| `rdp_client.rs` `RdpClient::connect` | 115-line function (file 446) |
| `service.rs` `add_host_key` | 86-line function |

### State B — module layout

| Module | Holds | Projected production lines |
|---|---|---|
| `host_tooling.rs` | types, probe trait, argv builders, parsers, `SubprocessHostToolingProbe` and its `probe` impls, tests | **806 → ~395** |
| `host_tooling/subprocess.rs` | `ProbeOutput`, `StartedProbe`, `RunningProbe`, `start_probe` and the pipe plumbing (`#[cfg(unix)]`) | ~181 |
| `host_tooling/readings.rs` | `start_agent_probe`, `git_identity_of`, `github_cli_of`, `warn_if_failed`, … (`#[cfg(unix)]`) | ~147 |
| `host_tooling/desktop_probes.rs` | `start_desktop_probes`, `desktop_readings_of`, `desktop_probe_failed`, `warn_if_a_desktop_probe_failed` | ~94 |
| `service.rs` | struct, consts, the trait impl as nine one-line delegators, six `mod` lines | **972 → ~245** |
| `service/host_keys.rs` | `add_host_key_impl` + its three helpers, `list_host_key_candidates_impl`, `list_ssh_config_hosts_impl` | ~210 |
| `service/builders.rs` | `new`, the twelve `with_*`, accessors, `cached_tooling`, `pending_prompt_pump_count` | ~180 |
| `service/peer_forwards.rs` | `record_rpc_activity`, `eligible_instance_ids`, `classify_addressed_daemon_route`, `common_room_slot`, `rpc_served_by_peer`, `stream_served_by_peer` | ~121 |
| `service/listings.rs` | `list_eligible_daemons_impl`, `list_known_hosts_impl`, `get_host_tooling_impl` | ~106 |
| `service/prompts.rs` | `stream_host_prompts_impl`, `answer_host_prompt_impl` | ~98 |
| `service/stats_stream.rs` | `stream_host_stats_impl` | ~96 |
| `screen_sharing_service.rs` | unchanged but for the `start_stream` delegator and one `mod` line | **482 → ~418** |
| `screen_sharing_service/session_stream.rs` | `start_stream_impl` (72 → ~43), `start_session_bridge` | ~82 |
| `host_targets.rs` `start_host_stream_impl` | + `start_bridge_for_desktop`, `find_host_target` | **76 → ~53** |
| `bridge.rs` `start_bridge` | + `publish_to_livekit` | **58 → ~33** |
| `rdp_client.rs` | `connect` delegates to `RdpClient::connect_impl` (115 → ~49) + `connector_config`, `finalize_connection` | **446 → ~470** (≤ 500) — *applied: 459, `connect_impl` 72, `finalize_connection` refused (see `## Applied`)* |
| `add_host_key_impl` | + `os_user_for_session`, `await_passphrase_answer`, `add_key_on_blocking_pool` | **86 → ~48** |

### Delta — in apply order

Range-anchored (v1) plans of a file run before any other edit to it; plans marked *post-move* are anchored
with `restructure anchors` **after** the hand move they follow (their targets do not exist before it) and
proven with `check --deep` then, before they are applied.

| # | Step | What | Anchor | Proof today |
|---|---|---|---|---|
| 1 | plan `10-host-tooling-desktop-probes` | `move_item` + `name: desktop_probes`, `reexport: outside` | items (4) | ✅ `check --deep`: no findings |
| 2 | plan `11-host-tooling-readings` | `move_item` + `name: readings` | items (7) | ✅ no findings |
| 3 | plan `12-host-tooling-subprocess` | `move_item` + `name: subprocess` | items (10) | ✅ no findings |
| 4 | plan `14-service-builders` | `extract_module` `builders`, `to_file`, `reexport: none` | range `service.rs:108:5–287:6` (v1: first edit of `service.rs`) | ✅ no findings |
| 5 | plan `13-service-peer-forwards` | `extract_module` `peer_forwards`, `to_file` | items (6 members) | ✅ no findings |
| 6 | **H1–H9** (one commit) | hand moves, proof script, `cargo check -p tddy-host-service --all-targets` | — | consented |
| 7 | plan `20-service-host-keys` *post-move* | `extract_module` `host_keys`, `to_file`, `reexport: none` over `add_host_key_impl`, `list_host_key_candidates_impl`, `list_ssh_config_hosts_impl` | items | at implementation |
| 8 | plan `21-service-prompts` *post-move* | `extract_module` `prompts` over `stream_host_prompts_impl`, `answer_host_prompt_impl` | items | at implementation |
| 9 | plan `22-service-listings` *post-move* | `extract_module` `listings` over `list_eligible_daemons_impl`, `list_known_hosts_impl`, `get_host_tooling_impl` | items | at implementation |
| 10 | plan `23-service-stats-stream` *post-move* | `extract_module` `stats_stream` over `stream_host_stats_impl` | items | at implementation |
| 11 | plan `27-service-add-host-key` *post-move* | 3 × `extract_method`, bottom-up, in `service/host_keys.rs` `add_host_key_impl`: the blocking-pool unlock (`let keypair …` → `.map_err(…)?;`) → `add_key_on_blocking_pool`; the prompt issue and wait (`let prompt …` → the `match`) → `await_passphrase_answer`; the OS-user resolution (`let github_user …` → `.to_string();`, the range probe c was refused on) → `os_user_for_session` | item + relative ranges | at implementation; the inherent-method form is what plan 16 proves today |
| 12 | plan `16-host-targets-start-host-stream` | 2 × `extract_method`, bottom-up | item + relative ranges | ✅ no findings |
| 13 | **H10** (one commit) | hand move, proof script, `cargo check -p tddy-screen-sharing --all-targets` | — | consented |
| 14 | plan `24-start-stream-bridge` *post-move* | `extract_method` `start_session_bridge` over the `let mut started = None; if let Some(ref config) …` block of `start_stream_impl` (probe a's range) | item + relative range | at implementation |
| 15 | plan `25-start-stream-module` *post-move* | `extract_module` `session_stream`, `to_file`, over `start_stream_impl` + `start_session_bridge` | items | at implementation |
| 16 | plan `17-bridge-start-bridge` | `extract_method` `publish_to_livekit` | item + relative range | ✅ no findings |
| 17 | **H11** (one commit) | hand move, proof script, `cargo check -p tddy-rdp --all-targets` | — | consented |
| 18 | plan `26-rdp-connect` *post-move* | 2 × `extract_method`, bottom-up, in `RdpClient::connect_impl`: the finalize step (`let server_public_key …` → `})?;`) → `finalize_connection`; the `let config = Config { … };` literal (probe b's range) → `connector_config` | item + relative ranges | at implementation |

The three `probe-*` plan files stay as the evidence of the refusals:

```
this seam cannot be cut here: rust-analyzer offers no "extract into function" assist for the given range (it offered: Extract into variable)
```

**A post-move plan the engine refuses is a stop**, not a licence for another hand move: the consent covers
H1–H11 only.

## Applied

Everything above was applied in `## Delta` order, one commit per plan or hand-move file (20 commits over the
docs commit `3970ed4f4`), except as recorded here. The post-move plans 20–27, written at implementation, are
copied into the plans directory; their operations:

| Plan | Operation(s) | Anchor |
|---|---|---|
| `20-service-host-keys` | `extract_module` `host_keys`, `to_file`, `reexport: none` | items `HostServiceImpl::{add_host_key_impl, list_host_key_candidates_impl, list_ssh_config_hosts_impl}` |
| `21-service-prompts` | `extract_module` `prompts`, same flags | items `{stream_host_prompts_impl, answer_host_prompt_impl}` |
| `22-service-listings` | `extract_module` `listings`, same flags | items `{list_eligible_daemons_impl, list_known_hosts_impl, get_host_tooling_impl}` |
| `23-service-stats-stream` | `extract_module` `stats_stream`, same flags | item `stream_host_stats_impl` |
| `24-start-stream-bridge` | `extract_method` `start_session_bridge` | item `start_stream_impl`, relative 26:9–59:10 |
| `25-start-stream-module` | `extract_module` `session_stream`, same flags | items `{start_stream_impl, start_session_bridge}` |
| `26-rdp-connect` (**reduced**) | `extract_method` `connector_config` only | item `RdpClient::connect_impl`, relative 14:9–57:11 |
| `27-service-add-host-key` | 3 × `extract_method`, bottom-up: `add_key_on_blocking_pool`, `await_passphrase_answer`, `os_user_for_session` | item `service::host_keys::HostServiceImpl::add_host_key_impl`, relative 59:9–76:90, 33:9–47:11, 20:9–29:26 |

**Engine refusal (not worked around):** plan 26's `finalize_connection` (`let server_public_key …` → `})?;`) was
refused by `check --deep`, twice, against a warm index: the assist wrote `server_cert: _` (E0121, the TLS
certificate type `ironrdp_tls::upgrade` returns cannot be spelled). Developer decision (2026-10-21): apply
`connector_config` alone. `connect_impl` is **72** lines, not the projected 49. Entry:
`docs/dev/todo/2026-10-21-restructure-extract-assist-cannot-spell-an-opaque-parameter-type.md`; the rdp todo
entry is narrowed to the finalize step.

**Hand edits beyond H1–H11** (each developer-approved 2026-10-21 as a post-move build or lint fix; one engine
defect entry each):

| # | Class | File | Edit | Entry |
|---|---|---|---|---|
| E1 | build fix, E0446 | `host_tooling/subprocess.rs` | `struct RunningProbe` → `pub(super) struct RunningProbe` + `TODO` | `2026-10-21-restructure-widened-field-leaves-its-private-type` |
| E2 | build fix, E0277 | `screen_sharing_service/host_targets.rs` (`find_host_target`) | one stray `*` before `format!(` deleted | `2026-10-21-restructure-extract-assist-writes-a-stray-deref-before-format` |
| E3 | build fix E0283, then lint fix `clippy::extra_unused_type_parameters` | `tddy-screenshare/src/bridge.rs` (`publish_to_livekit`) | first a `::<C>` turbofish at the call; then the copied, unused `<C: ScreenSharingClient>` and the turbofish deleted | `2026-10-21-restructure-extract-assist-copies-an-unused-generic` |
| E4 | lint fix, `clippy::let_and_return` | `tddy-rdp/src/rdp_client.rs` (`connector_config`) | returns the `Config { … }` literal instead of `let config = …; config` | `2026-10-21-restructure-extract-leaves-a-let-and-return` |
| E5 | build fix (non-unix targets), found by `/validate-changes` | `github_cli.rs`, `host_tooling.rs`, `host_tooling/readings.rs` | `#[cfg(unix)]` restored on the `use` lines plans 11–12 wrote for `#[cfg(unix)]` items (at `origin/master` the `github_cli.rs` import was `#[cfg(unix)]`) | `2026-10-21-restructure-move-drops-cfg-on-repointed-imports` |

**Tool edits:** `cargo fix` removed 13 unused imports in `subprocess.rs` (plan 12) and the now-unused `mut` on
`prepared` (plan 16); `cargo fmt` on every touched package.

**`restructure verify --against origin/master`** ([G]'s run on the final engine head; not re-run at
`/validate-changes`, which relied on it and proved the residue by script instead): `406953 → 407018`
statements, 20 visibility-normalised, `tokens lost: none`, and `Error: 2 statement(s) the tree lost and 67 it
gained`. The two lost: `let mut prepared = prepare_bridge(` (the `mut`, E-tool) and the `started = Some(self.try_spawn_bridge(…` 
statement plan 24 moved, whose arguments now read the helper's `&str` parameters. Both are shown equivalent in
`## Validation Results`; the 67 gained are the delegators, signatures, call sites and returns listed under
`## Visibility` § moved-line diff.

**Measurements (applied tree)** — production lines measured two ways by script (`honest`: total minus every
`#[cfg(test)]`-gated item; `first`: lines before the first `#[cfg(test)]`); the two agree on every file here:

| File / function | Before | After |
|---|---|---|
| `host_tooling.rs` | 806 | **396** |
| `host_tooling/{desktop_probes, readings, subprocess}.rs` | — | 100, 164, 184 |
| `service.rs` | 972 | **242** |
| `service/{builders, host_keys, listings, peer_forwards, prompts, stats_stream}.rs` | — | 234, 251, 135, 137, 123, 123 |
| `screen_sharing_service.rs` | 482 | 415 |
| `screen_sharing_service/session_stream.rs` | — | 116 |
| `screen_sharing_service/host_targets.rs` | 323 | 346 |
| `tddy-screenshare/src/bridge.rs` | 399 | 404 |
| `tddy-rdp/src/rdp_client.rs` | 446 | 459 |
| `start_stream` → `start_stream_impl` | 72 | **41** (+ `start_session_bridge` 44) |
| `start_host_stream_impl` | 76 | **53** (+ `start_bridge_for_desktop` 26, `find_host_target` 18) |
| `start_bridge` | 58 | **31** (+ `publish_to_livekit` 31) |
| `RdpClient::connect` → `connect_impl` | 115 | **72** (+ `connector_config` 46) — projected 49 not reached |
| `add_host_key` → `add_host_key_impl` | 86 | **48** |
| `stream_host_stats` → `stream_host_stats_impl` | 96 | 96 — unchanged, out of scope (`## Boundaries`) |

## Callers

| | |
|---|---|
| External importers | `host_tooling`: none outside the crate for the moved items; `github_cli.rs` (same crate) imports `start_probe`, `ProbeOutput`, re-pointed by `reexport: outside`. `service`: none — methods are reached through the type; the trait signature is unchanged. `screen-sharing`, `screenshare`, `rdp`: private functions only |
| Facade | `reexport: "outside"` on the three `move_item`s; `none` on every `extract_module` |
| Consequence | no file outside the four packages changes |

## Visibility

Every widening, **read from the diff** (`git diff origin/master...HEAD`, each `pub(super)`/`pub(crate)` at
HEAD compared with the item at `origin/master`; `/validate-changes`, 2026-10-21). Items already `pub(crate)` at
`origin/master` (`ProbeOutput` and its fields, `StartedProbe`, `start_probe`, `StartedProbe::collect`,
`summarise`, `HOST_CPU_INTERVAL`, `HOST_DISK_INTERVAL`) are not widenings. The hand-moved `*_impl` methods and
`start_session_bridge` are new items, `pub(crate)` by design. No widened item's privacy was enforcing an
invariant: each is a free function or a field read by the tests of the same parent module, or a method of the
same type now reached from a sibling module.

| Item | Plan | Was | Is | Why it had to widen | Doc comment fixed |
|---|---|---|---|---|---|
| `mod subprocess` (new module) | 12 | — | `pub(crate) mod` | `github_cli.rs` imports `start_probe`, `ProbeOutput` from it | n/a |
| `start_desktop_probes`, `desktop_readings_of`, `warn_if_a_desktop_probe_failed` | 10 | private | `pub(super)` | the `probe` impl in `host_tooling.rs` | n/a |
| `start_agent_probe`, `agent_status_of`, `not_on_path`, `warn_if_failed`, `git_identity_of`, `git_config_value`, `github_cli_of` | 11 | private | `pub(super)` | the `probe` impl in `host_tooling.rs`; `github_cli_of` also its tests | n/a |
| `StartedProbe::running`, `RunningProbe::child` (fields) | 12 | private | `pub(super)` | `host_tooling.rs` tests | n/a |
| `struct RunningProbe` | 12 (hand edit E1) | private | `pub(super)` | E0446: the widened `StartedProbe::running` names it; marked `TODO` | n/a |
| `record_rpc_activity`, `eligible_instance_ids`, `classify_addressed_daemon_route`, `common_room_slot`, `rpc_served_by_peer`, `stream_served_by_peer` | 13 | private | `pub(crate)` | called from `service.rs` and the `service/*` handler modules | n/a |
| `cached_tooling` | 14 | private | `pub(crate)` | `get_host_tooling_impl` (`service/listings.rs`) | n/a |
| `start_session_bridge` (new in plan 24) | 25 | private | `pub(crate)` | `extract_module`'s default for a moved member; called only from `start_stream_impl` in the same module | n/a |

Plans 16, 17, 20–24, 26 and 27 widen nothing: `find_host_target`, `start_bridge_for_desktop`,
`publish_to_livekit`, `connector_config`, `os_user_for_session`, `await_passphrase_answer` and
`add_key_on_blocking_pool` are private.

**Moved-line diff** (`/validate-changes`, scripted: removed vs added lines over `origin/master...HEAD` per
package, whitespace and visibility normalised, `use`/`mod`/blank lines ignored): **no comment line lost** in
any package; the residue is only (a) the eleven hand-move signatures, doc lines and delegators, (b) the
extracted helpers' signatures, call sites and `Ok(..)` returns, (c) three signatures rustfmt re-wrapped once
`pub(super)` lengthened them (`start_agent_probe`, `git_identity_of`, `git_config_value`), (d) re-wrapped
`use` lists, (e) the `RunningProbe` TODO, (f) `let mut prepared` → `let prepared` and `let config = Config {`
→ `Config {` (hand edit E4). The `&bridge_identity`/`&track_name`/`&livekit_room` arguments now pass the
helper's `&str` parameters (see `## Validation Results` § behaviour).

## Baseline

Recorded 2026-10-21 on `master` `ea5ba6fb1` (still `master`'s tip at this revision: `git fetch origin
master` shows no new commit), one package at a time, with `./dev cargo test --no-fail-fast -p <pkg> --
--test-threads=1` (the test command `./test -p` runs, without its prerequisite build of unrelated binaries —
disk). `./test -p` runs on `fa9b42215` gave the same counts for the first three.

| Gate | Before | After |
|---|---|---|
| `tddy-host-service` — unit 177; `askpass_prompt_rpc` 11, `clone_grants` 5, `github_cli_environment` 1, `peer_routing` 2, `stream_host_prompts_rpc` 3 | **199 passed / 0 failed / 0 ignored** | **199 / 0 / 0** ✅ |
| `tddy-screen-sharing` — unit 19; `bridge_readiness_acceptance` 6, `dead_vnc_service_removed` 3, `host_desktop_sign_in_acceptance` 15, `screen_sharing_record_propagation` 3, `screen_sharing_records_unit` 10, `screen_sharing_service_acceptance` 8, `session_vault_target_store_acceptance` 6, `x11_channels_acceptance` 9 | **79 / 0 / 0** | **79 / 0 / 0** ✅ |
| `tddy-screenshare` — unit 9 | **9 / 0 / 0** | **9 / 0 / 0** ✅ |
| `tddy-rdp` — `rdp_client_integration` 5, `rdp_inspect_integration` 2, `rdp_nla_integration` 4 | **11 / 0 / 0** | **11 / 0 / 0** ✅ |

**No failing test**; the failing set the applied tree is held to is empty.

**After** (`/validate-changes`, 2026-10-21, on the applied tree + E5, same command, one package at a time): **298 passed / 0 failed / 0 ignored**, equal to the baseline by name (empty failing set both sides). `cargo clippy -p <each> --all-targets -- -D warnings` clean on all four; `cargo fmt --check` clean.

**Known load-sensitive tests** (green here under a load average of 260–280; one of these failing alone, under
load, is re-run alone before it is called a regression):

- `tddy-host-service` probe tests ending in `… did not answer within 5s` (`host_tooling.rs:754`, the
  `PROBE_TIMEOUT` kill-and-reap path; `ssh_agent.rs:358`'s agent timeout): a real `gh`/`git`/`ssh-add` child
  slow to start under load is killed at the deadline. Workaround: re-run the single test with
  `--test-threads=1` on a quieter machine.
- `tddy-screen-sharing` `tests/x11_channels_acceptance.rs` (nine tests; `StandInX11vnc` polled every 20 ms,
  10 s launch deadline) and `tests/bridge_readiness_acceptance.rs` (six; `StandInBridge`, 20 s start
  deadline): `/bin/sh` stand-ins a starved scheduler can miss. Workaround: the same.

## Validation Results

### `/validate-changes` (2026-10-21, [W])

**Stack gate:** base `master`, merge-base = `origin/master` `e0256b346` (fetched; no rebase needed); `origin/master..HEAD`
is this PR's 21 commits only; no file outside the four packages' `src/` and docs is in the diff.

**Behaviour — hand moves, proved independently** (a script of the validator's own, not the commit messages:
body of each trait method at `origin/master` vs the `*_impl` body at the hand-move commit, byte for byte, and vs
HEAD with whitespace normalised; brace counting skips string literals and `//` comments):

| Move | Body lines | At hand-move commit | Delegator | At HEAD |
|---|---|---|---|---|
| H1–H5, H7–H9 | 21, 31, 39, 39, 49, 43, 49, 91 | byte-identical (`61cebebf4`) | one line `self.<name>_impl(request).await` | identical (moved whole to `service/*`) |
| H6 `add_host_key` | 81 | byte-identical (`61cebebf4`) | one line | 43: the three plan-27 ranges replaced by their calls |
| H10 `start_stream` | 67 | byte-identical (`ae0257247`) | one line | 36: the plan-24 range replaced by its call |
| H11 `connect` | 109 | byte-identical (`cc74002e0`) | `Self::connect_impl(host, port, credentials).await` | 66: the plan-26 range replaced by its call |

**Behaviour — helper extractions, read in the diff:** every extracted body is the original range verbatim (the
moved-line diff loses no line but the bound names below); each call passes the same values in the same order;
`?`, `.await` and `map_err` sit where they were; no extracted helper owns a value with a `Drop` side effect
that is now dropped earlier (`PendingPrompt`, `StartStreamRequest`, `StartHostStreamRequest` are plain data).

- plan 27: `os_user_for_session(&req)?` returns `(github_user, os_user)` with the same two `ok_or_else`
  mappings; `await_passphrase_answer(&req, github_user)` issues, claims and awaits exactly as before (it ends
  `let answer = match …; answer`, the assist's form — clippy-clean, untouched); `add_key_on_blocking_pool(&req,
  os_user, encrypted_answer).await?` moves the same values into the same `spawn_blocking` closure with the same
  `map_err`.
- plan 16: `find_host_target(&req, scope)?` is the original chain; `start_bridge_for_desktop(req, operator,
  config, target, prepared, sign_in).await?` runs the X11 start, `start_signed_in_bridge` and their `?`s in the
  original order and returns `started`, from which the caller builds the same `StartStreamResponse`.
- plan 17: `publish_to_livekit(config, width, height).await?` creates the channel, connects, publishes and spawns
  the RPC loop in the original order before `request_frame_update`; errors keep their `classify(…, FailureKind::…)`.
- plan 26: `connector_config(credentials)` builds the same literal, still after the TCP connect; it reads only
  `credentials`.

**The two statements `restructure verify` reported lost:**

1. `let mut prepared = prepare_bridge(` → `let prepared` — **equivalent.** The only mutation (`&mut prepared` for
   the X11 start) moved into `start_bridge_for_desktop`, whose parameter is `mut prepared: PreparedBridge`; the
   outer binding is moved into that call and never touched again (the compiler accepts the immutable binding,
   which it would reject if it were mutated).
2. `started = Some(self.try_spawn_bridge(config, &bridge_target, target.username.clone(), password,
   &bridge_identity, &track_name, &livekit_room, session_bridge_key(…)).await?)` → the same statement inside
   `start_session_bridge` with `bridge_identity, track_name, livekit_room` — **equivalent.** At `origin/master`
   the arguments were `&String` deref-coerced to `try_spawn_bridge`'s `&str` parameters; now the caller passes
   `&bridge_identity`, `&track_name`, `&livekit_room` to `start_session_bridge`'s `&str` parameters (the same
   coercion, one call earlier) and they are forwarded unchanged: the same three strings, borrowed from the same
   locals of `start_stream_impl`, which still own them and still move them into `StartStreamResponse`
   afterwards. `config` is still `ref config` from `self.config`; `store` is the same `&Arc<dyn
   ScreenSharingTargetStore>` (`require_target_store` returns a reference); `req` moves into the helper, and
   `start_stream_impl` reads none of its fields after the call. The `?` inside the helper returns the same
   `Status`, and the caller's `.await?` propagates it unchanged; the `None`/`Err` arms still only log.

**Hand edits E1–E4 and tool edits** (see `## Applied`): each is one of the approved classes and none touches a
moved body. **E5** (new, from this validation): `#[cfg(unix)]` restored on the re-pointed `use` lines — on unix
targets a no-op, on others it restores what `origin/master` compiled.

**Tests changed only mechanically:** no `#[test]`, `#[tokio::test]`, `#[ignore]`, sleep or `.skip` added or
removed in the diff; no file under any `tests/` changed; the only test-code edits are three re-pointed `use`
lines in `host_tooling.rs`'s `mod tests`.

**`restructure verify`:** not re-run here (the index daemon is a 6–10 minute cold load on a tight disk); relied on
[G]'s run and accounted for its 2 lost / 67 gained by the scripts above.

**Code issues:** no record under the four packages' `docs/code-issues/` carries `Claimed by:`.
`oversized-file-host-tooling.md` (396) and `oversized-file-service.md` (242) close; `oversized-file-host-registry.md`
and `complexity-host-messages-unlock-and-add.md` name files this PR does not touch.

**Risk summary:** Critical 0 · Warning 1 (the dropped `#[cfg(unix)]`, fixed as E5) · Info 2 (plan 26 reduced;
`connect_impl` 72 and `stream_host_stats_impl` 96 stay over 60, recorded).

## Plan

[`2026-10-21-host-desktops-sweep-host-service-plans/`](./2026-10-21-host-desktops-sweep-host-service-plans/) —
the seven engine plans proven at planning (10–14, 16, 17), the post-move plans 20–27 as written at
implementation (copied in at `/validate-changes`; an apply rewrites a plan's anchors, so these are the
applied files, and plan 26 holds only `connector_config`), and the three refused probes. Their operations are
tabled in `## Applied`.

## Decisions & Trade-offs

1. **Engine plans 14 and 13 run before the hand moves**, although the decision text says hand moves come
   "before the engine plans": those two plans cut the *inherent* impl, not the trait bodies, and plan 14 is
   range-anchored, so running it first is what keeps it valid without re-anchoring. The engine plans that
   cut the hand-moved `*_impl` methods (20–23) do follow the hand moves. If the developer reads the decision
   as "hand moves first, full stop", plan 14 is re-anchored after H1–H9; nothing else changes.
2. **The hand-moved methods land in an inherent block of the same file first**, and the engine moves them
   out: the consent is for moving bodies out of the trait, not for creating modules by hand.
3. **`service.rs` transiently grows to ~1,170** between H1–H9 and plan 23. Each step is its own commit, so
   the tree compiles at every commit and a failure bisects to one step.
4. **`add_host_key` is shortened as part of the consented work** (86 → ~48) although it did not grow in the
   stack, because the developer named it; `stream_host_stats` (96) is not, because nobody did.
5. **Score 5** is a warning that the boundary may be wrong: this PR crosses four crates and mixes hand moves
   with engine plans. It was split from the config work at `6e`; a further split (host-service / start paths)
   is possible with no edge between the halves.

## Final Checklist

Tasks executed by `/wrap-context-docs` before this changeset is deleted.

- [ ] `packages/tddy-host-service/docs/changesets/2026-10-21-host-tooling-and-service-split.md` — release-note file: modules, the hand moves and their proof, before/after numbers, baseline
- [ ] `packages/tddy-screen-sharing/docs/changesets/2026-10-21-start-paths-shortened.md` — release-note file
- [ ] `packages/tddy-screenshare/docs/changesets/2026-10-21-start-bridge-helper.md` — release-note file
- [ ] `packages/tddy-rdp/docs/changesets/2026-10-21-connect-helpers.md` — release-note file
- [ ] `packages/tddy-screenshare/docs/changesets/2026-10-21-bridge-ready-link-correction.md` — correction to `2026-10-20-bridge-ready`: its "Bridge start report" link now lives at `../../../tddy-daemon-extensions/docs/bridge-readiness.md` (the wrapped entry is not edited)
- [ ] `docs/dev/changesets/2026-10-21-host-desktops-sweep-host-service.md` — cross-package history entry, with each closed record's final measurement
- [ ] Delete the records and entries marked ✅ RESOLVED HERE (after their final rows are in the history entry); narrow any that the applied numbers do not close
- [ ] `docs/dev/todo/2026-10-19-restructure-extract-method-refused-under-async-trait.md` — add probes a–c as evidence and the second consented hand move
- [ ] `packages/tddy-host-service/docs/host-service.md` file table (`:12`) — `service.rs` no longer holds the handlers' bodies, the `with_*` builders or the peer routing → rows for the six `service/` modules; add `host_tooling/{desktop_probes,readings,subprocess}.rs` beside `host_tooling.rs` (`:159`)
- [ ] `packages/tddy-host-service/docs/host-add-key.md` (`:29`) — "`service.rs` | the four handlers, their routing and their auth" → `service/host_keys.rs` (bodies), `service/peer_forwards.rs` (routing)
- Triaged as still true (no item): `host-tooling-probe.md` prose naming `host_tooling.rs` (the module, and its tests stay in that file) and `desktop_readings_of` / `start_desktop_probes` by name only; `screen-sharing-service.md:61`
- [ ] Re-run the doc triage (`grep -rn` of every moved symbol and old path over the four packages' README/docs) against what landed
