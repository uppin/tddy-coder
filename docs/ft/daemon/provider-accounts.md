# Provider accounts

A signed-in person connects a **model-provider account** by pasting an API key: Meta Model API,
Fireworks, Baseten, Modal, or a generic OpenAI-compatible or Anthropic-compatible endpoint. The key
is sealed in that person's credential vault beside their GitHub accounts; the account appears on the
[Accounts screen](../web/accounts-screen.md) grouped by provider, can be relabelled and removed like
any other account, and lists the models its endpoint serves.

Provider-account records are ordinary vault records, so they propagate like any other when
[credential sync](credential-sync.md) is configured. They are separate from the model registry
(`models.db`), which is a per-daemon, plaintext store driving subagents and the Models chat.

## Presets

A fixed table of six presets names the wire protocols each serves and the root for each:

| Preset | Anthropic Messages | OpenAI Responses | OpenAI Chat Completions |
|---|---|---|---|
| `meta` | `https://api.meta.ai` | `https://api.meta.ai/v1` | `https://api.meta.ai/v1` |
| `fireworks` | `https://api.fireworks.ai/inference` | — | `https://api.fireworks.ai/inference/v1` |
| `baseten` | — | — | `https://inference.baseten.co/v1` |
| `modal` | — | — | operator-supplied (per-deployment URL) |
| `openai-compatible` | — | operator-supplied | operator-supplied |
| `anthropic-compatible` | operator-supplied | — | — |

One OpenAI root serves both OpenAI protocols. The connect request is refused — never silently
corrected — when:

- the preset is unknown, or the key is empty;
- a preset with an operator-supplied root is given none;
- a fixed root is given an override;
- a root is given for a protocol the preset does not serve (an Anthropic root for Modal, an OpenAI
  root for `anthropic-compatible`);
- a supplied root is not an `http(s)` URL with a host, or carries userinfo, a query or a fragment.

## Connecting

`ConnectProviderAccount` validates the request, checks the vault is open (a locked or uninitialized
vault refuses with the screen's usual messages, before the key leaves the daemon), then **probes**
the endpoint's model list with the key. A key the endpoint rejects is refused with the provider's
reason and nothing is stored. A key that lists models is stored as `provider = <preset id>`,
`secret = key`, with the endpoints and the probe time in the record's metadata.

The account's id is the preset and the label's slug (`fireworks-prod`; a label that already names the preset is not prefixed twice, so `Meta hackathon` is `meta-hackathon`), numbered `-2`, `-3`, … when
taken. Two connects asking for the same id at the same time get two ids; neither replaces the other.

## Listing models

`ListProviderAccountModels` asks the account's endpoint, with its key, which models it serves:
`GET <openai root>/models` with a bearer key, or `GET <anthropic root>/v1/models` with `x-api-key`
for an Anthropic-only preset. A root that already ends in `/v1` is not given a second one.

## What is never shown

No RPC returns the key. `AccountSummary` carries the preset and the wire protocols the endpoint
serves (`anthropic-messages`, `openai-responses`, `openai-chat`), from which the harnesses that can
run on the account follow.

## Running a harness on an account

A session's harness can run on a provider account instead of its vendor. Which harness can run on
which account is one **capability matrix**, held by the daemon for every harness:

| Harness (agent id / session type) | Runs on an account that |
|---|---|
| Claude Code — `claude`, `claude-acp`, `claude-cli` | serves Anthropic Messages (`meta`, `fireworks`, `anthropic-compatible`) |
| Codex — `codex`, `codex-acp` | serves OpenAI Responses |
| OpenCode — `opencode`, `opencode-cli` | serves OpenAI Responses or OpenAI Chat Completions |
| Muse Code — `muse-cli` | is a `meta` account |
| Cursor — `cursor`, `cursor-cli` | — never: Cursor's agent cannot be pointed at another provider |

An unsupported pair is refused with the reason, never silently ignored. Of the harnesses the matrix
admits, only **`claude-cli`** started unsandboxed is run on an account today
([Claude Code CLI session § Running on a provider account](claude-cli-session.md#running-on-a-provider-account));
any other harness asked to run on an account is refused as not routable in this build.

### `ListProviderRoutes`

`accounts.AccountsService/ListProviderRoutes(session_token, harness)` answers, for one harness, the
caller's provider accounts in vault order, each with `supported` and — when not — the `reason`, in the
matrix's words. `harness_routable` is false for a harness no account can run — Cursor, and every
harness whose route this build does not have yet (Codex, OpenCode, Muse Code); every account is then
unsupported and `harness_reason` says why, once. `harness` is a tool-session agent id or a CLI
session type; an id that names no harness is answered the same way, `harness_routable` false with
"unknown harness". The web
never re-derives the matrix; a daemon that has not wired it answers `FAILED_PRECONDITION` rather than
guessing.

### Which account a session runs on

A session's account is chosen at three levels, highest first: the **session** (the operator's choice
on start), the **agent definition** it was started with, and the **person's default** for the harness.
An explicit "built in" choice at a level overrides any account below it. Only the session level is
fed today.

## Out of scope

- Editing a stored key in place — remove the account and connect it again.
- Moving registry providers into the vault.

## Related

- [Account linking](account-linking.md) — the other way an account enters the vault.
- [Claude Code CLI session](claude-cli-session.md#running-on-a-provider-account) — the harness that runs on an account.
- Implementation: [tddy-accounts provider accounts](../../../packages/tddy-accounts/docs/provider-accounts.md)
