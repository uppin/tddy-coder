# Initial Discovery Artifact

Every planning flow that analyzes the codebase **must** persist the full Explore / Discovery pass
in a companion file next to the changeset. Later sessions reconstruct State A from this file
instead of re-running the search.

This file is **not** a changeset, **not** State A, and **not** wrapped into package docs. It is
planning evidence. `/wrap-context-docs` deletes it with the changeset.

## Location and naming

```
docs/dev/1-WIP/{changeset-slug}-initial-discovery.md
```

`{changeset-slug}` is the changeset basename without `.md` — the same `YYYY-MM-DD-feature-name`
used for `docs/dev/1-WIP/YYYY-MM-DD-feature-name.md`.

Pick the slug **before** the first exploration (Step 2 of `planning-phase.md`) so the discovery
file exists before the changeset is written.

**One companion per changeset.** Concurrent WIP changesets each have their own file. A PR stack
does not share one file: wrap of PR₁ would delete discovery that PR₂ still needs.

## When to write and update

1. **First exploration (mandatory)** — during planning code analysis, before the PRD is approved
   and before the changeset is created. Create the file as soon as the first Explore pass returns.
2. **Later explorations** — if more Explore / Grep / Glob / Read work happens while writing the
   changeset or resuming a plan, **append** a new `## Exploration N` section at the tail and
   **rewrite** `## Combined Conclusions` at the top. Never delete or overwrite an earlier pass.
3. **Changeset link (mandatory)** — the changeset's first content section after the header is
   `## Initial Discovery`, pointing at this file. See `.cursor/rules/changeset-doc.mdc`.

Do not skip this artifact because the exploration felt small. A single Grep + three Reads is still
one exploration pass.

## How to explore

**On Cursor**: launch the `explore` subagent via `Task` (`subagent_type: "explore"`). Give it the
dump contract below so its return value is storeable, not a one-paragraph summary.

**On other harnesses**: use the equivalent Explore / code-search agent, or run Grep / Glob / Read
yourself. Either path is an exploration pass and must be recorded the same way.

Parent-driven Grep / Glob / Read without a subagent still counts. Record it as its own
`## Exploration N` with the same sections.

### Explore-agent dump contract

When launching Explore, require this return shape (verbatim intent, not a polite summary):

```text
Return a complete discovery dump, not a summary.

1. Sequence — chronological list of every glob, grep, and file read, with the pattern or path
   and why you ran it.
2. Inspected files — every path opened, why, and the code excerpts that informed the findings
   (not entire files; not path-only lists).
3. Grep / glob — every pattern, path scope, and the notable hits (file:line or a short snippet).
4. Findings — what this pass established about current architecture, APIs, and behavior.

The parent stores this dump in docs/dev/1-WIP/{slug}-initial-discovery.md. Combined conclusions
are written by the parent, not by this agent.
```

## File structure

Combined conclusions stay at the **top**. Each exploration pass is appended at the **tail**,
oldest first. Later passes never replace earlier ones.

````markdown
# Initial Discovery: {Feature / Change Name}

**Changeset**: [YYYY-MM-DD-feature-name.md](./YYYY-MM-DD-feature-name.md)
**Date**: YYYY-MM-DD
**Passes**: N

## Combined Conclusions

Synthesize every pass. This is the actionable reading of State A — packages in play, current
architecture, APIs and behavior that will change, constraints, conflicts with other WIP
changesets, and open questions. Rewrite this section after each new pass so it stays current.

Do not put tool traces here. Those belong in the Exploration sections below.

## Exploration 1: {short title} — {YYYY-MM-DD}

**Agent**: Explore subagent | parent Grep/Glob/Read | other
**Scope**: {packages, directories, or question this pass answered}

### Sequence

1. Glob `{pattern}` in `{path}` — {why}
2. Grep `{pattern}` in `{path}` — {why}
3. Read `{file}` — {why}
4. …

### Grep / glob

| Tool | Pattern / glob | Path scope | Notable hits |
|------|----------------|------------|--------------|
| Glob | `**/session*.rs` | `packages/tddy-daemon/` | `session.rs`, `session_store.rs` |
| Grep | `fn resume_session` | `packages/tddy-daemon/src` | `session_store.rs:140` |

### Inspected files

#### `packages/{package}/src/{module}.rs`

**Why**: {what you were checking}
**Excerpt**:

```rust
// the code that informed the plan — enough to reconstruct the finding
```

### Findings

What this pass established. Keep pass-local; the top of the file holds the synthesis.

## Exploration 2: {short title} — {YYYY-MM-DD}

…
````

## Capture rules

- **Full dump, not a teaser.** Inspected files, grep/glob queries, and the exploration sequence
  must be in the file. Combined Conclusions are not a substitute for the passes.
- **Code in the file.** Each inspected file gets a relevant excerpt, not only a path. Quote the
  region that informed State A or the plan. Do not paste entire files.
- **Every pass separately.** Two Explore launches, or an Explore pass plus a later parent Grep
  session, are two `## Exploration N` sections.
- **Chronological tail.** Exploration 1 is the first pass. New passes append after the last one.
- **Rewrite only Combined Conclusions** when a new pass lands. Do not edit prior Exploration
  sections except to fix factual transcription errors.
- **Do not invent searches.** Only record globs, greps, and reads that actually ran.
- **Do not merge this file into package docs.** Wrap deletes it. State B lives in the changeset
  and then in stable docs; discovery evidence does not.

## Changeset section

The changeset opens with this section immediately after the header metadata (before Affected
Packages):

```markdown
## Initial Discovery

Full codebase exploration that grounded this plan: [initial-discovery.md](./YYYY-MM-DD-feature-name-initial-discovery.md).

State A below is distilled from that file. Do not duplicate grep traces or file dumps here.
```

Use the real companion filename in the link. The link text may stay `initial-discovery.md`.

## Wrap

`/wrap-context-docs` deletes `{changeset-slug}-initial-discovery.md` in the same step that deletes
the changeset. It does **not** transfer Combined Conclusions or Exploration sections into
`packages/{package}/docs/`.

On a stack, delete **this PR's** companion only.

If the companion is missing at wrap time, say so in the Wrap Report and continue — do not block
wrap on a missing discovery file.

## Related

- `planning-phase.md` — Step 2 creates this file
- `.cursor/rules/changeset-doc.mdc` — changeset template the `## Initial Discovery` section opens
- `.agents/commands/wrap-context-docs.md` — deletes this file
- `code-restructuring` — steps 1–3 persist this companion; the Refactor changeset opens with `## Initial Discovery`
