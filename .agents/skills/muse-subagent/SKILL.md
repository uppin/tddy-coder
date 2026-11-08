---
name: muse-subagent
description: >-
  `#muse` — offload a command's implementation stages to Muse Code run headless (`muse exec --json`, one session continued across turns by `--session-id`), with a Claude Code Sonnet · low subagent as its driver: it starts turns, follows and judges the JSONL stream, and resumes the same Muse session until the stage is really done. Built for `/yolo-pr-stack` (every node's `[G]`/`[W]`/`[F]`), and applies equally to `/green-yolo`'s phases and to `/green`, `/code-restructuring`, `/pr-wrap`, `/fix-pr` run on their own. Planning stays with the session's own model. Runs on this machine only. Use ONLY when the user asks for it — the `#muse` tag, or words like "let Muse do it" / "offload to Muse Code". Never choose it on your own.
---

# Muse subagent — `muse exec` does the stage, a driver subagent watches it

```
main agent (this session)                  driver subagent                  this machine
─────────────────────────                  ───────────────                  ────────────
$MA ensure local · guide login ───►                                         muse, logged in
brief (references/driver-brief.md) ───►    run ──────────────────────────►  muse exec --json --yolo
                                                                              --session-id <uuid> in <WORKTREE>
                                                                              └─► ~/.muse-runs/<name>/stream.jsonl
                                           wait · progress · summary ◄───
                                           not done? run --resume <sid>
◄── ✅ done | 🔴 stopped ─────────────────
report the stop in the end-of-turn summary
```

The session's own model keeps everything that decides *what* to build. Muse Code gets a stage whose
plan and failing tests already exist, and does the typing. The driver writes no code: it starts turns,
reads the stream, verifies what Muse Code claims against git and the PR, and resumes the same Muse
session until the stage's done condition holds.

## What is offloaded, per command

| Command | Stages Muse Code runs | Stays with the main agent |
|---|---|---|
| `/green` · `/code-restructuring` | G | preflight; a final scoped re-run of the changeset's tests |
| `/pr-wrap` (after `/validate-changes`) | W | preflight; reporting a stop |
| `/fix-pr` | F (a fresh session) | preflight; reporting a stop |
| `/green-yolo` | Phase A → G, Phase B → W, Phase C → F (W and F one session) | Phase 0 preflight, Phase D (`#automerge`), the report |
| `/yolo-pr-stack` | every node's G, W, F | all of planning, dispatch, the ledger, the sweep plan, the TODO gate (`yolo-pr-stack.md` § *Muse mode*) |

Every stage runs its command's file end to end, with the answers `green-yolo.md` § *Consent* gives
(standalone commands too: invoking a command with `#muse` is consent to that command's own
commit-and-push). Muse Code cannot ask — `--yolo` turns off `request_user_input` — so everything
`green-yolo.md` lists as **not pre-approvable** comes back as `STATUS: stopped` with the reason and
the options, and the main agent reports it in its end-of-turn summary. The developer answers by
re-invoking the command.

Never offloaded: the interview, PRDs, changesets, red commits, `/plan-pr-stack`'s 6e review, stack
cascades (`/pr-stack-rebase` over a range), landing (`/merge-pr-stack`, `/squash-pr`, `#automerge`).
A stage that turns out to need a planning decision stops, and the decision comes back here.

## The script

`.agents/skills/muse-subagent/scripts/muse-agent.sh` (call it `$MA`; use its absolute path — a
subagent's shell does not keep a `cd`). Never hand-roll `muse exec` lines.

| Command | Who | What |
|---|---|---|
| `ensure local` | main | `muse`, `jq`, `uuidgen` on PATH; one cheap probe turn proves the login (Muse Code has no status command); prints `MUSE_VERSION`, `MUSE_MODEL`, `READY=yes` |
| `run --host local --name --dir --prompt-file [--model] [--effort] [--resume <sid>]` | driver | start a turn detached and **return at once** (`TURN=`, `SESSION_ID=`, `WATCH=`) |
| `wait local <name> [secs=540]` | driver | block until the turn ends (`WAIT=turn-ended`, `RUN_EXIT=`) or `secs` pass (`WAIT=still-running`) |
| `progress local <name> [n]` | driver | the current turn's last n tool calls (command, exit code) and its final message |
| `summary local <name>` | driver, main | `SESSION_ID`, `TURNS`, `LAST_TURN_STATE` (`finished · error · interrupted · running`), `LAST_TURN_TERMINAL`, `RESULT`, `RESULT_EMPTY`, `STDERR` when a turn did not finish |
| `cancel local <name>` | driver, main | TERM the turn's process group, KILL after 10 s (`RUN_EXIT=130`) |
| `exec local <dir> <cmd…>` | driver | a shell command in a dir — the driver uses it only to read state |

Exit codes: **2** usage (including a non-local host, or a `--resume` that is not the name's
session) · **10** the probe turn did not complete — not logged in · **11** the probe turn was refused
for quota (API 429) — logged in, but nothing runs until the window resets · **14** busy: a turn of that name
is running, or another agent holds the worktree · **19** `muse` not on PATH · **20** `jq`/`uuidgen`
missing. `RUN_EXIT=16` is a turn killed by a machine stop or reboot: resume its session.

**How a turn lives.** Every turn of a `<name>` appends to `~/.muse-runs/<name>/stream.jsonl`, between
`muse-agent` `turn-start` and `turn-end` markers; Muse diagnostics go to `stderr-<n>.log` beside it.
**One `<name>` is one Muse session**: the script mints a UUID on the name's first turn and passes it
back as `--session-id` on every `--resume`, which continues the same conversation. A turn is a
detached process in its own process group, so it outlives the Claude Code session that started it;
watch it with `tail -f ~/.muse-runs/<name>/stream.jsonl`.

**The wire format** the script reads (Muse Code 1.4.x, `muse exec --json`): one JSON record per
line with a `payload_type`. The turn's final message is the `run.terminal.<state>` record's
`.payload.text` (`.payload.terminal` is `completed` on success; anything else is `error`); tool
calls are `tool.result` records whose `.payload.text` is a JSON object with `command` and
`exit_code`; `run.model.configured` names the model. If a Muse upgrade changes these, fix the
script, not the driver.

## Main agent

### 1. Make the host ready

`$MA ensure local`. Never install or log in for the user — it is their machine and their account.
Suggest the `!` prefix so the output lands in the conversation, and re-run `ensure` after the answer:

| Exit | Tell the user |
|---|---|
| **19** | *"Muse Code isn't installed (`muse` is not on PATH). Install it, then tell me."* |
| **10** | *"Muse Code isn't logged in. Run `! muse login` (or set `META_API_KEY`), then tell me."* |
| **20** | *"Run `! brew install jq`."* |
| **11** | *"Muse Code's quota is exhausted until <the reset time from the message>."* Do not dispatch; run the stages without `#muse` only if the user says so |

Keep `MUSE_MODEL` for the announcement line and the ledger. It is the model for every turn unless
the user named another one.

**Effort.** Passed as `--effort` (Muse's `--reasoning-effort`; its own default is `high`). Under
`/yolo-pr-stack`, from the node's complexity score (§ *Muse mode* there); standalone **medium**,
unless the user named one.

### 2. Preflight stays here

Run the invoked command's own preflight first — the branch, a clean tree, the PR state, stack
detection (`green.md` step 0, `green-yolo.md` Phase 0). A preflight stop is reported as the command
reports it; no driver starts.

**Only the user's own work.** `--yolo` runs every command unapproved and unsandboxed, with the reach
of the account it runs under. So the worktree's content must be the user's own: a branch they created
or planned in this session, or a PR whose author is them
(`gh pr view <PR> --json author --jq .author.login` equals `gh api user --jq .login`). A PR or branch
authored by anyone else is not offloaded — run the command without the tag, or ask.

**The worktree.** Standalone, the stage runs in this session's worktree. Under `/yolo-pr-stack` each
node gets its own worktree as usual (`yolo-pr-stack.md` § *0 — Ephemeral worktree*). **While a driver
runs, the main agent does not edit, commit, check out or rebase in that worktree** — Muse Code owns
it until the driver reports.

### 3. Brief the driver

Spawn one Claude Code subagent per stage with [references/driver-brief.md](references/driver-brief.md)
as its prompt: placeholders filled, the stage's prompt pasted in. The driver writes no code, so it
runs on **Sonnet · low**, passed explicitly — anything more is wasted, and Haiku misjudges "done".
Announce it in one line before the Agent call:

```
▶ #812 [G] /green → Muse Code (<MUSE_MODEL> · medium) on local, driven by sonnet · low — #muse
```

**Names.** One `<name>` per Muse session, reused only to continue it: `<command>-<branch>-<yyyymmddHHMM>`
standalone (`green-agent-tools-n3-202611081130`); under `/green-yolo`, `G` is `gy-<branch>-G` and `W`
and `F` share `gy-<branch>-W`; under `/yolo-pr-stack`, `yolo-<slug>-n<k>-G` and `yolo-<slug>-n<k>-W`.
Branch slashes become `-` (the script accepts `[A-Za-z0-9._-]` only).

### 4. Own restarts

- **`🔴 stopped`** is a stop the command set on purpose: report it in the end-of-turn summary with
  the reason and options, and end the turn. Never answer it for the developer.
- **`RUN_EXIT=16`** is the driver's to resume.
- After a context reset, re-derive each stage from `$MA summary local <name>`. Never re-run a turn
  whose session is still running.

### 5. Verify, then carry on

A driver's ✅ is a claim. Before the next stage, or before reporting: the head SHA is the one the
driver reported, pushed where the done condition says so, and the tree has no tracked changes.
After a standalone G, re-run the changeset's planned tests once, scoped (`./test -p <pkg>`). Then
continue the invoked command at the step after the offloaded stage.

## Rules

- **Opt-in only**: `#muse`, or the user's words. Never chosen on the agent's own initiative.
- **Planning never leaves the session.** Muse Code gets stages with a plan and red tests, nothing else.
- **The user's machine and accounts are the user's**: guide installs and logins, never run them.
- **Only the user's own branches and PRs** — `--yolo` runs every command unapproved and unsandboxed.
- **One name per Muse session**; `F` continues `W`'s name and session.
- **One agent per worktree.** The main agent stays out of a worktree a driver is running in; `run`
  refuses a second agent there with 14.
- **Verify, don't trust**: "done" means git and the PR checks say so.
- **Kill only what you started**: `$MA cancel local <name>`, nothing else — never `pkill`/`killall`
  by pattern, never a process, terminal or tmux session that belongs to the user or another agent.
- **Scoped gates only**, as [AGENTS.md § Verification](../../../AGENTS.md#verification-scope-it-locally-run-it-in-full-on-ci)
  says — every stage prompt gives Muse Code that rule.
- Never `--no-verify`, never amend, never force-push — every stage prompt says so to Muse Code.

## Related

`behaviour-modifiers` skill (the `#muse` registry row) · `/yolo-pr-stack` § *Muse mode* ·
`/green-yolo` § *Consent* (the answers every stage runs on) · `pr-stack` skill (worktree and branch
rules)
