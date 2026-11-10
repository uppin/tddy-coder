# Harness profiles

**Status:** shipped (#provider-route 8/9, [#669](https://github.com/uppin/tddy-coder/pull/669))

A **harness profile** is an agent definition that points a harness at a [provider account](provider-accounts.md)
and model. It is the **agent-definition** level of
[route precedence](provider-accounts.md#which-account-a-session-runs-on): an explicit session-level
choice beats it, and it beats the person's default.

## The file

Profiles live in `<tddyhome>/agents/*.yaml`, beside OpenAI-loop agent definitions. A document with a
top-level `harness:` key is a profile; anything else there is read as an ordinary definition.

```yaml
name: claude-meta-wonder
label: Claude Code · Meta Wonder   # optional; the name is shown when absent
harness: claude-cli
provider_account: meta             # an account label or id in the starter's vault
model: muse-wonder-1.0
```

`provider_account` may be omitted (it reads as empty), so a profile that only names a harness and
a model loads and is listed. A [project-home session](project-concept.md#project-home-session) set
up with a profile as its `agent` has the profile applied before its session type is checked, so only
a `claude-cli` profile can run main. A start through a profile whose `provider_account` is empty is
refused ("no credential vault…") instead of running on the harness's own vendor, so a profile names
the account it runs on.

Unknown fields are refused, so a profile cannot carry `base_url` or `api_key` — it is a harness on an
account, not an endpoint. Profiles are edited as YAML only; there is no UI editor.

| `harness:` | Starts |
|---|---|
| `claude-cli`, `opencode-cli`, `muse-cli`, `cursor-cli` | a CLI session of that type |
| `claude`, `claude-acp`, `codex`, `codex-acp`, `opencode`, `cursor` | a tool session with that agent |

The `harness:` is resolved through the host's coding agents. A CLI session type is resolved to the
agent it runs (`opencode-cli` → `opencode`, any `<id>-cli` of a terminal agent, a dropped-in one
included), and the session starts from that spec. Any other name is a tool session, and must be an
agent the host can run headless or over ACP.

Any other harness is refused (`INVALID_ARGUMENT`) when the profile is started — an unknown name is
never resolved to another agent. Whether the harness can
run on the account at all is the [capability matrix](provider-accounts.md#running-a-harness-on-an-account)'s call.

## In the agent list

`ListAgents` offers every profile after the configured agents and registry assistants, under its
`label` (or name); its id is the profile name. A row whose id names a spec in the host's coding-agent
registry (built-in or `<tddyhome>/tools/<id>.yaml`) is a `CODING_AGENT` row carrying that spec's `harness`
and `route_providers`; the create-session pane asks for provider routes only when `route_providers` is
non-empty. Every other row is `MODEL`.

A profile whose harness speaks ACP (`claude-acp`, `codex-acp`, `opencode`) is also offered by
`ListSubagents` as a **coding agent** a session can attach to its roster — see
[Session agent roster § Coding agents](session-agent-roster.md#coding-agents). Headless and PTY
harness profiles are not offered there.

## Starting a session with a profile

A `StartSession` whose `agent` names a profile is rewritten before the allowed-agents check and before
any forwarding to a peer daemon, so a forwarded start carries the resolved session type, model and
account id, never the profile name:

- The profile's account is the agent-definition route source; the request's own account or "built in"
  choice, if any, wins. Once the profile's account is chosen, the person's default is not read.
- The account is found in the starting person's vault by id, then by a unique label. None is
  `NOT_FOUND`; a label two accounts share is refused (`INVALID_ARGUMENT`) rather than guessed.
- The profile's model is used only when the request names none.
- Refusals name the profile and the account.

## The agent's environment

An agent process starts from an allow-listed environment, never the daemon's own. The daemon
computes it in one place from three parts:

- **The base set**, read from the daemon's environment: `PATH`, `HOME`, `USER`, `LOGNAME`, `SHELL`,
  `TERM`, `LANG`, `LC_*`, `TMPDIR`, `TZ`, `SSH_AUTH_SOCK`, `HTTP(S)_PROXY` / `NO_PROXY` (both cases)
  and `XDG_*`. It is built in; an operator replaces it whole with
  `<tddyhome>/tools/agent-env/base.yaml` (a subdirectory, because every `*.yaml` directly in `tools/`
  is a tool file).
- **The spec's `pass_env`**: names, or `PREFIX_*` patterns, of further daemon variables the agent
  needs. The built-in specs name their provider keys: `ANTHROPIC_API_KEY` (claude, claude-acp),
  `OPENAI_API_KEY` (codex, codex-acp), `CURSOR_API_KEY` (cursor), both of the first two (opencode);
  muse, warp and oz name none.
- **What the launch sets on purpose**: the spec's `env`, the provider route's variables, the
  session's own variables.

No other daemon variable reaches the agent. An entry matching `TDDY_*`, in `pass_env` or the base
set, is refused when the file loads, naming it. A user's Bash terminal is not an agent: it still
inherits the daemon's environment.

## Related

- [Provider accounts](provider-accounts.md)
- [Session agent roster](session-agent-roster.md) — the other kind of `<tddyhome>/agents/*.yaml` file.
- Implementation: [`tddy-discovery` roster](../../../packages/tddy-discovery/docs/roster-and-subagent-runtime.md),
  [`tddy-agent-launch` module layout](../../../packages/tddy-agent-launch/docs/module-layout.md#session-start)
