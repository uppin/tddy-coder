# Accounts screen (`src/components/accounts/`)

Every credential the selected daemon holds for the signed-in person, grouped by provider, with
**rename** and **remove**. Product view: [docs/ft/web/accounts-screen.md](../../../docs/ft/web/accounts-screen.md).
Service: [tddy-accounts](../../tddy-accounts/docs/accounts-service.md).

## Route and entry point

| | |
|---|---|
| Route | `ACCOUNTS_ROUTE = "/accounts"` and `isAccountsPath` in `src/routing/appRoutes.ts` — an exact match: not `/accounts-archive`, not `/accounts/github` |
| Dispatch | one branch in the `src/index.tsx` route chain, after Hosts |
| Nav | `shell-menu-accounts` in `DaemonNavMenu`, between **Hosts** and **VMs** |

**Not capability-gated.** Accounts is plain RPC over whatever wire the host connection was opened on,
so the screen reads no `capabilities` set and `useHasCapability` stays the single predicate
([capability gating](capability-gating.md#the-one-predicate)).

## Components

`AccountsAppPage` / `AccountsScreen` follow the `HostsAppPage` / `HostsScreen` split: the page owns
the data, the screen renders what it is given.

**`AccountsAppPage`** wraps `AppShell` (`title="Accounts"`, `data-testid="accounts-app-page"`) and
makes **one `ListAccounts` call per visit** against `useDaemonClient(AccountsService)`. The effect
re-fires only when the selected daemon or the session token changes, each of which invalidates the
answer; a `current` flag discards a reply from a daemon just navigated away from. A rename answers
with the account as it now stands and a removal with what remains, so neither re-reads. A failed
rename or removal is shown as `accounts-action-error` **beside** the list, which stays — the list is
still what the vault holds. The daemon's reason is shown without the transport's `[code]` prefix
(`ConnectError.rawMessage`).

**`AccountsScreen`** renders an `AccountsOutcome`:

| `kind` | From | Renders (`data-testid`) |
|---|---|---|
| `listed`, no providers | `providers` empty, both flags false | `accounts-empty` — nothing linked yet, and that an account appears once linked from its provider |
| `listed` | `providers` | `accounts-group-<provider>`, one `accounts-row-<provider>-<accountId>` per account |
| `uninitialized` | `vault_uninitialized` | `accounts-uninitialized` — no vault yet; choosing a passphrase creates one |
| `locked` | `vault_locked` | `accounts-locked` — unlock with the passphrase; nothing is lost |
| `error` | a rejected `ListAccounts` | `accounts-error` — the daemon's reason, verbatim |

The screen asks for no passphrase itself. The app-wide `CredentialVaultPrompt`
([daemon sign-in](daemon-sign-in.md)) does; the uninitialized and locked notices point at it and say
that reloading the page brings it back after "Not now".

A row is addressed by `provider` **and** `accountId` (`accounts-row-<provider>-<accountId>`): an
account id is unique only within its provider. Each row shows the label (`…-label`), the subject
(`…-subject`), "no credential stored" when `hasSecret` is false, `#keyring` 6/9's sync-status badge
(`…-sync-status`, `SyncStatusBadge`) when the account has one, a rename form (`…-rename-input`,
`…-rename-submit`) and a remove button (`…-remove`) whose first press only asks; the confirmation
(`…-remove-confirm`) sends `RemoveAccount`, and Cancel sends nothing.

### Connecting a provider account by API key

When the page passes `onConnectProviderAccount`, a listed screen shows **Connect API key**
(`accounts-connect-api-key`), which opens `ConnectProviderAccountForm` (`accounts-connect-form`):
preset select (`accounts-connect-preset`), label (`accounts-connect-label`), a password-type key
field (`accounts-connect-api-key-input`), and — only for a preset that needs one — the OpenAI root
(`accounts-connect-openai-base-url`) or the Anthropic root (`accounts-connect-anthropic-base-url`).
A URL typed for a preset that takes none is not sent. The page sends `ConnectProviderAccount`; on
success it closes the form and re-reads the list, on refusal the daemon's reason is shown as
`accounts-connect-error` and the form stays open.

`PROVIDER_PRESET_OPTIONS` mirrors the daemon's preset table (`tddy_accounts::presets()`), which
stays the authority: every request that disagrees with it is refused, so a drift shows on screen as
a refusal, never as an account pointed at the wrong endpoint.

A provider-account row (`AccountRow.preset` non-empty) also shows its preset and wire protocols
(`…-preset`) and a **Models** toggle (`…-models-toggle`) that sends `ListProviderAccountModels` only
when expanded, rendering `…-models` or the reason as `…-models-error` (`ProviderModels.tsx`).
`AccountRow.defaultFor` carries `AccountSummary.default_for`; when the page passes `onSetDefaults`,
the row shows `ProviderAccountDefaultsControl` — a multi-select of `claude-cli` / `opencode-cli`
(`…-default-for`) and **Save default** (`…-default-for-save`). The control is keyed on the current
defaults, so a row re-seeds when the daemon reports a change. `AccountsAppPage` sends
`SetProviderAccountDefaults` and replaces every row in `changed`; a refusal is shown beside the list.

### The sync-status badge

`AccountRow.syncStatus: AccountSyncStatus | null` — `"synced"`,
`"pending"`, `"undeliverable"`, `"conflict"`, `"refused"`, or `null` for `SYNC_STATUS_UNSPECIFIED`
(nothing has synced the account yet, the common case for a daemon with no `keyring.group_secret`
configured). `null` renders no badge at all — not an empty or zero-value one. `syncStatusFromRpc`
(`AccountsAppPage.tsx`) maps the wire enum; `SyncStatusBadge` (`AccountsScreen.tsx`), extracted as
its own small component rather than inlined into `AccountRowView`, renders the five visible states.
See [credential sync](../../tddy-credential-sync/docs/credential-sync.md) for what the five answers
mean and [accounts service](../../tddy-accounts/docs/accounts-service.md) for the port that supplies
it.

## Tests

| Spec | Pins |
|---|---|
| `cypress/component/AccountsScreenAcceptance.cy.tsx` (page object `cypress/support/pages/accountsScreenPage.ts`) | grouping by provider; the subject beside the label; empty, uninitialized, locked and errored as four distinct notices; rename through `SetAccountLabel` with the identity unchanged; a failed rename reported beside the list; removal only after confirmation; no sync badge for `UNSPECIFIED`, a `Refused` badge, a `Synced` badge |
| `cypress/component/ProviderAccountDefaultsAcceptance.cy.tsx` | marking an account the Claude CLI default sends `SetProviderAccountDefaults`; a default taken by another account leaves the row that held it; the New-session form pre-selects the default |
| `cypress/component/ProviderAccountConnectAcceptance.cy.tsx` | connecting a Meta account adds it to its provider group with its preset; a rejected key shows the daemon's reason and adds nothing; the URL field appears only for presets whose endpoint the operator supplies; the key is never shown once sent |
| `src/routing/appRoutes.test.ts` § accounts route | `/accounts` matches; `/accounts-archive` and `/accounts/github` do not |
| `ModelsNavAcceptance.cy.tsx`, `PresenceCapabilityGatingAcceptance.cy.tsx` | the menu's order, with **Accounts** after **Hosts** |

All use `mountWithRpc` + `anInMemoryRpcBackend`; Storybook: `AccountsScreen.stories.tsx` (Listed,
Empty, Uninitialized, Locked, Errored).

⚠ `AccountRowView`, `renderOutcome` and `AccountsAppPage` are each over 60 lines; the split is
tracked in `docs/dev/todo/2026-10-04-keyring-accounts-screen-long-components.md`.
