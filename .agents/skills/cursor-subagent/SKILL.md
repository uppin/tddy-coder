---
name: cursor-subagent
description: >-
  `#glm` / `#cursor-subagent` (alias `#composer`) — offload a command's implementation stages to the Cursor agent CLI (`agent -p --output-format stream-json`, one conversation continued with `--resume`) on `glm-5p3-flash` (`#glm`, i.e. `agent --model glm-5p3-flash`) or the newest Composer model (`#cursor-subagent` / `#composer`), with a Claude Code Sonnet · low subagent as its driver: it starts turns, follows and judges the agent's stream, and resumes the same Cursor session until the stage is really done. Built for `/yolo-streak` (every node's `[G]`/`[W]`/`[F]`), and applies equally to `/green-yolo`'s phases and to `/green`, `/code-restructuring`, `/pr-wrap`, `/fix-pr` run on their own. Planning stays with the session's own model. Runs on this machine only. Use ONLY when the user asks for it — `#glm`, `#cursor-subagent`, `#composer`, or words like "let GLM do it" / "offload to Cursor". Never choose it on your own.
---

# `cursor-subagent` — Cursor `agent -p` does the stage, a driver subagent watches it

```
main agent (this session)                  driver subagent                  this machine
─────────────────────────                  ───────────────                  ────────────
$CA ensure local · guide login ───►                                         Cursor agent CLI, logged in
brief (references/driver-brief.md) ───►    run ──────────────────────────►  agent -p in <WORKTREE>
                                                                              └─► ~/.composer-runs/<name>/stream.jsonl
                                           wait · progress · summary ◄───
                                           not done? run --resume <sid>
◄── ✅ done | 🔴 stopped ─────────────────
report the stop in the end-of-turn summary
```

The session's own model keeps everything that decides *what* to build. The Cursor agent gets a stage
whose plan and failing tests already exist, and does the typing. The driver writes no code: it starts
turns, reads the stream, verifies what the agent claims against git and the PR, and resumes the same
Cursor session until the stage's done condition holds.

## The tags

| Tag | Executor model |
|---|---|
| `#glm` | `glm-5p3-flash` (`GLM_MODEL` from `$CA ensure`; `agent --model glm-5p3-flash`) |

**`glm-5p3-flash` is an unlisted model: it must always be set explicitly.** `agent --list-models`
never shows it, yet `agent --model glm-5p3-flash` runs it. So:

- Never conclude from `--list-models` (or from the model's absence in Cursor's picker) that `#glm`
  is unavailable, and never swap to a listed GLM model (`glm-5.2-*`) because the flash one "is not
  there". `$CA ensure` probes it with a one-line turn; only a failed probe means it is unavailable.
- Every turn passes `--model glm-5p3-flash` explicitly, in every `$CA run` and in any hand check
  (`agent -p --model glm-5p3-flash …`). Never rely on Cursor's default model: an omitted `--model`
  silently runs a different model.
- **Cursor turns the unlisted model down intermittently.** A turn can exit 1 within seconds, with
  no tool calls, printing `Cannot use this model: glm-5p3-flash. Available models: …` — and the
  identical command succeeds minutes later (observed: rejected at 08:46 and 08:55, accepted at
  08:51, 08:57 and 08:59 on the same run script). That is a transient refusal, not lost access:
  wait 2 minutes and `run` the same turn again (same `<name>`, same `--resume`), up to 3 times. Such
  a failed start does not count toward the turn cap. Only after the third consecutive rejection is
  it reported, still without switching models; `$CA ensure` re-probing the model tells the main
  agent whether access is really gone.
| `#cursor-subagent` (alias `#composer`) | the newest Composer (`COMPOSER_MODEL` from `$CA ensure`) |

Both run on this machine. Opt-in only: the tag or the user's words. A tagged request never lapses
into doing the stage on the session's own model because the offload is hard — stop and say why
instead. `#glm` is this offload with the GLM model; it never swaps to Composer, nor the reverse,
without the developer naming the other tag.

## What is offloaded, per command

| Command | Stages the Cursor agent runs | Stays with the main agent |
|---|---|---|
| `/green` · `/code-restructuring` | G | preflight; a final scoped re-run of the changeset's tests |
| `/pr-wrap` (after `/validate-changes`) | W | preflight; reporting a stop |
| `/fix-pr` | F (a fresh session) | preflight; reporting a stop |
| `/green-yolo` | Phase A → G, Phase B → W, Phase C → F (W and F one session) | Phase 0 preflight, Phase D (`#automerge`), the report |
| `/yolo-streak` | every node's G, W, F | all of planning, dispatch, the ledger, the sweep plan, the TODO gate (`yolo-streak.md` § *Muse mode* — the same table applies, with this skill's script, stream and model) |

Every stage runs its command's file end to end, with the answers `green-yolo.md` § *Consent* gives
(standalone commands too: invoking a command with `#glm` is consent to that command's own
commit-and-push). Print mode has nobody to answer, so everything `green-yolo.md` lists as **not
pre-approvable** comes back as `STATUS: stopped` with the reason and the options, and the main agent
reports it in its end-of-turn summary. The developer answers by re-invoking the command.

Never offloaded: the interview, PRDs, changesets, red commits, `/plan-streak`'s 6e review, stack
cascades (`/pr-stack-rebase` over a range), landing (`/merge-pr-stack`, `/squash-pr`, `#automerge`).
A stage that turns out to need a planning decision stops, and the decision comes back here.

## The script

`.agents/skills/cursor-subagent/scripts/cursor-agent.sh` (call it `$CA`; use its absolute path — a
subagent's shell does not keep a `cd`). The host is always `local`. Never hand-roll `agent -p` lines.

**Take `$CA` from an up-to-date checkout** — the main session's worktree on `master`, never a stack
node's worktree, whose branch can predate the script.

| Command | Who | What |
|---|---|---|
| `ensure local` | main | `jq`; the Cursor agent CLI; its login; prints `COMPOSER_MODEL`, `GLM_MODEL`, `READY=yes`. `glm-5p3-flash` is **unlisted** (`agent --list-models` never shows it), so `GLM_MODEL` comes from a one-line probe turn on it |
| `run --host local --name --dir --prompt-file --model [--resume <sid>]` | driver | start a turn detached and **return at once** (`TURN=<n>`, `WATCH=…`) |
| `wait local <name> [secs=170]` | driver | block until the turn ends (`WAIT=turn-ended`, `RUN_EXIT=`) or `secs` pass (`WAIT=still-running`) |
| `progress local <name> [n]` | driver | the current turn's last n tool calls and messages |
| `summary local <name>` | driver, main | `SESSION_ID`, `TURNS`, `LAST_TURN_STATE` (`finished · error · interrupted · running`), `RESULT`, `RESULT_EMPTY`, `LAST_THOUGHT` |
| `cancel local <name>` | driver, main | kill the current turn's process group (`RUN_EXIT=130`) |
| `exec local <dir> <cmd…>` | driver | a shell command in a dir — the driver uses it only to read state |

Exit codes the caller must act on: **2** usage (including a host other than `local`) · **10** Cursor
not logged in · **12** the account has neither a Composer nor the `glm-5p3-flash` model · **14** busy
— a turn of that `<name>` is still running, or another agent holds the worktree · **19** Cursor
agent CLI not installed · **20** `jq` missing. `RUN_EXIT=16` is a turn killed by a machine stop or
reboot: resume its session.

**How a turn lives.** Every turn of a `<name>` appends to `~/.composer-runs/<name>/stream.jsonl`,
between `composer-agent` `turn-start` and `turn-end` markers. **One `<name>` is one Cursor session**,
continued with `--resume`, and that stream is what drivers judge from and what the main agent re-reads
after a context reset. A turn is a detached process in its own process group, so it outlives the
Claude Code session that started it; watch it with `tail -f ~/.composer-runs/<name>/stream.jsonl`.

## Main agent

### 1. Make the host ready

`$CA ensure local`, and on a stop guide the user. Never install or log in for them: it is their
machine and their account. Suggest the `!` prefix so the output lands in the conversation, ask with
`AskUserQuestion` when it is available, and re-run `ensure` after the answer.

| Exit | Tell the user |
|---|---|
| **19** no Cursor agent CLI | *"The Cursor agent CLI isn't installed. Run `! curl https://cursor.com/install -fsS \| bash`, then tell me."* (It installs `agent` into `~/.local/bin`, which must be on `PATH`.) |
| **10** not logged in | *"The Cursor agent isn't logged in. Run `! agent login` (with `NO_OPEN_BROWSER=1` it prints a URL to open), then tell me."* `agent status` shows the result |
| **20** no `jq` | *"Run `! brew install jq`."* |
| **12** no Composer or GLM model | the account cannot use either. Report it; do not pick another model unasked |

**Which model for every turn** (from `$CA ensure`, always passed explicitly to `$CA run --model …` — never omitted): `#glm` →
`GLM_MODEL` (`glm-5p3-flash`); `#cursor-subagent` / `#composer` → `COMPOSER_MODEL`. An empty value
for the tag's model is exit 12 for that tag. **Under `/yolo-streak` and `/green-yolo` the chosen
model is mandatory** for the whole run.

### 2. Preflight stays here

Run the invoked command's own preflight first — the branch, a clean tree, the PR state, stack
detection (`green.md`, `green-yolo.md` § *Phase 0*). A preflight stop is reported as the command
reports it; no driver starts.

**Only the user's own work.** The Cursor agent runs with `--trust --force`: it executes every shell
command without approval, with the reach of the account it runs under. So the worktree's content must
be the user's own: a branch they created or planned in this session, or a PR whose author is them
(`gh pr view <PR> --json author --jq .author.login` equals `gh api user --jq .login`). A PR or branch
authored by anyone else is not offloaded — run the command without the tag, or ask.

**The worktree.** Standalone, the stage runs in this session's worktree. Under `/yolo-streak` each
node gets its own worktree as usual. **While a driver runs, the main agent does not edit, commit,
check out or rebase in that worktree** — the Cursor agent owns it until the driver reports.

### 3. Brief the driver

Spawn one Claude Code subagent per stage with [references/driver-brief.md](references/driver-brief.md)
as its prompt: placeholders filled and the stage's prompt pasted in. The driver writes no code, so it
runs on **Sonnet · low**, passed explicitly; anything more is wasted, and a smaller tier misjudges
"done". Announce it in one line before the Agent call:

```
▶ #812 [G] /green → glm-5p3-flash via cursor agent -p, driven by sonnet · low — #glm
```

**Names.** One `<name>` per Cursor session, reused only to continue it, unique on the machine:
`<command>-<branch>-<yyyymmddHHMM>` standalone (`green-feat-foo-202611281130`); under `/green-yolo`,
`G` is `gy-<branch>-G`, and `W` and `F` share `gy-<branch>-W`; under `/yolo-streak`,
`yolo-<slug>-n<k>-G` and `yolo-<slug>-n<k>-W`.

### 4. Own restarts

- **`🔴 stopped`**: report it in the end-of-turn summary with the reason and options, and end the
  turn. Never answer it for the developer.
- **`RUN_EXIT=16`** is the driver's to resume.
- After a context reset, re-derive each stage from `$CA summary local <name>`. Never re-run a turn
  whose session is still running.
- **A driver that returns mid-stage** — an interim message ("still waiting") with no Report — has
  lost its loop, not its stage. Hold the wait yourself: one background Bash call that repeats
  `$CA wait local <name> 170` until it prints `WAIT=turn-ended`, then `SendMessage` the driver to
  continue from step 5 of its loop. Never start a second driver or a second turn for that `<name>`.

### 5. Verify, then carry on

A driver's ✅ is a claim. Before the next stage, or before reporting: the head SHA is the one the
driver reported, pushed where the done condition says so, and the tree is clean of tracked changes.
After a standalone G, re-run the changeset's planned tests once in the worktree, scoped
(`./test -p <pkg>`). Then continue the invoked command at the step after the offloaded stage.

## Rules

- **Opt-in only**: `#glm`, `#cursor-subagent`, `#composer`, or the user's words.
- **Executor model per tag** — `#glm` → `GLM_MODEL` (`glm-5p3-flash`); `#cursor-subagent` /
  `#composer` → `COMPOSER_MODEL`. Both come from `$CA ensure local`.
- **Planning never leaves the session.** The Cursor agent gets stages with a plan and red tests,
  nothing else.
- **The user's machine and accounts are the user's**: guide installs and logins, never run them.
- **Only the user's own branches and PRs.** The agent runs every command unapproved
  (`--trust --force`); never point it at a worktree holding someone else's code.
- **One `<name>` per Cursor session**, continued with `--resume`. A stage that continues another
  (`F` after `W`) uses the same name and session.
- **One agent per worktree.** The main agent stays out of a worktree a driver is running in; `run`
  refuses a second agent there with 14.
- **Verify, don't trust**: "done" means git and the PR checks say so, not the agent's `STATUS:` line.
- **Kill only what you started.** Stop a turn with `$CA cancel local <name>` — it ends that name's
  own process group, nothing else. Never `pkill`/`killall` by pattern, and never a process, terminal
  or tmux session that belongs to the user or to another agent — not even in cleanup.
- Never `--no-verify`, never amend, never force-push. The same rules as local work, and every stage
  prompt gives them to the Cursor agent.

## Related

[`behaviour-modifiers`](../behaviour-modifiers/SKILL.md) (`#glm`, `#cursor-subagent`) ·
[`muse-subagent`](../muse-subagent/SKILL.md) (the same pattern on Muse Code) · `/yolo-streak`
§ *Muse mode* · `/green-yolo` § *Consent* · [`pr-stack`](../pr-stack/SKILL.md) (worktree and branch rules)
