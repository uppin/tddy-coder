# 2026-10-22 — A Telegram-started tool session cannot choose a provider account

**Category:** Deferred from `#provider-route` 4/9 (#665)
**Source:** `/pr-wrap` on `feature/provider-route/tool-route`

`packages/tddy-telegram-control/src/telegram_session_control/workflow_spawn.rs` builds its
`SpawnOptions` with `provider_route_json: None`. A tool session started from Telegram always runs on its
harness's vendor, because the Telegram start has no account choice and does not go through
`tddy-agent-launch`'s route resolution.

**Why deferred:** the Telegram flow has no account picker and no session token for the owner's vault.
Out of #665's scope (web and daemon starts).

**What would close it:** the Telegram start resolving a route the way `tool_session_route_json` does —
at least the owner's per-harness default once `#provider-route` 5/9 lands — and passing its JSON here.

**Re-checked 2026-11-06 (`#agent-tools` 18/19, #824):** Telegram's CLI agent starts
(`/start-agent`, `/start-claude`, `/start-cursor`) now go through the generic start, so they take
its route resolution — but with no session token, so no vault account can be opened, and there is
still no account picker in Telegram. The `/start-workflow` spawn (`workflow_spawn.rs`) is unchanged
and still passes `provider_route_json: None`. Still open.
