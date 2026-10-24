# OpenCode CLI Session

> **Related:** [Claude Code CLI session](claude-cli-session.md), [Cursor Agent CLI session](cursor-cli-session.md), [Muse Code CLI session](muse-cli-session.md) and [Warp CLI session](warp-cli-session.md) — sibling session types; all five share `CliSessionManager`, `StreamSessionTerminalIO` and the web terminal mount, and all five are registered through one **CLI harness descriptor**. OpenCode, Muse Code and Warp share one start and resume path.
>
> **Also runs as an agent tab:** the harness can be opened as an extra terminal inside another session, beside that session's main agent in its worktree — see [Session terminal tabs](../web/session-terminal-tabs.md).

## Summary

A session type, `opencode-cli`, that runs OpenCode's interactive TUI (`opencode` binary) in a PTY in a
daemon-cut git worktree. The operator picks **OpenCode** as the session type in the web **Create
session** pane, picks a model from OpenCode's own list (or a provider account and one of its models),
and gets direct terminal access. The session is resumable. On a [provider account](provider-accounts.md)
OpenCode is given a generated configuration naming that account's endpoint, key and model; without
one it runs on the person's own OpenCode setup.

OpenCode is the provider-agnostic harness: it runs Fireworks, Baseten, Modal and any generic endpoint
that serves OpenAI Responses or OpenAI Chat Completions.

## Start session

1. The **Create session** pane's session-type toggle offers **OpenCode** (`create-session-type-opencode-cli`)
   beside **Managed session**, **Claude CLI** and **Cursor CLI**. When selected the pane shows the
   **Provider** field and a **Model** select populated from `ListAgentModels("opencode-cli")`, plus the
   branch fields with the **Create remote branch** toggle. It offers no first prompt, sandbox, managed
   codebase, specialized agents or semantic index, and the request sends none of them.
2. **Start New Session** issues `StartSession` with `session_type = "opencode-cli"`, the chosen `model`
   (may be empty), and `provider_account_id` / `provider_built_in` as chosen.
3. A chosen account is resolved **before anything is cut or spawned** (see
   [Running on a provider account](#running-on-a-provider-account)).
4. The daemon cuts a worktree for the session on `opencode-cli/<short id>` when `new_branch_name` is
   empty (`BranchIntentPolicy::cli_harness("opencode-cli")`), pushing it when **Create remote branch** is set.
5. The daemon spawns `opencode [--model <provider/model>] [--prompt <first prompt>]` in the worktree.
   `--model` is passed only when a model was chosen — an empty model leaves the choice to OpenCode.
6. `StartSessionResponse` returns `session_id`; LiveKit fields are empty.

Refused starts:

| Request | Status |
|---|---|
| `sandbox = true` with no provider account (none chosen, no default, or **Built in**) | `FAILED_PRECONDITION` "a sandboxed opencode-cli session needs a provider account" — see [Sandbox mode](#sandbox-mode) |
| `sandbox = true` with a non-blank first prompt | `FAILED_PRECONDITION` — the jail hosts OpenCode without a first prompt |
| `managed_codebase = true` | `INVALID_ARGUMENT` — OpenCode has no workflow of its own for a managed codebase to drive |

## Sandbox mode

The web **Create session** pane does not offer a sandbox toggle for OpenCode; the mode is reached through
`StartSession` directly.

`sandbox = true` runs OpenCode in a jail (macOS Seatbelt, Linux userns + cgroups). The jail hosts
`tddy-sandbox-runner --agent-kind opencode`, which runs `opencode --model <model>` in its PTY and
relays the PTY over the daemon's own connection, as a sandboxed Cursor session does. The worktree is
the daemon's, cut exactly as for an unsandboxed start. The jail's `$HOME` is the session's own
scratch directory, so a resume finds OpenCode's session store where it left it.

A sandboxed OpenCode session **needs a provider account**. The person's own OpenCode setup is not
inside the jail, so without an account the harness would start with no credentials and could not
authenticate. The start is refused before anything is cut or spawned.

On the account:

- the generated configuration is written owner-only (`0700` directory, `0600` file) into the jail's
  scratch, and `OPENCODE_CONFIG` names it;
- the key reaches the agent child through the jail's secret-file channel (`TDDY_SECRET_<NAME>`), never
  the jail's environment map or argv;
- the runner's `--model` is the route's (`tddy-route/<model>`).

The binary is resolved to an absolute path on the daemon's `PATH` before the jail is built, because
the sandbox profile grants reads to it by path. A binary that is not there is refused with
`FAILED_PRECONDITION`.

The jail's runner hosts **no first prompt**, so a sandboxed start with a non-blank `initial_prompt` is
refused (`FAILED_PRECONDITION`) rather than dropping the prompt silently. A blank prompt is no prompt.

A resume of a sandboxed session relaunches its jail with `--resume` (`opencode --continue`), on the
account the start recorded and re-resolved from the owner's vault as it stands now. It never resumes
on the host's PTY. A resume whose account is gone, or whose session has no account record, is refused
with the start's statuses.

## The binary

The binary is resolved as: `TDDY_OPENCODE_CLI` when set and non-blank, else
`cli_harnesses.opencode-cli.binary_path` in `daemon.yaml`, else `opencode` on `PATH`:

```yaml
cli_harnesses:
  opencode-cli:
    binary_path: /home/dev/.opencode/bin/opencode
```

The same resolution is used for the start, the resume and the model-list probe.

## Models

`ListAgentModels("opencode-cli")` runs `opencode models` as the requesting OS user and reads one
`provider/model` per line (blank lines skipped, the first is the default). When the resolved binary is
an absolute path it is forwarded to `tddy-tools list-models` as `--opencode-cli-path`; a bare name is
looked up on `PATH`. A binary that cannot be run or exits non-zero fails the listing rather than
answering an empty list. With a provider account chosen, the form lists the account's models instead
([provider accounts](provider-accounts.md)).

## Terminal access

The web UI mounts `GhosttyTerminalSession` on `StreamSessionTerminalIO`, as for claude-cli and
cursor-cli. `ConnectSession` answers with empty LiveKit coordinates: the PTY is served over the
daemon's own connection, never a terminal room of its own. The daemon facilitates the session (it runs
its agent here), so the session has a session room like the other CLI types.

## Session persistence

`~/.tddy/sessions/<session-id>/.session.yaml` holds `session_type: opencode-cli`, `model` (omitted when
none was chosen), `repo_path` (the worktree), `pid` and `status: active`. `ListSessions` shows `agent`
as `opencode-cli` and `model` as the chosen model. There are no activity-status hooks: OpenCode reports
no `ReportSessionStatus`, and no transcript is tailed.

Context files served to the session's agent: `AGENTS.md`, `.agents/**`, `opencode.json` and
`.opencode/**`, on top of the shared base.

## Resume and delete

- **Resume** relaunches `opencode --continue` in the session's worktree, with neither model nor prompt.
  tddy never learns OpenCode's own session id; a session owns its worktree, so the worktree's last
  OpenCode session is this one. The session's provider route is re-resolved from the owner's vault as
  it stands now and its configuration regenerated. A resume whose worktree is gone is refused with
  `FAILED_PRECONDITION` — there is nothing for `--continue` to continue.
- **Delete** stops the process and removes the session directory and its daemon-managed worktree (a
  client-supplied checkout is kept).

## Running on a provider account

An opencode-cli session can run on any provider account whose endpoint serves OpenAI Responses or
OpenAI Chat Completions. The start resolves the account first, then writes, owner-only (`0600`), into
the session directory an `opencode.json` that declares the account as provider `tddy-route` and
selects the model on it:

| Account serves | Provider package | Model entry |
|---|---|---|
| OpenAI Responses (preferred) | `@ai-sdk/openai`, `baseURL` = the Responses root | `options.include = ["reasoning.encrypted_content"]`, so reasoning is carried across turns |
| only OpenAI Chat Completions | `@ai-sdk/openai-compatible`, `baseURL` = the Chat root | — |

The key is referenced in the file as `{env:TDDY_OPENCODE_PROVIDER_KEY}` and never written into it. The
child is launched with `OPENCODE_CONFIG=<session dir>/opencode.json`, `TDDY_OPENCODE_PROVIDER_KEY=<key>`
and `--model tddy-route/<model>`. The account id and model are recorded beside the session (never the
key), so a resume re-applies them. A start that chooses no account writes no configuration and runs on
the person's own OpenCode setup.

Refusals are those of [Claude Code CLI § Running on a provider account](claude-cli-session.md#running-on-a-provider-account)
(locked vault, unknown account, empty model), plus an account that serves neither OpenAI protocol:
`INVALID_ARGUMENT`, naming "OpenAI Responses or Chat Completions". A start that chose an account never
runs on OpenCode's own setup instead.

## Out of scope

- Sandboxed OpenCode (`#provider-route` 9/9).
- Activity-status hooks and transcript tailing.

## References

- [OpenCode CLI](https://opencode.ai/docs/cli/), [OpenCode config](https://opencode.ai/docs/config/)
- [Provider accounts](provider-accounts.md)
- [OpenCode ACP backend](../coder/opencode-acp-backend.md) — OpenCode as a headless tool-session agent (`--agent opencode`)
- [Terminal sessions](terminal-sessions.md)
