# Coder accounts (`src/coder_account.rs`)

A Coder account is an ordinary `CredentialRecord`: `provider` is `CODER_PROVIDER` (`"coder"`),
`secret` is the deployment session token, and the deployment and the user the token belongs to
live in `metadata`. `src/coder_account.rs` is the one place that knows how a record spells one.
Product view: [docs/ft/daemon/provider-accounts.md](../../../docs/ft/daemon/provider-accounts.md).

## Types

| Item | Role |
|---|---|
| `CoderDeploymentUrl` | a deployment root held without its trailing slash; `parse` refuses a non-`http(s)` URL, one without a host, or one carrying userinfo, a query or a fragment |
| `CoderUser` | `id` and `username` of the person a token belongs to, as the deployment names them |
| `CoderProbeError` | `Rejected` (the deployment refused the token), `Unreachable` (nothing answered), `Unusable` (something answered that is not a Coder deployment) |
| `CoderDeploymentProbe` | the connect-time check: `whoami(url, token)` asks the deployment who the token belongs to. A port because the HTTP client is the daemon's (`tddy_daemon::coder_deployment_probe::HttpCoderDeploymentProbe`); this crate stays free of a network stack |
| `CoderAccount` | `account_id`, `label`, `deployment_url`, `username`, `user_id`; `for_user`, `from_record` and `into_record(token, now)` are the codec. The account id is `coder-<user_id>@<deployment host>` |
| `CoderCredentials` | the `url` and `token` a later reader of an open vault is given |
| `CoderAccountError` | `InvalidUrl`, `MissingToken`, `Rejected`, `Unreachable`, `NotFound`, `Vault` |

Record metadata keys: `url`, `subject` (the username), `subject_id` (the user id), `probed_at`.

## Service

`CoderAccounts::connect` checks the vault is open before the network, so a closed vault refuses
without the token leaving the daemon. Connecting a deployment and user the vault already holds
replaces that record's token and keeps its account id and label.
`CoderAccounts::credentials_for(subject, account_id)` answers the stored URL and token while
that subject's vault is open, and refuses — never answers empty — while it is not.

`AccountsServiceImpl::with_coder_accounts(coder)` wires `ConnectCoderAccount`; without it that
RPC answers `FAILED_PRECONDITION`. The service maps a refusal to `INVALID_ARGUMENT` (the URL or
an empty token), `UNAUTHENTICATED` (the deployment rejected the token), `UNAVAILABLE` (the
deployment could not be reached) or `FAILED_PRECONDITION` (a closed vault, or a daemon that
wired no Coder accounts). The daemon wires it in
`tddy_daemon::account_linking::accounts_service_over` over the person's `SessionVaults`.

## Tests

- `src/coder_account.rs` unit tests — the URL refusals, the codec round-trip, the account id.
- `tests/coder_account_acceptance.rs` — the RPC over a real vault sealed in a temporary
  directory, with a stand-in deployment: every refusal, the reconnect path, and reading the
  stored credentials back while the vault is open.
- `tddy-daemon/tests/coder_deployment_probe.rs` — the HTTP probe against a local listener: the
  `Coder-Session-Token` header, a rejected token, and a server that is not a Coder deployment.
