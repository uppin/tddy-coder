# oversized-file: config.rs

**Location:** `packages/tddy-supervisor/src/config.rs`
**Category:** oversized-file
**Detected:** 2026-11-09 — `/pr-wrap` file-length gate on #859 (`#agent-tools` 31/32), production lines counted to the first `#[cfg(test)]`
**Metrics:** **507 production lines** · budget 500
**Thresholds breached:** length 507 > 500
**Restructure:** not designed — two self-contained seams exist (see below)
**Status:** Open — pushed over the budget by #859 (463 → 507); split deferred; unclaimed
**Verified:** machine-measured line count (`awk` to the first `#[cfg(test)]`) on #859's tree, 2026-11-09

## Measurement history

| Run | Production lines | Note |
|---|---:|---|
| 2026-11-09 | 463 → 507 | grown by #859 (`#agent-tools` 31/32): the top-level `users:` block (`UsersConfig`, `default_user_sockets_dir`), `ManagedService::serves_users`, `refuse_old_user_list_keys` (the load error naming `users.yaml` for `allowed_session_users`, `host_sockets`, `agent_tool_sockets`), and the trust checks widened to `pub(crate)` for `users_file.rs`. Not split there: #865 in the same stack also modifies this file, so a split mid-stack would conflict with it |

## What would close it

`config.rs` under 500 production lines. Two seams are already self-contained:

1. **The trust checks** — `require_trustworthy_config_file`, `require_trustworthy_config_directory`
   (lines ~424–498, about 75 lines), which `SupervisorConfig::load` and `UsersFile::load` both call:
   into `config/trust.rs`.
2. **The user-mapping surface** — `UsersConfig`, `default_user_sockets_dir` and
   `refuse_old_user_list_keys`: into `config/users.rs`, beside `config/spawn_policy.rs`.

Either alone clears the budget. Both are plain `move_item`s for the `code-restructuring` engine once
#865 has landed.
