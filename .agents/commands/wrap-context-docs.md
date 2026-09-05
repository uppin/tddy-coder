---
description: Transfer knowledge from completed changesets and PRDs into permanent documentation, then delete the working documents.
---

# Wrap Context Documentation

Transfer knowledge from changesets and PRDs into permanent documentation, then clean up working documents.

## Core Principle

**"Wrapping"** means:
1. Extract the final state (State B) from the working document
2. Update the actual permanent docs with that knowledge
3. Add a changelog or changeset **index** entry (audit trail)—see merge hygiene below
4. Delete the working document

Wrapping is **NOT** just adding changelog entries. It is a full knowledge transfer. In particular it is **not**:
- Only adding a changelog/changesets entry
- Leaving the feature/dev docs unchanged
- Creating links to the deleted PRD/changeset files

**State B, not delta**: the permanent docs must read as cohesive, unified documents with no trace of the change process — no "previously", "now", "changed from", or other temporal language.

## Changelog / changeset format — one file per entry

Follow [changelog-merge-hygiene.md](../../docs/dev/guides/changelog-merge-hygiene.md).

Changelogs and changeset histories are **directories**, not documents. Wrapping **creates a new
file** in each; it never opens an existing one.

| Target | Path | The entry |
|---|---|---|
| Product changelog | `docs/ft/<area>/changelog/` | `YYYY-MM-DD-<slug>.md` |
| Package changesets | `packages/<pkg>/docs/changesets/` | `YYYY-MM-DD-<slug>.md` |
| Cross-package changesets | `docs/dev/changesets/` | `YYYY-MM-DD-<slug>.md` |

- **First line** `# YYYY-MM-DD — Title`; for a changeset, `**Type:** Feature` (or `Fix`,
  `Architecture`, …) on the next line. Then the body.
- **No length rule.** The old one-bullet-one-line discipline existed only to keep git's union
  merge driver effective. That driver is gone — write what the change deserves.
- **Never edit, reorder or retitle an existing entry** while wrapping. Corrections are their own
  commit.
- **Never create an index.** There isn't one, by design: a shared append-point is precisely what
  this layout removes. The directory listing is the index.
- **Links are relative to the entry's directory** — `../../ft/coder/x.md` from
  `docs/dev/changesets/`, `../../../docs/ft/coder/x.md` from `packages/<pkg>/docs/changesets/`.
- **Slug collision** on the same day: append `-2`.

## Decision Logic

Prerequisites for wrapping — **all** must be true:
- Every Scope checkbox marked `[x]`
- All acceptance criteria met
- Document status is Complete
- **On a stack branch**: this PR is being set ready for review, its parent has already wrapped, and the
  document being wrapped is one this PR owns — see [Stack Mode](#stack-mode--wrapping-one-pr-of-a-stack)

**If all checkboxes are `[x]`** -> Proceed with wrapping.

**If any checkboxes are NOT `[x]`** -> Display the CRITICAL DISCLAIMER (below), skip the wrap, and keep the document for future work.

**If no working documents are found** -> Report that and stop.

## CRITICAL DISCLAIMER (wrapping blocked)

```
+------------------------------------------------------------------------+
|                                                                        |
|   !! WRAPPING BLOCKED - INCOMPLETE ITEMS DETECTED !!                   |
|                                                                        |
|   Document: <name>.md                                                  |
|                                                                        |
|   The following items are not marked complete:                          |
|                                                                        |
|   - [ ] Item 1 description                                             |
|   - [~] Item 2 description                                             |
|                                                                        |
|   IMPACT:                                                              |
|   - Knowledge stays in 1-WIP/, not transferred to permanent docs       |
|   - Technical debt accumulates                                         |
|                                                                        |
|   Wrapping incomplete work will permanently lose tracking of           |
|   unfinished items.                                                    |
|                                                                        |
+------------------------------------------------------------------------+
```

The disclaimer must:
- Be the **first** thing in your response, before any other text
- List **every** incomplete item with its current status (Not Started / In Progress)
- State the concrete impact of not wrapping
- Offer the three options below, with an effort estimate where you can give one

Then present three options to the user:

1. **Complete All Work** (recommended when the remaining items are a few hours of work) - finish the remaining items, set the status to Complete, and re-run `/wrap-context-docs`.
2. **Accept Current State** - wrap anyway: mark the incomplete items "Deferred" or "Out of Scope", document **why** they are deferred, and set the status to Complete (Phase N). Only choose this when the current state is cohesive, deployable, and architecturally sound.
3. **Keep Working** - abort the wrap and continue development, calling `/update-context-docs` as you progress; wrap when the work is truly complete.

Do not proceed without the user choosing an option.

## What "Apply to the permanent docs" means

For each working document:

1. **Read and understand** the changeset/PRD completely
2. **Extract State B** - what the docs should say after wrapping (ignore deltas, rationale, and process descriptions)
3. **Find merge points** - which sections of which target docs need to change
4. **Transform the language** - "Changed to X" → "Uses X"; "Now supports Y" → "Supports Y"; strip "new", "updated", "previously"
5. **Merge intelligently** - replacement, addition, enhancement, or removal of sections
6. **Update the actual feature/dev docs** with the new content
7. **Then** add the changelog/changeset index entry as the audit trail
8. **Delete** the source document

## Wrapping Changesets

For changesets in `docs/dev/1-WIP/`:

1. **Extract State B** from the changeset document
2. **Apply to dev docs**: Update `packages/*/docs/` with the final state descriptions
3. **Update change history**:
   - Create **one new file** in each affected `packages/{package}/docs/changesets/`, named `YYYY-MM-DD-<slug>.md`.
   - If the work is cross-package, create **one new file** in `docs/dev/changesets/` too.
4. **Delete** the changeset file from `docs/dev/1-WIP/` (not archived)

## Wrapping PRDs

For PRDs in `docs/ft/*/1-WIP/`:

1. **Extract State B** - the final feature specification
2. **Apply to feature docs**: Update `docs/ft/{area}/` with the completed feature documentation
3. **Add changelog entry**: create a **new file** `docs/ft/{area}/changelog/YYYY-MM-DD-<slug>.md` starting `# YYYY-MM-DD — Title` (see the format section above). Do not reference deleted PRD filenames.
4. **Delete** the PRD file from `docs/ft/*/1-WIP/` (not archived)

## Wrapping Superpowers Working Docs

Design specs (`docs/superpowers/specs/`) and implementation plans (`docs/superpowers/plans/`) are working documents produced by the `superpowers:brainstorming` and `superpowers:writing-plans` skills. Once the implementation is complete, their knowledge is fully captured in the code and permanent docs — they have no permanent documentation role.

For files in `docs/superpowers/specs/` and `docs/superpowers/plans/`:

1. **Verify implementation is complete** — confirm the feature was built and the relevant changesets/PRDs have already been wrapped
2. **No knowledge transfer needed** — the spec/plan content was already transferred into permanent docs via the changeset/PRD wrapping step
3. **Delete** the file (not archived) — its purpose is fulfilled

These files do **not** get their own changelog entries; the changeset/PRD wrap already captures the audit trail.

## Stack Mode — wrapping one PR of a stack

**Load the `pr-stack` skill (`.agents/skills/pr-stack/SKILL.md`) first** — it owns the document
ownership rules and the bottom-up wrap order summarised below.

**Detect first.** This branch is in a stack when its PR is based on another open PR's branch:

  ```bash
  gh pr list --state open --json number,headRefName,baseRefName
  gh pr view --json baseRefName --jq .baseRefName      # not master/main → this branch has a stack parent
  ```

Neither matches → ordinary branch, skip this section. A match → wrap **this PR's** documentation under
the rules below. Model: the `pr-stack` skill and
the `pr-stack` skill § *Per-PR documents*.

### S1. Know which document you are wrapping, and who owns it

On a stack branch this tree contains **every predecessor's** working documents as well as this PR's,
because a stacked branch inherits their commits. Only this PR's are wrapped:

| Document | Where it lives | Wrapped by |
|---|---|---|
| A **predecessor's** changeset / PRD | `docs/dev/1-WIP/`, inherited through the branch's history | **That PR**, when it is set ready for review. Never wrap or delete one from here — it would show as a deletion in that PR's own diff |
| This branch's changeset | `docs/dev/1-WIP/YYYY-MM-DD-*.md` | **this PR** — the wrap this command performs |
| This branch's PRD | `docs/ft/*/1-WIP/PRD-YYYY-MM-DD-*.md` | **this PR** |
| Permanent docs | `packages/*/docs/`, `docs/ft/<area>/` | reached **only** through a wrap |

Consequences worth stating plainly:

- **The attached `changeset.md` is not a substitute for a branch changeset.** It never enters the repo,
  so it cannot carry State B into `packages/*/docs/`. If this PR produced knowledge that belongs in
  package or feature docs and there is no changeset for it in `docs/dev/1-WIP/`, write one on this branch
  first, then wrap it.
- **Never edit `packages/*/docs/` directly** (CLAUDE.md) — the changeset workflow via `docs/dev/1-WIP/`
  is the only path in, on a stack branch as anywhere else.
- A predecessor's documents stay untouched by the wrap. They are the boundary contract this PR was
  built against, and that PR owns their lifecycle.

### S2. Wrap only the documents THIS PR owns

A stack branch is based on its parent's branch, so its tree contains the **parents'** documents as well
as its own. They are **inherited, not this PR's to wrap.**

Separate the two by authorship, not by "what happens to be in `1-WIP`":

```bash
base=$(gh pr view --json baseRefName --jq .baseRefName)
git diff --name-only --diff-filter=A "origin/$base"...HEAD -- 'docs/dev/1-WIP/*' 'docs/ft/*/1-WIP/*'
```

Files this branch's own commits added are this PR's. Everything else in those directories is inherited.

- ❌ Never wrap or delete a **parent's** changeset or PRD — that PR wraps its own when it goes ready.
- ❌ **Never delete a parent's document to shrink this PR's diff.** A stale inherited copy — the parent
  merged and wrapped, so its `1-WIP` file is gone on the base but still present here — is
  `/pr-stack-rebase`'s to clear: the deletion arrives with the base. Re-deleting it here puts a parent's
  file in this PR's diff as a loss.
- ❌ Never wrap a **dependent's** documents — they do not exist in this tree, and a deletion inside a
  shared file would surface as loss in the dependent's diff.
- ✅ Wrap exactly the set whose scope matches this PR's `## Responsibility` — equivalently, this
  branch's own commits.

### S3. Link dependents by PR number and URL, never by document path

- Forward links to dependent PRs go into the permanent docs as **PR number + URL** plus the stack name.
  A `docs/dev/1-WIP/` path disappears the moment that dependent wraps, so it must never be carried into
  permanent documentation.
- Never add a link back to a **parent's** working document: the parent wraps first, so the link dangles
  immediately. Reference the parent's PR number/URL, or the permanent doc its wrap produced.
- Changelog and changeset entries follow
  [changelog-merge-hygiene.md](../../docs/dev/guides/changelog-merge-hygiene.md) — **one new file
  per directory**: `packages/*/docs/changesets/`, `docs/dev/changesets/` for cross-package work, and
  `docs/ft/<area>/changelog/`. In a stack this is what stops several branches colliding inside one
  merge window: each node adds its own file, so there is nothing to conflict over. **Give each node
  a distinct slug** — several nodes of one stack land on the same day, and `-2` suffixes make the
  history unreadable. Never rewrite another node's entry.

### S4. In a stack, wrap bottom-up

Each parent's docs leave `docs/dev/1-WIP/` before its dependents' do. Forward-only linking depends on
that ordering — a forward link from a parent to a dependent's working document dangles if the dependent
wrapped first.

So a dependent that finishes ahead of its parent **stays a draft with its changeset in
`docs/dev/1-WIP/`** until the parent has been readied and wrapped. Report that as the reason rather than
wrapping out of order.

### S5. The wrap is not the whole readiness gate

Wrapping is what a stacked PR does when it is being **set ready for review** — not while it stays a
draft (its changeset belongs in `1-WIP` where the next session picks it up) and not after merge.

The full readiness gate, the PR title correction and `gh pr ready <N>` live in `/pr-wrap` step 8. This
command's part of it is narrow: report whether the wrap actually cleared **this PR's** documents out of
`docs/dev/1-WIP/` and `docs/ft/*/1-WIP/`, and which inherited documents it deliberately left in place.

**Nothing hands off.** A wrap touches only the branch it runs on: no cross-branch push, no shared
manifest, no "action required" notice. The stack-level document (`pr-stack-plan.md`) and the live
topology (`gh pr list`) needs no committed copy.

## Process

1. Identify documents to wrap:
   - Changesets: `docs/dev/1-WIP/`
   - PRDs: `docs/ft/*/1-WIP/`
   - Superpowers working docs: `docs/superpowers/specs/` and `docs/superpowers/plans/`
   - **Stack branch**: narrow this set to the documents **this PR** added — see
     [Stack Mode](#stack-mode--wrapping-one-pr-of-a-stack); inherited parent documents are excluded, and
     a predecessor's `docs/dev/1-WIP/` documents are never in this set
2. For each changeset/PRD, check completion status
3. Apply decision logic (complete vs incomplete)
4. Execute the wrap: extract -> update docs -> create the changeset/changelog entry files -> delete source
5. For superpowers working docs, verify implementation complete then delete
6. Report what was wrapped and where knowledge was transferred

## Output Format

```markdown
## Documentation Wrap Report

### Wrapped Documents
| Source | Target | Status |
|--------|--------|--------|
| docs/dev/1-WIP/feature.md | packages/{package}/docs/ | Applied |
| docs/ft/{area}/1-WIP/PRD-....md | docs/ft/{area}/ | Applied |

### Deleted Sources
- `docs/dev/1-WIP/2026-01-22-feature.md`

### Left in Place (stack branches only)
| Document | Why |
|----------|-----|
| docs/dev/1-WIP/2026-01-20-parent-feature.md | inherited from parent PR #412 — that PR wraps it |
| a predecessor's `docs/dev/1-WIP/` changeset | that PR's document, never wrapped from here |

### Skipped (Incomplete)
| Document | Missing |
|----------|---------|
| docs/dev/1-WIP/other.md | Testing, Code Quality |

Reason: complete the remaining work before wrapping.
```

## Common Mistakes to Avoid

- **WRONG**: only adding a changelog entry without updating the docs
- **WRONG**: linking to deleted PRD/changeset files
- **WRONG**: leaving delta language ("changed from X to Y") in the final docs
- **WRONG**: leaving the feature/dev docs unchanged
- **WRONG** (stack): wrapping or deleting a **parent's** changeset/PRD, or deleting one to shrink this
  PR's diff — a stale inherited copy is `/pr-stack-rebase`'s to clear
- **WRONG** (stack): treating a predecessor's inherited `docs/dev/1-WIP/` changeset as the
  document to wrap — it is a read-only session artifact, not a repo file
- **WRONG** (stack): carrying a `docs/dev/1-WIP/` path for a dependent into permanent docs instead of its
  PR number and URL
- **WRONG** (stack): wrapping a dependent before its parent has wrapped
- **CORRECT**: extract from PRD/changeset → update feature/dev docs → add changelog entry → delete source

## Related

- **Rules**: [.cursor/rules/changeset-doc.mdc](../../.cursor/rules/changeset-doc.mdc), [.cursor/rules/prd-doc.mdc](../../.cursor/rules/prd-doc.mdc), [.cursor/rules/dev-doc.mdc](../../.cursor/rules/dev-doc.mdc), [.cursor/rules/feature-doc.mdc](../../.cursor/rules/feature-doc.mdc)
- **Commands**: `/update-context-docs`, `/pr-wrap`
- **Stack**: Commands `/pr-stack-rebase`, `/pr-wrap` (step 8: readiness gate, title, `gh pr ready`) · Docs the `pr-stack` skill (`.agents/skills/pr-stack/SKILL.md`), [changelog-merge-hygiene.md](../../docs/dev/guides/changelog-merge-hygiene.md)
