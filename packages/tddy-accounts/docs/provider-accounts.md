# Provider accounts (`src/provider_account.rs`)

A model-provider account is an ordinary `CredentialRecord`: `provider` is the preset id, `secret`
is the API key, and the endpoints it serves live in `metadata`. `src/provider_account.rs` is the one
place that knows how a record spells one. Product view:
[docs/ft/daemon/provider-accounts.md](../../../docs/ft/daemon/provider-accounts.md).

## Types

| Item | Role |
|---|---|
| `WireProtocol` | `AnthropicMessages`, `OpenAiResponses`, `OpenAiChat`; `as_str()` is the wire spelling |
| `PresetEndpoint` | per protocol: `NotServed`, `Fixed(root)`, `OperatorSupplied` |
| `ProviderPreset`, `presets()`, `preset(id)` | the six-preset table, in the order the screen offers them |
| `endpoints_for(preset, anthropic_base_url, openai_base_url)` | resolves a `ProviderEndpoints`, or a `ProviderAccountError` |
| `ProviderAccount` | `account`, `preset`, `label`, `endpoints`, `api_key`; `to_record(now)` / `from_record(&record)` are inverse; `from_record` is `None` for any non-provider record (GitHub, screen-sharing) |
| `account_id_for(preset, label)` | the id a new account asks for: `<preset>-<label slug>`, the slug alone when it already starts with `<preset>-`, or the bare preset id |
| `unclaimed_account_id(wanted, held)` | `wanted`, or the first free `<wanted>-<n>` from 2 |

`ProviderAccountError`: `UnknownPreset`, `MissingBaseUrl`, `FixedBaseUrl`, `UnservedBaseUrl`,
`InvalidBaseUrl`, `MissingApiKey`. The service maps every one to `INVALID_ARGUMENT` with a sentence
for the person. `UnservedBaseUrl` exists so a URL the operator typed is refused rather than dropped;
since one OpenAI root feeds both OpenAI protocols, it is unserved only when the preset serves neither.

Record metadata keys: `preset`, `probed_at`, `endpoint.<wire protocol>`, and `default_for`
(`META_DEFAULT_FOR`: the harness ids the account is its owner's default for, comma-separated).
`defaults_of(&record)` reads it (empty for a non-provider record); `default_account_for(held,
harness)` is the provider account in `held` that is the default for `harness`. `refuse_undrivable(matrix,
&account, harnesses)` is the first harness `RouteSupport` refuses for the account, with its reason.

## Ports

- **`ProviderModelProbe`** (`async list_models(endpoints, api_key) -> Result<Vec<String>, String>`)
  — the connect-time key check and the model listing. A port because the HTTP client is the
  daemon's (`tddy_daemon::provider_model_probe::HttpProviderModelProbe`); this crate stays free of a
  network stack and does not depend on `tddy-model-registry`.
- **`ProviderAccountStore`** — `put_provider_account(session_token, &account)` and
  `provider_account(session_token, &account_id)`. `put_provider_account` never writes over a held
  record: it stores under `unclaimed_account_id(account.account, held)` and returns the record with
  the id it got. `SessionVaultAccountStore` makes that choice atomic with the write by holding a
  store-wide mutex from the vault read to the `put` — the vault serialises each operation, not the
  read-then-write between them.
- **`RouteSupport`** (`harness_routable(harness) -> Result<(), String>`, `supports(harness, preset,
  protocols) -> Result<(), String>`) — which harness can run on which account. A port because the
  matrix is `tddy_agent_backend::provider_route`'s; this crate does not depend on the harness crates.
  The daemon's implementation is `tddy_daemon::provider_route_support::HarnessRouteSupport`.
- **`ProviderAccountDefaults`** (`set_provider_account_defaults(session_token, &account, harnesses)
  -> Result<Vec<CredentialRecord>, AccountsError>`) — a port of its own, so `ProviderAccountStore` is
  not widened. `SessionVaultAccountStore` lists the vault under the same store-wide mutex as
  `put_provider_account`, sets the named provider account's `default_for` to `harnesses` (deduplicated,
  in order), removes those harnesses from every other provider account, and `put`s only the records
  that changed, returning them. A non-provider record sharing the id is never touched. Each `put` is
  its own vault operation; a `set_label` or sync write landing between the list and the put is
  overwritten (`TODO(keyring)`, [backlog](../../../docs/dev/todo/2026-10-04-keyring-accounts-rename-lost-update.md)).

## Service

`AccountsServiceImpl::with_provider_accounts(store, probe)` wires the two RPCs; without it both
answer `FAILED_PRECONDITION`. `ConnectProviderAccount`: preset → key present → `endpoints_for` →
`AccountStore::list` (a locked vault refuses before the network) → probe (refusal:
`INVALID_ARGUMENT` "the <Provider> endpoint did not accept this key: <reason>") → `put_provider_account`.
`ListProviderAccountModels`: `NOT_FOUND` for an id the vault holds no provider account under;
a probe failure is `UNAVAILABLE`. `AccountSummary.preset` / `wire_protocols` are filled from
`ProviderAccount::from_record` and are empty for every other account.

`AccountsServiceImpl::with_route_support(support)` wires `ListProviderRoutes`; without it that RPC
answers `FAILED_PRECONDITION`. It lists the vault (`AccountStore::list`, so a locked vault refuses as
`ListAccounts` does), keeps the provider accounts in vault order, and marks each with
`RouteSupport::supports(harness, preset, wire protocols)` — or, when `harness_routable` refuses, marks
every account with that one reason and answers `harness_routable = false`, `harness_reason`. The
reasons are passed through verbatim.

`AccountsServiceImpl::with_provider_account_defaults(defaults)` wires `SetProviderAccountDefaults`;
it also needs `with_route_support`, and without either it answers `FAILED_PRECONDITION`. It finds the
named provider account through `AccountStore::list` (`NOT_FOUND` otherwise), refuses with
`refuse_undrivable` (`INVALID_ARGUMENT`, the matrix's reason) before anything is written, then calls
the port and answers with a summary per changed record. `summary_of` fills `default_for` from
`defaults_of`. The daemon wires it in `tddy_daemon::account_linking::accounts_service_over` over the
same `SessionVaultAccountStore`.

## Tests

- `src/provider_account.rs` unit tests — the preset table, `endpoints_for` refusals, the codec, ids.
- `src/vault_store.rs` — simultaneous puts of one id each get their own; a held id is numbered; a
  default moves to the named account and the previous holder loses it; a GitHub record sharing the id
  is untouched.
- `tests/provider_account_defaults_acceptance.rs` — `SetProviderAccountDefaults` over an in-memory
  store: a second default for a harness clears the first; an undrivable harness is refused; a row lists
  its defaults; no account is the default for a harness nobody chose.
- `tests/provider_accounts_acceptance.rs` — the RPCs over an in-memory store and a fake probe.
- `tests/provider_routes_acceptance.rs` — `ListProviderRoutes` over a fake matrix: provider accounts
  only, vault order, verdicts verbatim; an unroutable harness; the unwired refusal.
- `tddy-daemon/tests/provider_model_probe.rs` — the HTTP probe against a local stub endpoint.
