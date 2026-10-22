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
admits, **`claude-cli`**, **`opencode-cli`** and **`muse-cli`** started unsandboxed are run on an account today
([Claude Code CLI session § Running on a provider account](claude-cli-session.md#running-on-a-provider-account),
[OpenCode CLI session § Running on a provider account](opencode-cli-session.md#running-on-a-provider-account),
[Muse Code CLI session § Running on a Meta account](muse-cli-session.md#running-on-a-meta-account));
so is a **tool session** whose agent is `claude`, `claude-acp`, `codex`, `codex-acp` or `opencode` (below). Any
other harness asked to run on an account is refused as not routable in this build.

### Tool sessions

A tool session (headless `tddy-coder`) started or resumed with an account gets its route in a file,
because the spawned process's environment carries no credential:

- The daemon resolves the route for the session's agent before anything is created, writes it to
  `<session_dir>/provider-route.json` — mode `0600`, owned by the session's OS user — and passes
  `tddy-coder --provider-route-path <file>`. No key is on the command line or in the spawn
  environment. Resume re-resolves the account and model the start recorded.
- `tddy-coder` refuses a route file with any group or other permission bit, or one that is not a
  route; the session does not start on the vendor instead.
- Claude agents get the same Anthropic route as a claude-cli session. Codex agents get the account
  declared as a Responses provider (`tddy-route`) through `-c` overrides and the model through `-m`
  (`codex exec`) or `-c model=` (`codex-acp`); the key is in the child's `TDDY_ROUTE_API_KEY`.
- The OpenCode agent (`opencode acp`) gets the same generated `opencode.json` as an opencode-cli
  session, written owner-only into a directory the agent's worker removes when it ends and named by
  `OPENCODE_CONFIG`, with the key in `TDDY_OPENCODE_PROVIDER_KEY`. The route's `--model` is not passed:
  the configuration selects the model ([OpenCode ACP backend](../coder/opencode-acp-backend.md)).
- An account with no agent, a `cursor` agent, the `stub` or a specialized agent definition is
  refused with the reason. A tool session started from Telegram cannot choose an account
  (`docs/dev/todo/2026-10-22-telegram-tool-sessions-choose-no-provider-account.md`).
- The Codex `-c` overrides follow Codex's documented `config.toml` keys; they were checked against
  `codex-acp --help`, and every test runs a stub binary rather than a real `codex`.

### `ListProviderRoutes`

`accounts.AccountsService/ListProviderRoutes(session_token, harness)` answers, for one harness, the
caller's provider accounts in vault order, each with `supported` and — when not — the `reason`, in the
matrix's words. `harness_routable` is false for a harness no account can run — Cursor, the one
harness this build has no route for; every account is then
unsupported and `harness_reason` says why, once. `harness` is a tool-session agent id or a CLI
session type; an id that names no harness is answered the same way, `harness_routable` false with
"unknown harness". The web
never re-derives the matrix; a daemon that has not wired it answers `FAILED_PRECONDITION` rather than
guessing.

### Which account a session runs on

A session's account is chosen at three levels, highest first: the **session** (the operator's choice
on start), the **agent definition** it was started with, and the **person's default** for the harness.
An explicit "built in" choice at a level overrides any account below it. The session and default
levels are fed; nothing fills the agent-definition level yet.

### A person's default per harness

A person marks a provider account as their **default** for one or more harnesses ("Claude CLI →
Meta"). The default is **per person**, kept in the account's own vault record (metadata
`default_for`, the harness ids comma-separated), so it survives a daemon restart and travels with the
account under [credential sync](credential-sync.md). There is no daemon-wide default: the daemon holds
no key of its own to run one on.

`accounts.AccountsService/SetProviderAccountDefaults(session_token, account_id, harnesses[])` makes the
account the default for exactly those harnesses:

- A harness has at most one default per person. Naming it here clears it from whichever account held
  it; the response's `changed` lists every account whose defaults moved (the one named and any that
  lost a harness to it).
- A harness the account cannot drive, per the matrix above, is refused `INVALID_ARGUMENT` with the
  matrix's reason (a Baseten account as the Claude CLI default: "needs Anthropic Messages"); nothing is
  changed.
- An empty list clears every default the account holds. An id the vault holds no provider account
  under is `NOT_FOUND`.
- `AccountSummary.default_for` lists the harnesses an account is the default for.

At start, a **claude-cli**, **opencode-cli** or **muse-cli** session that names neither an account nor "built in"
runs on the owner's default for its harness. An explicit choice — an account, or "Anthropic (built
in)" — wins and does not read the default at all. While the owner's vault is **locked**, a start that
names nothing is refused (`FAILED_PRECONDITION`) rather than run on the vendor the default may have
been chosen to avoid; an absent or uninitialized vault has no default. A sandboxed claude-cli start
whose default is an account is refused like one that names the account, since a sandbox cannot run on
an account yet. Tool sessions decide from the session-level choice only
([backlog](../../dev/todo/2026-10-22-tool-session-starts-ignore-the-provider-default.md)).

## Out of scope

- Editing a stored key in place — remove the account and connect it again.
- Moving registry providers into the vault.

## Related

- [Account linking](account-linking.md) — the other way an account enters the vault.
- [Claude Code CLI session](claude-cli-session.md#running-on-a-provider-account) — the harness that runs on an account.
- Implementation: [tddy-accounts provider accounts](../../../packages/tddy-accounts/docs/provider-accounts.md)
