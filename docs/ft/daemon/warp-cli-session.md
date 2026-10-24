# Warp CLI Session

> **Related:** [OpenCode CLI session](opencode-cli-session.md) and [Muse Code CLI session](muse-cli-session.md)
> — the sibling session types it shares one start and resume path with; [Claude Code CLI session](claude-cli-session.md)
> and [Cursor Agent CLI session](cursor-cli-session.md). All five are registered through one **CLI harness
> descriptor** and share `CliSessionManager`, `StreamSessionTerminalIO` and the web terminal mount.
>
> **Also runs as an agent tab:** the harness can be opened as an extra terminal inside another session, beside that session's main agent in its worktree — see [Session terminal tabs](../web/session-terminal-tabs.md).

## Summary

A session type, `warp-cli`, that runs Warp's interactive Agent CLI (the `warp` binary, separate from
Warp.app) in a PTY in a daemon-cut git worktree. The operator picks **Warp** as the session type in the
web **Create session** pane and gets direct terminal access to Warp's agent. There is no model to
choose: Warp has no model flag, and the model is picked inside Warp with `/model`. The session runs on
the person's own Warp sign-in; it takes no [provider account](provider-accounts.md).

The session is **resumable**. Warp prints `warp --resume <TOKEN>` as it exits; the daemon reads that
line from the terminal's output and records the token beside the session, and **Resume** relaunches
`warp --resume <TOKEN>`, so the conversation continues where it stopped.

Only the interactive TUI is integrated — argv, no stream parsing. Warp's headless runner
(`oz agent run`) is not used.

## Start session

1. The **Create session** pane's session-type toggle offers **Warp** (`create-session-type-warp-cli`)
   after **Muse Code**. When selected the pane shows the branch fields with the **Create remote branch**
   toggle. It shows no **Model**, **Provider**, first prompt, sandbox, managed codebase, specialized
   agents or semantic index, and the request sends none of them. **Start New Session** is enabled once
   a project is chosen — no model is needed.
2. **Start New Session** issues `StartSession` with `session_type = "warp-cli"` and an empty `model`.
3. The daemon cuts a worktree for the session on `warp-cli/<short id>` when `new_branch_name` is empty
   (`BranchIntentPolicy::cli_harness("warp-cli")`), pushing it when **Create remote branch** is set.
4. The daemon spawns `warp` in the worktree, with no arguments.
5. `StartSessionResponse` returns `session_id`; LiveKit fields are empty.

Refused starts, each before anything is cut or spawned:

| Request | Status |
|---|---|
| `sandbox = true` | `FAILED_PRECONDITION` "the sandbox runner does not host warp-cli" — the sandbox runner hosts no Warp, and the refusal comes before any provider route is resolved |
| `managed_codebase = true` | `INVALID_ARGUMENT` — Warp has no workflow of its own for a managed codebase to drive |
| a provider account chosen (`provider_account_id`) | `INVALID_ARGUMENT` "Warp cannot be pointed at another provider; it always runs on its own vendor" — the refusal Cursor gets, checked before the empty-model refusal so it names Warp |

A person's **Default for** a harness has no Warp row, so a start that names no account never picks
one up.

## The binary

`TDDY_WARP_CLI` when set and non-blank, else `cli_harnesses.warp-cli.binary_path` in `daemon.yaml`,
else `warp` on `PATH`:

```yaml
cli_harnesses:
  warp-cli:
    binary_path: /opt/homebrew/bin/warp
```

## Models

`ListAgentModels("warp-cli")` answers an empty list with no default. The model is chosen inside Warp.

## The resume token

Warp prints its resume handle only as it exits, so the token is an output of the run:

- Every time the session's `warp` process exits, the daemon reads the tail of that terminal's output
  (the last 64 KiB), strips terminal escape sequences (CSI, OSC, DCS and the other string sequences,
  single-character escapes) and normalises `\r`, then takes the token of the **last**
  `warp --resume <TOKEN>` line. It records it in `.session.yaml` as `harness_resume_token`.
- The read happens once the process has exited **and** its output is complete, before the terminal is
  deregistered. A resume issued right after an exit waits for that read to finish, so it continues the
  conversation the run just printed.
- It happens on **every** exit, so a resumed run that prints a new token replaces the old one.
- An exit that printed no token — `warp` killed by a signal, crashed, or the daemon stopped first —
  leaves the recorded token unchanged. A session that has never exited cleanly has none.
- Nothing but `harness_resume_token` is written, and only when a token was found.

The reader runs for every CLI harness started through the shared OpenCode / Muse Code / Warp path;
only Warp prints the line, so the others never record a token.

## Resume

**Resume** relaunches `warp --resume <TOKEN>` in the session's worktree when a token is recorded. With
no recorded token it relaunches plain `warp` — a fresh conversation in the same worktree — and never
passes a bare `--resume`. Resume marks the session active again. A resume whose worktree is gone is
refused with `FAILED_PRECONDITION`.

The conversation continues only if Warp was let exit normally (`/exit`, or Ctrl+C twice) at least
once; a session whose `warp` was killed resumes into a fresh conversation in the same worktree.

## Terminal access, persistence and delete

As for [OpenCode](opencode-cli-session.md#terminal-access): the PTY is served over the daemon's own
connection and `ConnectSession` names no terminal room. `.session.yaml` holds `session_type: warp-cli`,
`repo_path` (the worktree), `pid`, `status` and, after the first exit that printed one,
`harness_resume_token`. It holds no `model`. `ListSessions` shows `agent` as `warp-cli`. There are no
activity-status hooks.

Context files served to the session's agent: `AGENTS.md`, `.agents/**` and `WARP.md`, on top of the
shared base.

**Delete** stops the process and removes the session directory and its daemon-managed worktree (a
client-supplied checkout is kept).

## Unconfirmed

The flags, the exit line and the context files come from Warp's published reference
(docs.warp.dev, 2026-10); no real `warp` binary was available to check them against (Warp.app bundles
only `oz`). Tests pin them as documented. The exit-line format is the riskiest fact, because the token
reader depends on it.

## Out of scope

- Sandboxed Warp (a jailed `warp`).
- A Warp provider account (a `warp` preset delivering `WARP_API_KEY`): connecting an account probes an
  HTTP model endpoint, and a Warp key has none.
- Model selection, a first prompt, and `--auto-approve`.
- A headless `oz agent run` tool-session backend.

## References

- Warp Agent CLI reference: https://docs.warp.dev/agents/cli/reference/
- Warp Agent CLI overview: https://docs.warp.dev/agents/cli/
