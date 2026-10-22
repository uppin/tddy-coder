# 2026-10-22 — A default provider account per harness

**Type:** Feature

`#provider-route` 5/9 ([#666](https://github.com/uppin/tddy-coder/pull/666)), across `tddy-service`
(`SetProviderAccountDefaults`, `AccountSummary.default_for = 10`), `tddy-accounts` (the `default_for`
record key, the `ProviderAccountDefaults` port and its vault implementation, the RPC validated
against the route matrix), `tddy-agent-launch` (`session_route_sources_with_default` feeding
`RouteSources.user_default` into the unsandboxed claude-cli and opencode-cli starts and the sandboxed
claude-cli refusal), `tddy-daemon` (wiring the port over `SessionVaultAccountStore`) and `tddy-web`
(the row's Default-for control and the Create pane's pre-selection).

Decisions: the default is per person, in the vault record — a daemon-wide default would need a
daemon-held key, which does not exist. A start that names no account while the owner's vault is
locked is **refused** rather than run on the vendor (no unconsented fallback); an explicit account or
"built in" choice never reads the vault for a default, so it is never refused for that reason. The
port is new rather than a widening of #662's `ProviderAccountStore`.

Validation found and fixed in this PR: an explicit choice was refused on a locked vault; the defaults
write could stamp `default_for` on a non-provider record sharing the account id; the sandboxed start
ignored a default (now refused like an explicit account); a row's control kept stale selections after
another row took its harness; the control offered `muse-cli`, which no session type can start yet.

Docs: [provider accounts § A person's default per harness](../../ft/daemon/provider-accounts.md#a-persons-default-per-harness),
[accounts screen](../../ft/web/accounts-screen.md),
[Claude Code CLI session](../../ft/daemon/claude-cli-session.md#running-on-a-provider-account),
[tool-session model selection § Provider](../../ft/web/tool-session-model-selection.md#provider),
[tddy-accounts provider accounts](../../../packages/tddy-accounts/docs/provider-accounts.md),
[tddy-agent-launch module layout](../../../packages/tddy-agent-launch/docs/module-layout.md#provider-route-of-a-session).

Backlog: `2026-10-04-keyring-accounts-rename-lost-update` (⚠ DURING) **kept and extended** with this
PR's second read-then-put site (`set_provider_account_defaults`, `TODO(keyring)`). **Added**
`2026-10-22-tool-session-starts-ignore-the-provider-default` (tool-session starts are #665's and were
still a `todo!()`). `2026-10-21-tddy-accounts-service-rs-over-file-budget` updated: 703 → 761
production lines, still deferred as stack-shared.

File budget: `packages/tddy-web/src/components/sessions/CreateSessionPane.tsx` 824 → 832 (stack-shared
with #663–#665; record `packages/tddy-web/docs/code-issues/oversized-file-create-session-pane.md`
regressed, split deferred). `AccountsScreen.tsx` 486 → 503 was decomposed here (`ProviderModels.tsx`),
ending at 455.
