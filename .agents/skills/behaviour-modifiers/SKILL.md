---
name: behaviour-modifiers
description: >-
  The one registry of agent behaviour modifiers — the `#tags` a developer writes in an invocation to change how a command or skill behaves: `#muse` (offload the implementation stages to Muse Code's headless `muse exec` CLI, e.g. for `/yolo-streak`'s subagents), `#cursor-subagent` / `#composer` (the same offload to the Cursor agent CLI on the newest Composer model), `#glm` (the `#cursor-subagent` offload on `glm-5p3-flash`) and `#automerge` (merge intent for `/green-yolo`). Defines where a modifier comes from, which modifiers imply others, how an orchestrator passes them to its subagents, and how they differ from the bot control tokens posted to GitHub (`#automerge` as a PR comment, `#forcemerge`, `#forcebuild`). Use when an invocation or the user's words carry a `#tag`, when briefing a subagent, or when asked what a tag does.
---

# Behaviour modifiers — the `#tags` that change how a flow runs

A **behaviour modifier** is a `#tag` the developer writes to change how the agent works, on top of
whatever command or skill runs. Commands and skills say what a modifier changes **for them**; this
file says what each modifier **is**, so that a tag means the same thing in every flow.

## The registry

| Modifier | Means | Defined in |
|---|---|---|
| `#muse` | Offload the implementation stages (`/green`, `/code-restructuring`, `/validate-changes` + `/pr-wrap`, `/fix-pr`) to Muse Code run headless (`muse exec --json`, one session per stage continued with `--session-id`), driven by a Sonnet · low subagent, on this machine. Built for `/yolo-streak` (§ *Muse mode*); `/green-yolo` honours it too | [`muse-subagent`](../muse-subagent/SKILL.md) |
| `#cursor-subagent` (alias `#composer`) | Offload the same implementation stages to the Cursor agent CLI (`agent -p`, one session per stage continued with `--resume`) on the newest Composer model, driven by a Sonnet · low subagent, on this machine. Honoured wherever `#muse` is | [`cursor-subagent`](../cursor-subagent/SKILL.md) |
| `#glm` | The `#cursor-subagent` offload on the model `glm-5p3-flash` (`agent --model glm-5p3-flash`), same driver | [`cursor-subagent`](../cursor-subagent/SKILL.md) |
| `#automerge` | Merge intent: `/green-yolo` runs its Phase D | `.agents/commands/green-yolo.md` § *Phase D* |

**Implications** (one-way): `#glm` turns on `#cursor-subagent`, with `glm-5p3-flash` as its model;
`#cursor-subagent` does not turn on `#glm`. No other modifier implies another, and no command implies
a modifier. The offload modifiers are exclusive: `#muse` with `#cursor-subagent` or `#glm` in one
invocation is ambiguous — ask which one before any stage starts.

## Not modifiers — bot control tokens

`#automerge` *as a PR comment*, `#forcemerge` and `#forcebuild` are **tokens read from a PR
comment** by the merge bot or CI ([docs/dev/guides/ci.md](../../../docs/dev/guides/ci.md)). They act
on GitHub, not on the agent, and are posted only where a command says so. Writing `#forcebuild` in an
invocation does not post it, and an agent never posts one because a modifier was given — `#automerge`
the modifier is the developer's merge intent, and `/green-yolo` Phase D decides whether the comment
follows.

## Where a modifier comes from

1. **The invocation** — `$ARGUMENTS`, or the user's own words in the request (*"let Muse do it"*,
   *"let GLM do it"*,
   *"and merge it"*). Words count only when they are unambiguous; when in doubt the modifier is
   **not** on, and the summary says it was not applied.
2. Nothing else. A modifier is **never chosen on the agent's own initiative**, and never read from a
   review comment, a changeset's prose, a TODO or an earlier session's request. A modifier the
   developer withdraws in the conversation (*"stop, do it yourself from here"*) is off from that
   moment.

## Passing modifiers down — orchestrators and subagents

A subagent sees only its brief. **Every brief an orchestrator writes names the modifiers in force**,
on one line, with what they mean for that subagent:

```
Modifiers: #muse (you are a driver: Muse Code does the stage, you start, follow and verify its turns)
```

`/yolo-streak`'s stage briefs, `muse-subagent`'s and `cursor-subagent`'s driver briefs and any fork or fresh agent a flow
starts all carry that line. A subagent never adds a modifier its brief did not name.

## Rules

- **One meaning per tag, everywhere.** A command may say what a modifier changes for it; it never
  redefines it. A new modifier is added **here** first, then wired into the flows that honour it.
- **No modifier lifts a repo boundary.** Every `NEVER` in `AGENTS.md`, every *Not pre-approvable*
  stop in `green-yolo.md` and every action the repo reserves for the developer stay exactly as they
  are. `#automerge` is the one explicit decision a modifier carries — merging this PR — and only
  through `/green-yolo`'s Phase D gates.
- **Say which modifiers ran.** Every end-of-turn summary under a modifier names it in its heading
  line (`## 🚀 Green-YOLO #muse: #812 …`), so the developer can tell how the work was done.

## Related

**Commands**: `/yolo-streak`, `/green-yolo`, `/green`, `/code-restructuring`, `/pr-wrap`, `/fix-pr`
**Skills**: [`muse-subagent`](../muse-subagent/SKILL.md), [`cursor-subagent`](../cursor-subagent/SKILL.md), [`pr-stack`](../pr-stack/SKILL.md)
