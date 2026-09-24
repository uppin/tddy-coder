# oversized-file: service.rs

**Location:** `packages/tddy-host-service/src/service.rs`
**Category:** oversized-file
**Detected:** 2026-09-24 — `/pr-wrap` step 3.5 file-length gate on #509 (`#keyring` 2/9), production lines counted to the first `#[cfg(test)]`
**Metrics:** **939 production lines** of 939 total (no `#[cfg(test)]`; 0 test lines) · budget 500 · **1.9× over**
**Thresholds breached:** length 939 > 500
**Restructure:** not designed
**Status:** Open — **unclaimed**
**Verified:** ⚠ **not hand-verified** — the line count is machine-measured; the seam table is a first reading of the item list

## Measurement history

| Run | Production lines | Note |
|---|---|---|
| 2026-09-24 | 939 | first detection. The size predates #509 (939 at merge-base `4e7157d2`). #509 changed 6 lines (3+/3−), all of them the `os_user_for_github` binding (`let os_user = self…` → `&self…`). It did not grow the file |

## What would close it — candidate seams (not proven)

| Seam | Items | Out |
|---|---|---|
| A | peer routing, which is inherent and moves without stubs: `record_rpc_activity`, `eligible_instance_ids`, `classify_addressed_daemon_route`, `common_room_slot`, `rpc_served_by_peer`, `stream_served_by_peer` (L250–375) | ~125 |
| B | host keys and SSH, from the `HostService` impl: `add_host_key`, `list_host_key_candidates`, `list_ssh_config_hosts` (L632–842) | ~210 |
| C | `stream_host_stats` (L843–938) | ~95 |
| D | prompts: `stream_host_prompts`, `answer_host_prompt` (L520–631) | ~110 |
| — | stays: struct, constructor and `with_*` builders (L47–249), plus `list_eligible_daemons`, `list_known_hosts`, `get_host_tooling` (L382–519) | — |

Seams B, C and D sit inside a trait impl. Their bodies move to free functions and the impl keeps delegating stubs. From this first reading, A+B+C take it to about 510 lines with stubs, so D is also needed to get under budget. Prove the seams with `restructure check --deep` before applying them.

**Stack constraint:** `#keyring` dependents #510–#516 are still open, so no split lands until that stack does. `#keyring` 3/9 (#510) touches this file, so a split now would conflict with it.
