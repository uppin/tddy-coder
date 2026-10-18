# Provider accounts: connect Meta, Fireworks, Baseten, Modal and compatible APIs by key - PRD

**Date**: 2026-10-18
**PRD Type**: Enhancement
**Stack**: #provider-route 1/9 (base `master`)

## Affected Features

- **Primary Feature**: [Accounts screen](../../web/accounts-screen.md) — gains a "Connect API key" path beside
  the GitHub device link.
- **Related Feature**: [Account linking](../account-linking.md) — a second way an account enters the vault,
  one that never signs anyone in either.
- **Related Feature**: [Credential sync](../credential-sync.md) — provider-account records are ordinary vault
  records and propagate like any other when `keyring.group_secret` is set.

## Summary

A signed-in person connects a **model-provider account** by pasting an API key: Meta Model API,
Fireworks, Baseten, Modal, or a generic OpenAI-compatible or Anthropic-compatible endpoint. The key is
sealed in that person's credential vault; the account appears on the Accounts screen grouped by
provider, can be relabelled and removed like a GitHub account, and lists the models its endpoint
serves.

## Background

Later nodes of this stack let Claude Code, Codex, OpenCode and Muse Code run against such an account.
Today the vault holds only GitHub accounts and can only add one through the device-code link, and no
other place stores a per-person provider key that a harness could use. The model registry's providers
(`models.db`) are a separate, per-daemon, plaintext store that drives subagents and the Models chat;
the developer chose the vault for harness accounts (sealed, per person, syncable).

## Proposed Changes

### What's Changing
- **Provider presets** — a fixed table of six presets, each naming the wire protocols it serves and the
  base URL for each:

  | Preset | Anthropic Messages | OpenAI Responses | OpenAI Chat Completions |
  |---|---|---|---|
  | `meta` | `https://api.meta.ai` | `https://api.meta.ai/v1` | `https://api.meta.ai/v1` |
  | `fireworks` | `https://api.fireworks.ai/inference` | — | `https://api.fireworks.ai/inference/v1` |
  | `baseten` | — | — | `https://inference.baseten.co/v1` |
  | `modal` | — | — | operator-supplied (per-deployment URL) |
  | `openai-compatible` | — | operator-supplied | operator-supplied |
  | `anthropic-compatible` | operator-supplied | — | — |

  A preset with an operator-supplied URL requires one; a fixed one may not be overridden in this node.
- **`ConnectProviderAccount`** RPC on `AccountsService`: preset, label, API key (inbound only),
  optional base URL(s). Validates the preset and URLs, probes the endpoint's model list with the key,
  and stores a vault record — `provider = <preset id>`, `secret = key`, `metadata = endpoints +
  probe time`. A failed probe refuses with the provider's reason; nothing is stored.
- **`ListProviderAccountModels`** RPC: the models an account's endpoint serves (`GET …/models`
  against the OpenAI-compatible root, or the Anthropic `/v1/models` for an Anthropic-only preset).
- `AccountSummary` carries the account's preset and the wire protocols it serves — never the key.
- Accounts screen: "Connect API key" opens a form (preset select, label, key, URL when the preset needs
  one); the new account appears in its provider group.

### What's Staying the Same
- GitHub device-link flow, project account assignment, vault states and their four on-screen outcomes.
- The model registry (`models.db`) and its providers are untouched.
- No RPC ever returns a secret.

## Impact Analysis

### Technical Impact
`accounts.proto` (+2 RPCs, summary fields), `tddy-accounts` (presets, provider-account record codec,
service methods, store port `put`), daemon wiring (model probe HTTP client), `tddy-web` Accounts screen
and its RPC fake. No new external dependency (HTTP via the crates already used by the registry).

### User Impact
New capability only. A locked or uninitialized vault refuses the connect with the same messages the
screen already shows.

## Implementation Plan
1. Preset table + record codec (`ProviderAccount` ⇄ `CredentialRecord`).
2. `ConnectProviderAccount` / `ListProviderAccountModels` in the service, probe through a port.
3. Daemon wiring of the probe; web form, list rendering, model list.

## Acceptance Criteria
- [ ] Connecting a Meta account with a working key stores a sealed record and lists it under "Meta".
- [ ] A key the endpoint rejects is refused with the provider's reason and nothing is stored.
- [ ] A Modal / generic preset without a base URL is refused; a fixed preset with one is refused.
- [ ] `ListProviderAccountModels` returns the endpoint's model ids for that account.
- [ ] `ListAccounts` shows the preset and protocols and never the key.
- [ ] A locked vault refuses the connect as "vault locked".
- [ ] The Accounts screen connects, lists, relabels and removes a provider account.

## Out of Scope
- Using an account for any session (next nodes).
- Editing a stored key in place (remove and reconnect).
- Moving registry providers into the vault.

## References
- [Accounts screen](../../web/accounts-screen.md), [Account linking](../account-linking.md),
  [Credential sync](../credential-sync.md), `packages/tddy-credentials/docs/credential-store.md`.
- Meta Model API coding-agent guide: https://dev.meta.ai/docs/coding-agents ;
  Fireworks Anthropic compatibility: https://docs.fireworks.ai/tools-sdks/anthropic-compatibility
