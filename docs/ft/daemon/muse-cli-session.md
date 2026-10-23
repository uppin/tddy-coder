# Muse Code CLI Session

> **Related:** [OpenCode CLI session](opencode-cli-session.md) — the sibling session type it shares one
> start and resume path with; [Claude Code CLI session](claude-cli-session.md) and
> [Cursor Agent CLI session](cursor-cli-session.md). All four are registered through one **CLI harness
> descriptor** and share `CliSessionManager`, `StreamSessionTerminalIO` and the web terminal mount.

## Summary

A session type, `muse-cli`, that runs Meta's Muse Code TUI (`muse` binary) in a PTY in a daemon-cut git
worktree. The operator picks **Muse Code** as the session type in the web **Create session** pane, picks
a Muse model (or a Meta [provider account](provider-accounts.md) and one of its models), and gets direct
terminal access. The session is resumable. On a Meta account Muse Code is handed the account's key;
without one it uses the person's own Muse Code sign-in.

Only the interactive TUI is integrated — argv and environment, no stream parsing. Muse Code's headless
protocols (`muse exec --json`, `muse serve`) are undocumented, so there is no Muse tool-session agent.

## Start session

1. The **Create session** pane's session-type toggle offers **Muse Code** (`create-session-type-muse-cli`)
   after **OpenCode**. When selected the pane shows the **Provider** field and a **Model** select
   populated from `ListAgentModels("muse-cli")`, plus the branch fields with the **Create remote branch**
   toggle. It offers no first prompt, sandbox, managed codebase, specialized agents or semantic index,
   and the request sends none of them.
2. **Start New Session** issues `StartSession` with `session_type = "muse-cli"`, the chosen `model`
   (may be empty), and `provider_account_id` / `provider_built_in` as chosen. A start that chooses
   neither runs on the person's [default for Muse Code](provider-accounts.md#a-persons-default-per-harness),
   if they set one.
3. A chosen account is resolved **before anything is cut or spawned** (see
   [Running on a Meta account](#running-on-a-meta-account)).
4. The daemon cuts a worktree for the session on `muse-cli/<short id>` when `new_branch_name` is empty
   (`BranchIntentPolicy::cli_harness("muse-cli")`), pushing it when **Create remote branch** is set.
5. The daemon spawns `muse [--model <model>] [<first prompt>]` in the worktree. `--model` is passed only
   when a model was chosen, and a blank first prompt is none.
6. `StartSessionResponse` returns `session_id`; LiveKit fields are empty.

Refused starts:

| Request | Status |
|---|---|
| `sandbox = true` with no Meta account (none chosen, no default, or **Built in**) | `FAILED_PRECONDITION` "a sandboxed muse-cli session needs a provider account" — see [Sandbox mode](#sandbox-mode) |
| `sandbox = true` with a non-blank first prompt | `FAILED_PRECONDITION` — the jail hosts Muse Code without a first prompt |
| `managed_codebase = true` | `INVALID_ARGUMENT` — Muse Code has no workflow of its own for a managed codebase to drive |

## Sandbox mode

The web **Create session** pane does not offer a sandbox toggle for Muse Code; the mode is reached through
`StartSession` directly.

`sandbox = true` runs Muse Code in a jail (macOS Seatbelt, Linux userns + cgroups). The jail hosts
`tddy-sandbox-runner --agent-kind muse`, which runs `muse --model <model>` in its PTY and relays it
over the daemon's own connection. The worktree is the daemon's. The jail's `$HOME` is the session's
own scratch directory. Muse Code is a native binary, so its jail policy is the interactive TUI policy
**without** run-time code generation.

A sandboxed Muse Code session **needs a Meta account**: the person's own Muse Code setup is not inside
the jail. Without one the start is refused before anything is cut or spawned. On the account,
`META_API_KEY` reaches the agent child through the jail's secret-file channel (`TDDY_SECRET_<NAME>`),
never the jail's environment map or argv. The binary is resolved to an absolute path on the daemon's
`PATH` first, and a binary that is not there is refused.

The jail's runner hosts **no first prompt**, so a sandboxed start with a non-blank `initial_prompt` is
refused (`FAILED_PRECONDITION`). A resume relaunches the jail with `--resume` (`muse --continue`) on
the account the start recorded, re-resolved from the owner's vault. It never resumes on the host's
PTY.

## The binary

`TDDY_MUSE_CLI` when set and non-blank, else `cli_harnesses.muse-cli.binary_path` in `daemon.yaml`,
else `muse` on `PATH`:

```yaml
cli_harnesses:
  muse-cli:
    binary_path: /opt/muse/bin/muse
```

## Models

`ListAgentModels("muse-cli")` answers a curated list: **Muse Spark 1.3** (`muse-spark-1.3`, the default)
and **Muse Wonder 1.0** (`muse-wonder-1.0`). With a provider account chosen, the form lists the
account's models instead.

## Terminal access, persistence and delete

As for [OpenCode](opencode-cli-session.md#terminal-access): the PTY is served over the daemon's own
connection and `ConnectSession` names no terminal room. `.session.yaml` holds `session_type: muse-cli`,
`model`, `repo_path` (the worktree), `pid` and `status: active`; `ListSessions` shows `agent` as
`muse-cli`. There are no activity-status hooks.

Context files served to the session's agent: `AGENTS.md`, `.agents/**` and `.muse/**`, on top of the
shared base.

**Delete** stops the process and removes the session directory and its daemon-managed worktree (a
client-supplied checkout is kept).

## Resume

**Resume** relaunches `muse --continue` in the session's worktree, with neither model nor prompt. A
session owns its worktree, so the worktree's last Muse Code session is this one. The provider route is
re-resolved from the owner's vault as it stands now. A resume whose worktree is gone is refused with
`FAILED_PRECONDITION`.

## Running on a Meta account

Muse Code talks only to Meta, so it runs on a `meta` [provider account](provider-accounts.md) and no
other: any other preset is refused `INVALID_ARGUMENT` with the reason (a Fireworks account: "… does
not accept a fireworks account; it runs on Meta only"). The route is the account's key alone,
delivered in the child's `META_API_KEY` — no file, no endpoint, no extra argument — so the launch keeps the session's own
`--model`. The account id and model are recorded beside the session (never the key), so a resume
re-applies them. A start that chooses no account (and has no default) injects no key.

Refusals otherwise are those of
[Claude Code CLI § Running on a provider account](claude-cli-session.md#running-on-a-provider-account)
(locked vault, unknown account, empty model). A start that chose an account never runs on the person's
own sign-in instead.

## Unconfirmed

The key variable (`META_API_KEY`), `--model`, `--continue` and the `.muse/` tree come from secondary
sources; no real `muse` was available to check them against. Tests pin them as written.

## Out of scope

- Sandboxed Muse Code (`#provider-route` 9/9).
- A headless `muse exec --json` tool-session backend.
