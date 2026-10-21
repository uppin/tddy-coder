# 2026-10-21 — The web's provider-preset options mirror the daemon's table by hand

**Category:** Deferred from `#provider-route` 1/9 (#662)
**Source:** `/validate-changes` on `feature/provider-route/accounts`

`PROVIDER_PRESET_OPTIONS` in `packages/tddy-web/src/components/accounts/ConnectProviderAccountForm.tsx`
restates, per preset, its label and whether it needs an OpenAI or an Anthropic base URL
(`needsOpenAiBaseUrl` / `needsAnthropicBaseUrl`). The authority is
`tddy_accounts::provider_account::presets()`. A drift is not dangerous — the daemon refuses an unknown
preset, a missing operator-supplied URL, an override of a fixed one and a URL for an unserved protocol,
so the screen shows a refusal, never an account pointed at the wrong endpoint — but adding a preset
means editing both tables.

**Why deferred:** removing the mirror needs a new RPC (for example `ListProviderPresets` on
`AccountsService`, answering each preset's id, label and the per-protocol `PresetEndpoint` kind), a proto
change no later `#provider-route` node owns, and #662's contract did not include it.

**What would close it:** that RPC, the form reading its options from it, and the hard-coded array
deleted.
