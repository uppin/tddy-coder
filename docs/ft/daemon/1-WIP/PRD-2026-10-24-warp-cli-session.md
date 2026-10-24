# Warp CLI session (`warp-cli`) — PRD

**Date**: 2026-10-24
**PRD Type**: Feature Enhancement
**Stack**: `#warp-agent` 1/2 — branch `feature/warp-agent/warp-cli-session`. The successor node
(`feature/warp-agent/agent-terminal-tabs`) carries its own PRD.

## Affected Features

- **Primary (new)**: `docs/ft/daemon/warp-cli-session.md`, created at wrap and modelled on
  [Muse Code CLI session](../muse-cli-session.md). It describes a session type that runs Warp's
  interactive agent TUI.
- [Terminal sessions](../terminal-sessions.md) — `warp-cli` joins the host-served session types (claude-cli / cursor-cli / opencode-cli / muse-cli / workspace) wherever the document lists them.
- [Provider accounts](../provider-accounts.md) — the per-harness table gains a `warp-cli` row: **accepts no account**. A Warp session runs on the person's own Warp sign-in.
- [Session drawer § Create session](../../web/session-drawer.md) — the session-type toggle offers **Warp**, with no model field.
- [OpenCode CLI session](../opencode-cli-session.md) and [Muse Code CLI session](../muse-cli-session.md) — their "Related" lines name the fifth sibling.

## Summary

A session type, `warp-cli`, runs Warp's interactive Agent CLI (the `warp` binary) in a PTY, in a
git worktree the daemon cuts for the session. The operator picks **Warp** in the web **Create
session** pane and gets direct terminal access to Warp's agent. There is no model to choose: Warp
has no model flag, and the model is picked inside Warp with `/model`. The session runs on the
person's own Warp sign-in.

The session is **resumable**. When `warp` exits it prints `warp --resume <TOKEN>`. The daemon reads
that line from the terminal's output and records the token beside the session. **Resume** then
relaunches `warp --resume <TOKEN>`, so the conversation continues where it stopped.

## Background

The repo integrates four interactive CLI harnesses (Claude Code, Cursor Agent, OpenCode, Muse Code)
through one **CLI harness descriptor** registry (`packages/tddy-agent-backend/src/cli_harness.rs`).
Since #667 a further harness is one more descriptor, not another arm in every match. Warp has shipped
an Agent CLI, `warp`, separate from Warp.app (Homebrew cask `warp-agent-cli`). Developers who use
Warp's agent want it as a first-class session beside the others.

Facts about `warp` that shape this PRD, from Warp's own reference (2026-10):

| Fact | Consequence |
|---|---|
| `warp` with no flags starts an interactive session in the current directory | The daemon spawns `warp` with the worktree as its cwd |
| No flag for a model, and none for a first prompt | No model field; the first prompt is not offered |
| `--resume CONVERSATION_TOKEN` reopens a conversation, and the token is printed **only on exit** | The token is read from the PTY output after exit and recorded |
| Context: `AGENTS.md` or `WARP.md`, from repo root down; skills in `.agents/skills/` | Context files served to the agent: `AGENTS.md`, `.agents/**`, `WARP.md` |
| Auth: interactive browser login, or `WARP_API_KEY` | Native login only in this PRD; a Warp provider account is deferred |

## Proposed Changes

### What's changing

**Start session**

1. The **Create session** pane's session-type toggle offers **Warp** (`create-session-type-warp-cli`)
   after **Muse Code**. When it is selected, the pane shows the branch fields with the **Create remote
   branch** toggle. It shows no **Model**, **Provider**, first prompt, sandbox, managed codebase,
   specialized agents or semantic index, and the request sends none of them. **Start New Session** is
   enabled with no model chosen.
2. `StartSession` with `session_type = "warp-cli"` and an empty `model` is accepted. The daemon cuts a
   worktree on `warp-cli/<short id>` when `new_branch_name` is empty, and pushes it when **Create
   remote branch** is set.
3. The daemon spawns `warp` in the worktree, with no arguments. `StartSessionResponse` returns the
   `session_id`, and its LiveKit fields are empty.

Refused starts:

| Request | Status |
|---|---|
| `sandbox = true` | `FAILED_PRECONDITION` — "the sandbox runner does not host warp-cli" |
| `managed_codebase = true` | `INVALID_ARGUMENT` — Warp has no workflow of its own for a managed codebase to drive |
| a provider account chosen (`provider_account_id`) | `INVALID_ARGUMENT` — "Warp cannot be pointed at another provider; it always runs on its own vendor" (the refusal Cursor gets) |

A person's **Default for** a harness has no Warp row, so a start that names no account never picks
one up.

**The resume token**

- Every time the session's `warp` process exits, the daemon reads the tail of that terminal's output.
  It strips terminal escape codes, takes the **last** `warp --resume <TOKEN>` line, and records the
  token in `.session.yaml` (`harness_resume_token`).
- This happens on **every** exit, so a resumed run that prints a new token replaces the old one.
- An exit that printed no token (killed by a signal, crashed, or the daemon stopped first) leaves the
  recorded token unchanged.

**Resume**

- **Resume** relaunches `warp --resume <TOKEN>` in the session's worktree when a token is recorded.
- With no recorded token, it relaunches plain `warp`, a fresh conversation in the same worktree. It
  never passes a bare `--resume`.
- A resume whose worktree is gone is refused with `FAILED_PRECONDITION`, as for the other harnesses.

**The binary**

- Resolved in this order: `TDDY_WARP_CLI` when set and non-blank; else
  `cli_harnesses.warp-cli.binary_path` in `daemon.yaml`; else `warp` on `PATH`.

**Persistence, listing and delete**

- `.session.yaml` holds `session_type: warp-cli`, `repo_path` (the worktree), `pid`, `status` and,
  after the first exit that printed one, `harness_resume_token`. It holds no `model`.
- `ListSessions` shows `agent` as `warp-cli`. The PTY is served over the daemon's own connection, and
  `ConnectSession` names no terminal room.
- **Delete** stops the process and removes the session directory and its daemon-managed worktree. A
  client-supplied checkout is kept.
- `ListAgentModels("warp-cli")` answers an empty list.

### What's staying the same

- The four existing harnesses: their argv, models, routes and web forms. The registry gains an entry,
  and no existing descriptor changes.
- `cursor_chat_id` and Cursor's chat-id minting. `harness_resume_token` is a separate field.
- No activity-status hooks for Warp, as for OpenCode and Muse.
- The provider-account presets, and the Accounts screen.

## Impact Analysis

### Technical Impact

| Package | Change |
|---|---|
| `tddy-agent-backend` | `cli_harness_warp.rs`: the `WARP_CLI` descriptor, its argv and the resume-token parser. A registry entry. `Harness::WarpCli`, which refuses every preset. The `warp` context-globs row. An empty curated catalogue |
| `tddy-agent-launch` | Generic CLI-harness resume passes the recorded token. An exit watcher after `launch()` records the token at start and resume. A `warp-cli` start that chose a provider account is refused. The `harness_profile_start` arm |
| `tddy-changeset` | `SessionMetadata.harness_resume_token: Option<String>` (serde default, skipped when `None`), plus every struct literal across the workspace |
| `tddy-session-activity` | `worktree_removal_applies_to` includes `warp-cli` |
| `tddy-web` | `SessionType` gains `warp-cli`, a toggle entry, the request branch, no model field, Create enabled without a model |
| `tddy-session-lifecycle` (tests only) | Daemon-level start and resume tests for `warp-cli` beside the OpenCode and Muse ones |

No proto change: `session_type` is a free string, and the new metadata field is a YAML field. No new
external dependency: escape stripping is done in-crate, or with the existing `strip-ansi-escapes`,
which is already a workspace dev-dependency and must be **asked** before it becomes a production one.

### User Impact

- A new session type in the Create pane. No change to existing session types.
- Resume continues the Warp conversation only if Warp was let exit normally (`/exit` or Ctrl+C twice)
  at least once. A session whose `warp` was killed resumes into a fresh conversation in the same
  worktree.

## Implementation Plan

1. The descriptor, argv, token parser, `Harness::WarpCli` and context globs. Unit and acceptance tests
   in `tddy-agent-backend`.
2. The `SessionMetadata` field, and its literal sites updated mechanically.
3. The exit watcher and the token-carrying resume in `tddy-agent-launch`; the provider-account
   refusal; worktree removal.
4. Daemon-level start and resume tests with a stub `warp` that prints `warp --resume <TOKEN>` on exit.
5. Web: the session-type toggle, request, model-optional Create. A Cypress component spec.

## Acceptance Criteria

- [ ] The registry offers Claude CLI, Cursor CLI, OpenCode, Muse Code and Warp, in that order.
- [ ] A `warp-cli` start with an empty model spawns `warp` with no arguments, in a fresh worktree on `warp-cli/<short id>`.
- [ ] `.session.yaml` records `session_type: warp-cli`, the worktree and the pid, and no model.
- [ ] After `warp` exits printing `warp --resume tok-123`, `.session.yaml` holds `harness_resume_token: tok-123`.
- [ ] A later exit printing `warp --resume tok-456` replaces it; an exit printing no token leaves it as it was.
- [ ] Resume with a recorded token launches `warp --resume <token>`; with none, it launches `warp`, never a bare `--resume`.
- [ ] Resume marks the session active again, and a resume whose worktree is gone is refused.
- [ ] `sandbox = true`, `managed_codebase = true` and a chosen provider account are each refused before anything is cut or spawned.
- [ ] Deleting a `warp-cli` session removes its daemon-managed worktree.
- [ ] `ListAgentModels("warp-cli")` returns no models.
- [ ] Context files served to the agent: `AGENTS.md`, `.agents/**`, `WARP.md`, on top of the shared base.
- [ ] The web Create pane offers **Warp**, shows no model field, enables **Start New Session** with no model, and sends `sessionType: "warp-cli"` with `model: ""`.

## Out of scope

- Sandboxed Warp (a jailed `warp`): a backlog entry is added.
- A Warp provider account (a `warp` preset delivering `WARP_API_KEY`), deferred: connecting an account
  probes an HTTP model endpoint, and a Warp key has none. A backlog entry is added.
- Model selection; `--auto-approve`.
- Warp in an extra terminal tab of another session. That is node 2,
  `feature/warp-agent/agent-terminal-tabs`.
- A headless `oz agent run` tool-session backend.

## Unconfirmed

The flags, the exit line and the context files come from Warp's published reference
(docs.warp.dev, 2026-10). No real `warp` binary is installed on the development machine: Warp.app
bundles only `oz`. Tests pin the behaviour as documented. The exit-line format is the riskiest fact,
because the parser depends on it.

## References

- Warp Agent CLI reference: https://docs.warp.dev/agents/cli/reference/
- Warp Agent CLI overview: https://docs.warp.dev/agents/cli/
- Precedent: #667 (Muse Code terminal sessions)
