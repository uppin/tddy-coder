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

Record metadata keys: `preset`, `probed_at`, `endpoint.<wire protocol>`.

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

## Service

`AccountsServiceImpl::with_provider_accounts(store, probe)` wires the two RPCs; without it both
answer `FAILED_PRECONDITION`. `ConnectProviderAccount`: preset → key present → `endpoints_for` →
`AccountStore::list` (a locked vault refuses before the network) → probe (refusal:
`INVALID_ARGUMENT` "the <Provider> endpoint did not accept this key: <reason>") → `put_provider_account`.
`ListProviderAccountModels`: `NOT_FOUND` for an id the vault holds no provider account under;
a probe failure is `UNAVAILABLE`. `AccountSummary.preset` / `wire_protocols` are filled from
`ProviderAccount::from_record` and are empty for every other account.

## Tests

- `src/provider_account.rs` unit tests — the preset table, `endpoints_for` refusals, the codec, ids.
- `src/vault_store.rs` — simultaneous puts of one id each get their own; a held id is numbered.
- `tests/provider_accounts_acceptance.rs` — the RPCs over an in-memory store and a fake probe.
- `tddy-daemon/tests/provider_model_probe.rs` — the HTTP probe against a local stub endpoint.
