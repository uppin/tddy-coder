# The restructure changeset

A restructure's changeset follows `changeset-doc.md` — same location
(`docs/dev/1-WIP/YYYY-MM-DD-<name>.md`), same lifecycle, same wrap. This page covers only where a
behaviour-preserving change fills a section differently from a feature. Anything not named here
follows `changeset-doc.md` unchanged.

The document exists to be read *instead of* the diff. A twenty-file split is unreviewable after the
fact, so the layout, the seam order and the baseline all have to be on paper before the executor runs.

## What differs from a feature changeset

| Section | A restructure |
|---|---|
| **Initial Discovery** | Required. Companion `{slug}-initial-discovery.md` from steps 1–3 (LSP outline, reference queries, grep of callers, seam checks). Link it as the first content section after the header. |
| **Type** | `Refactor`. Not `Feature`, even when the split is large. |
| **Related Feature Documentation** | `None — behaviour-preserving restructure.` State it explicitly; do not go looking for a PRD to link, and do not write one. |
| **Summary** | What moves and what stays reachable. The reachability half is the part reviewers check. |
| **Background** | Why this file is a problem *now* — size, the seam that exists, the class-member blind spot from step 2. |
| **Scope** | Testing means the recorded baseline re-run at zero regressions. There are no acceptance tests (see the rule in `SKILL.md`). |
| **Technical Changes** | State A is the tree as it is, by line and symbol count. State B is the layout table. The Delta is the seam list in dependency order. |
| **Callers** | Required. The files *outside* the target that the restructure rewrites, and whether a facade absorbs them. This is the blast radius, and it is not inferable from the layout. |
| **Testing Plan** | Replaced by **Baseline**. There is no strategy to choose; there are numbers to match. |
| **Acceptance Tests** | Omitted. |
| **Decisions & Trade-offs** | Carries the outliers and refusals — a 780-line function no `move_symbol` shrinks, a member the vocabulary cannot reach, a facade kept for reflection. These are the developer's calls and they belong in writing. |
| **Final Checklist** | Required, not optional, and derived at plan time. See below. |

## Skeleton

```markdown
# Changeset: {What is being restructured}

**Date**: YYYY-MM-DD
**Status**: 🚧 In Progress
**Type**: Refactor

## Initial Discovery

Full codebase exploration that grounded this plan: [initial-discovery.md](./YYYY-MM-DD-<name>-initial-discovery.md).

State A below is distilled from that file. Do not duplicate grep traces or file dumps here.

## Affected Packages

- **@wix/{package}**: [README.md](...) — {one line: what a reader of these docs would notice}
  - {doc file} — {section}, or `no dev-doc change: the facade keeps every documented entry point`

## Related Feature Documentation

None — behaviour-preserving restructure. No PRD.

## Summary

{What moves, and what stays reachable from outside.}

## Background

{Why now: size, the seam, what step 2 turned up.}

## Scope

- [ ] **Plan**: layout proposed, seam order fixed, snapshot taken
- [ ] **Apply**: dry run clean, plan applied, per-seam checks green
- [ ] **Baseline**: build and full suite back to the recorded numbers
- [ ] **Code Quality**: `yarn lint:fix` and `yarn type-check` clean
- [ ] **Documentation**: Final Checklist executed at wrap

**Status indicators**: `[ ]` not started · `[~]` in progress · `[x]` complete ✅

## Technical Changes

### State A

| File | Lines | Symbols |
|---|---|---|
| `src/pdf/PdfHelper.ts` | 5193 | 69 static members |

### State B

| Module | Symbols | Projected lines | Crossing edges |
|---|---|---|---|
| `helpers/svg-pdf-generation.ts` | 7 | ~350 | → `text-layout-pdfs` |
| `helpers/text-layout-pdfs.ts` | 8 | ~110 | → `svg-pdf-generation` |
| `helpers/layer-test-pdf.ts` | 1 | ~780 | ⚠ single function, over budget |

### Delta — seams in dependency order

1. `{seam}` — `{op}` — defines `{symbols}`
2. …

Definitions before their users; line order only breaks ties.

## Callers

What the restructure rewrites outside the target:

| | |
|---|---|
| External importers | {n} files across {packages} — or *none, the seam is package-internal* |
| Facade | `reexport: "glob"` on `{module}` / none (TypeScript has no facade operation) / one export line added by hand after apply |
| Consequence | {no caller changes} / {n files in the diff, listed below} |

{List the caller files when there are few enough to read. When the facade is a hand-added export line,
say whether it is the package's permanent public surface or scaffolding to be removed, and by whom.}

## Visibility

Every widening the run reported, and what each one costs. Rust only — the assist rewrites what it
relocates to `pub(crate)`, and the backend puts back only what nothing outside the new module reaches.

| Item | Was | Is | Why it had to widen | Doc comment fixed |
|---|---|---|---|---|
| `{item}` | private | `pub(crate)` | {the reference that needs it} | {file:line, or n/a} |

{Zero widenings is the good answer and worth stating: every seam kept its private helpers with their
callers. Where an item did widen, say whether its privacy was enforcing an invariant — a field that
was private specifically to force mutations through one method is an invariant the compiler used to
hold and now only prose does, and the prose is usually now wrong.}

**Then the moved-line diff, which is a different check.** Normalise `pub(crate)` and whitespace away
and set-compare every moved line against `HEAD`; state how many lines differ and why each one does.
Run it *alongside* the table above, never instead of it — normalising `pub(crate)` away is exactly
what hides the rows.

## Baseline

Recorded before any change (step 1), and the acceptance criterion for step 7:

| Gate | Before | After |
|---|---|---|
| `yarn build` | ✅ | |
| `yarn test` — {package} | {n} passed / {m} failed | |
| `yarn type-check` | ✅ | |

{Name any pre-existing failure here, so it cannot later read as a regression.}

## Plan

`{path}/plan.jsonl` — {n} operations. Snapshot digests are in its header line.

## Decisions & Trade-offs

- **{Outlier or refusal}** — {what the tool cannot reach, and what was decided instead.}

## Final Checklist

Tasks executed by `/wrap-context-docs` before this changeset is deleted.

- [ ] {package}/docs/changesets/YYYY-MM-DD-short-description.md — write release-note file (see below)
- [ ] {doc file, section} — {the statement the restructure made false → what it should say}
```

## Deriving the Final Checklist

This section is the whole reason the changeset exists at wrap time, and it is filled in at **plan**
time — while both the old names and the new ones are in hand. After apply, the docs still say the old
thing (the executor never touches prose) and you would be reverse-engineering which of it was true.

**A restructure falsifies documentation of structure, and nothing else.** Behaviour did not change, so
prose describing behaviour is still correct. The four things that go stale:

1. **File and module maps** — a listing of what lives where.
2. **"Where to add X" instructions** — the file named is no longer the file.
3. **Surface listings** — a table of a class's members, when members left the class.
4. **Code examples and diagrams** — an import path or a file name that no longer resolves.

Find them mechanically, over every path and symbol the plan touches:

```bash
grep -rn -e '<MovedSymbol>' -e '<old/path>' \
  <pkg>/README.md <pkg>/AGENTS.md <pkg>/docs
```

Then triage each hit:

- **False after the restructure** → one checklist item, named to the file and the section, saying what
  it should say. Not "update the docs".
- **Still true** → not an item. A facade that keeps `PdfHelper.convertSvgToPdf` reachable leaves a
  README example running exactly as written, and editing it anyway is churn.
- **Nothing false** → the checklist is the changelog entry alone. This is a complete and common
  outcome; do not manufacture doc work to make the wrap look substantial.

Re-run the triage in step 8 against what actually landed. A refused seam or an added facade changes the
answer, and the checklist is executed against the tree, not against the proposal.

## The changelog entry

Always written, as a **new file** in each affected package's `docs/changesets/YYYY-MM-DD-short-description.md` — or the package README's history where
the package has no `docs/`. Release-note style, past tense, and carrying the evidence:

```markdown
# `PdfHelper.ts` 5193 ➜ 278 lines

**Date:** 2026-08-21
**Type:** Refactor
**Packages:** @wix/pdf-test-helpers

A facade over 20 modules under `src/pdf/helpers/`, grouped by what they build. `PdfHelper` stays a
class because `PdfHelperWithFormXObjects` copies its statics with `Object.getOwnPropertyNames` +
`getOwnPropertyDescriptor`, which depends on them being non-enumerable own properties — an object
literal would have read better and been silently wrong. Every moved body was sliced from the AST,
none retyped. Behaviour unchanged: 1270 tests green before and after, build green both sides.
```

Three things a reviewer looks for and should not have to derive: **what moved**, **what deliberately
did not**, and **the before/after numbers behind the no-behaviour-change claim**. A restructure
changelog entry that omits the numbers is asserting the thing it was supposed to demonstrate.
