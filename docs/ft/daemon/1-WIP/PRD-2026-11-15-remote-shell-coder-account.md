# Connect a Coder deployment as an account - PRD

**Date**: 2026-11-15
**PRD Type**: Enhancement
**Stack**: `#remote-shell 2/7` — branch `feature/remote-shell/coder-account`, base `feature/remote-shell/shell-port`

## Affected Features

- **Provider accounts**: [provider-accounts.md](../provider-accounts.md) — a second kind of pasted-credential account, outside the model-provider presets
- **Accounts screen**: [accounts-screen.md](../../web/accounts-screen.md) — a Coder group with its own connect form
- **Credential sync**: [credential-sync.md](../credential-sync.md) — Coder records propagate like any other vault record

## Summary

A signed-in person connects a [Coder](https://github.com/coder/coder) deployment on the Accounts screen by entering its URL and a session token. The daemon checks the pair against the deployment, seals it in the person's credential vault as a `coder` account, and lists it beside their GitHub and provider accounts.

## Background

Sessions on Coder workspaces need a deployment URL and a token. The vault already holds per-person credentials with an open provider string, so a `coder` record fits without a schema change, and listing, renaming, removing and syncing work for it unchanged.

Neither existing intake fits. `BeginLinkAccount` is a GitHub device-code flow. `ConnectProviderAccount` is bound to the model-provider preset table: a seventh preset would put Coder into the harness routing matrix and probe it for a `/models` endpoint it does not have.

## Proposed Changes

### What's Changing

- `AccountsService.ConnectCoderAccount` takes a deployment URL, a session token and a label, and answers the stored account's summary.
- The daemon asks the deployment who the token belongs to (`GET /api/v2/users/me`) before storing anything. The username becomes the account's subject.
- The connect is refused, with the reason, when the URL is not an `http(s)` URL with a host, carries userinfo, a query or a fragment, the token is empty, the deployment rejects the token, or the deployment cannot be reached.
- The Accounts screen shows a **Coder** group with a connect form: URL, token, label.
- Connecting the same deployment and user again replaces that account's token.

### What's Staying the Same

- No secret reaches the web: `AccountSummary` has no field for one.
- Rename and remove work as for every other account.
- The model-provider presets, routes and defaults are untouched; a Coder account never appears in a Provider field.

## Impact Analysis

### Technical Impact

- `accounts.proto` gains one RPC and two messages; the TypeScript client is regenerated.
- A new module `coder_account.rs` in `tddy-accounts`, not more lines in `service.rs`.
- The vault must be open to connect, as for every other account write.

### User Impact

- A new group and form on `#/accounts`. Nothing else changes until a later PR lists workspaces.

## Implementation Plan

1. Record codec and the deployment probe port.
2. The RPC, its refusals, wiring in the daemon.
3. The web form and group.

Tests are written first and run per touched package; the whole-workspace result is read from CI.

## Acceptance Criteria

- [ ] A valid URL and token store a `coder` account whose subject is the deployment's username
- [ ] The account lists under the Coder group with its label and subject, and no secret
- [ ] Each refusal names its cause and stores nothing
- [ ] Reconnecting the same deployment and user replaces the token and keeps the account id and label
- [ ] A locked or missing vault is reported as such, not as an empty list
- [ ] The Accounts screen connects a Coder account and shows a refusal beside the form

## References

### Affected Features (Complete List)

- **Provider accounts**: [provider-accounts.md](../provider-accounts.md) — a second kind of pasted-credential account, outside the model-provider presets
- **Accounts screen**: [accounts-screen.md](../../web/accounts-screen.md) — a Coder group with its own connect form
- **Credential sync**: [credential-sync.md](../credential-sync.md) — Coder records propagate like any other vault record

### Related Documentation

- Changeset: `docs/dev/1-WIP/2026-11-15-remote-shell-coder-account.md` on this branch
- Other PRs of this stack are named by branch in the changeset's `## Dependencies` and `## Successor PRs`
