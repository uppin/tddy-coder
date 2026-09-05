# Agents Guide

`.agents/` is the **single canonical home** for this repo's assistant guidance. Every harness —
Claude Code and Cursor — reads the same files from here. `.claude/` and `.cursor/` hold only
symlinks into this directory, plus their own per-harness configuration.

## Layout

- `.agents/commands/*.md` → slash-command entrypoints (`/red`, `/green`, `/pr`, `/plan-red`, …)
- `.agents/skills/<name>/SKILL.md` → reusable workflows
- `.agents/skills/<name>/references/*.md` → supporting guidance attached to the relevant skill
- `.agents/agents/*.md` → subagent definitions (`tdd-implementer`, `refactor`, `test-writer`)

Symlinked from both harnesses:

```
.claude/{commands,skills,agents} ─┐
                                  ├─→ ../.agents/{commands,skills,agents}
.cursor/{commands,skills,agents} ─┘
```

## What stays per-harness

Not everything is shared, because the formats genuinely differ:

- `.cursor/rules/*.mdc` — Cursor-native rule format with no Claude Code equivalent. Shared files
  reference these by explicit repo-relative path (`.cursor/rules/testing-practices.mdc`), never as
  `@rule` shorthand, so both harnesses can resolve them.
- `.claude/settings.json`, `.claude/worktrees/` — Claude Code runtime state and permissions.

## Notes

- Write changes **here**, never in `.claude/` or `.cursor/` — a regular file (not a symlink)
  appearing in either harness's `commands`, `skills` or `agents` directory is unmigrated drift.
- **Subagent frontmatter is shared**, so it must stay portable. Do not add a harness-specific
  `model:` key (e.g. Cursor's `composer-2.5`) to `.agents/agents/*.md` — omit it and each harness
  applies its own default. Keys one harness does not recognise are ignored by the other.
- **Never delete a harness's mechanism to make a file portable; scope it instead.** Some steps
  exist on only one harness — Cursor's `AskQuestion` / `SwitchMode` tools, for example. Write them
  as a conditional so the file keeps working everywhere it already worked:

  > **On Cursor**: delegate to the `test-writer` subagent (`.agents/agents/test-writer.md`).
  > **On other harnesses**: write the tests yourself — the `tdd` skill carries the same behaviour.

## Skills are a product contract, not just tooling

`tddy-core` reads this directory at runtime: `AGENTS_SKILLS_DIR = ".agents/skills"`
(`packages/tddy-core/src/agent_skills.rs`). The TUI feature-prompt slash menu lists what it finds
here, and `docs/ft/coder/feature-prompt-agent-skills.md` specifies the behaviour.

Consequences when editing `.agents/skills/`:

- Each skill needs `SKILL.md` with YAML frontmatter carrying `name` and `description`.
- The frontmatter `name` **must match its folder name** — a mismatch is rejected as
  `InvalidSkillEntry` and the skill silently disappears from the slash menu.
- Adding or removing a skill folder changes what the shipped product shows its users.
