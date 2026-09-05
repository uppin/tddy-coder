# Report template

The shape of `tmp/eval-changeset/<slug>/report.md`. Every `<…>` is filled from a command that ran;
a measurement that was skipped is written as **skipped**, never as a zero or an estimate.

Keep the reply to the user short — the verdicts, the headline numbers, the per-node table if it is a
stack, and the ranked proposals. The document is where the evidence lives.

```markdown
# Changeset evaluation — <slug>

**Shape:** <single PR #N | stack of N PRs, `#<stack-slug>`>
**PRs:** <K/N — #n branch — title>, … <; note any member that could not be recovered>
**Base (fork point):** <sha> — <n> trunk commits have landed since
**Evaluated tree:** eval/<slug> @ <sha> <— note if reconstructed from a branch out of line,
or built from local unpushed refs>

## Verdict

**Complexity: <low | moderate | high | very high>** — <one sentence>
**Justification: <justified | justified but oversized | under-scoped | unjustified>** — <one sentence>
**Increments: <clean | some rework | heavy rework>** — <one sentence; single PR ⇒ not applicable>
**Cause of rework: <planning | system | discovery | linearization | mixed>** — <one sentence>
**Design: <carried the change | mixed | fought the change>** — <one sentence>

| | |
|---|---|
| Production lines | +<a> / -<b> across <n> files |
| Essential share | <p>% of production lines |
| Incidental share | <q>% — <the friction site that caused most of it> |
| Stack inflation | <x>× — <y>% of the work never reached the final tree |
| Reviewable in | ≈<h>h |

## 1. Intent

> <one paragraph of intent, quoted or summarised>

**Source:** <docs/dev/1-WIP/<date>-<slug>.md § Responsibility | docs/dev/changesets/… (wrapped) |
docs/ft/… PRD | PR #N body | **inferred from the diff**>

<If inferred: a reconstructed intent always fits its change, so the justification verdict below is
weak evidence and should be read as such.>

<Boundary check: any `## Boundaries` line the diff crosses, or "no stated boundaries were crossed".>

## 2. Size

| Role | Files | + | - |
|---|---|---|---|
| Production | | | |
| Test | | | |
| Docs | | | |
| Build & config | | | |
| *Generated (excluded)* | | | |
| **Judged total** | | | |

**Packages touched:** <n> — <list>
**New / deleted / renamed files:** <a> / <b> / <c>
**Concentration:** <top dirs from --dirstat>
**Commits before squash:** <n>

## 3. Complexity

| Axis | 1-5 | Evidence |
|---|---|---|
| Conceptual load | | <n new types/traits/states — name them, file:line> |
| Control-flow depth | | <file:line> |
| Blast radius | | <n sites to understand one behaviour change> |
| Reversibility | | <behind a seam | wire format | on-disk | public API | installed unit> |
| Review cost | | <≈h hours; whether node boundaries reduced it> |

## 4. Per-node increments

<Single PR: "Not applicable — single PR." and nothing else.>

| PR | Own +/- | Files | Overlap | Rework | Rework % | Rewrote |
|---|---|---|---|---|---|---|
| 1/N `<branch>` | | | | | | |
| … | | | | | | |
| **Integrated** | | | | | **inflation <x>×** | |

**Ideal:** inflation 1.0 — every PR a pure increment, no predecessor's lines rewritten.
**Measured:** <x>× — <n> lines of the stack's work did not survive into the final tree.

### Rework sites

- **PR <i>/N rewrote <n> lines of PR <j>/N** in `<files>` — <what was replaced, in one line>
  **Cause: <planning | system | discovery | linearization>** — <would a different cut or order have
  avoided this? name it, or say why none would have>

<Repeat per site. Then: which cause dominates by lines, and why.>

### Boundary contract

<PRs that violate `pr-stack`'s contract independently of rework: a types-only or stubs-only PR, a PR
implementing a surface its `## Dependencies` assigns to a predecessor, a PR not reviewable without
its successor, a document missing one of the four required headings, or a chain never registered with
`gh stack link`. Or: "every PR was independently reviewable and independently mergeable.">

## 5. Essential vs incidental

<Classifies the *integrated* diff — intra-stack rework has already cancelled out of it, which is why
§ 4 is measured separately.>

| Bucket | Files | Lines | Share |
|---|---|---|---|
| Essential | | | |
| Enabling | | | |
| Incidental | | | |
| Opportunistic | | | |

**Essential:** <file> — <why removing it would leave the intent undelivered>
**Enabling:** <file> — <the seam it built, and why it had to come first>
**Incidental:** <file> — <the tax, and which friction site in § 7 imposed it>
**Opportunistic:** <file> — <unrelated cleanup that rode along>

## 6. Justification

<Whether the size and complexity above are warranted by the intent in § 1. Cite the essential share,
the opportunistic share, and — for a stack — the inflation figure and whether the node boundaries
followed the intent's own seams or just the layers. A layer split is a boundary-contract violation,
not a style preference.>

<If a smaller change would have delivered the same intent, say concretely what it would have been.>

<If the change is smaller than the problem — a fallback added without consent, a test-only branch in
production code, a TODO standing in for the hard half — that is under-scoped, and belongs here, not
in a footnote. CLAUDE.md forbids the first two outright.>

## 7. How the design served the change

### Carried it

- **<seam>** (`<file>`) — <what it absorbed, and what that saved>

### Fought it

- **<pattern>** — <evidence: files, counts> — cost: ≈<n> incidental lines<, plus <m> rework lines in § 4>
  <one sentence on the mechanism: why the design forced this>

<If nothing fought the change, say so. It is the best possible finding and should not be padded.>

## 8. Proposals

### Planning — where the stack should have been cut

<Only if § 4 attributed rework to planning. Name the nodes, their responsibilities, and the rework
each would have avoided. Costs nothing to adopt; applies to the next stack.>

- **Instead of `<2/3: service> → <3/3: ui>`, cut `<2/3: feature A end to end> → <3/3: feature B end
  to end>`** — avoids <n> lines of rework, because <reason>. <The proposed cut is linear: a branching
  one cannot be registered as a stack.>

### Redesign — ranked by (lines saved next time × recurrence) ÷ migration cost

#### P1 — <name>

- **Deficiency:** <friction site from § 7, quantified from § 5 and § 4>
- **Proposal:** <the seam, in one sentence, naming the types or modules it creates>
- **Counterfactual:** this changeset would have been ≈<a> files / ≈<b> lines instead of <c> / <d>
  <; PRs N and M would not have existed, and PR <i>'s rework of PR <j> disappears>
- **Cost:** <migration size, call sites, what breaks, whether it can be incremental>
- **Risk of not doing it:** <what the next change of this shape pays>
- **Verdict:** <do now | do before the next change of this shape | not worth it — because …>

#### P2 — …

## 9. What was not measured

<Deep passes skipped and why: /analyze-clean-code, tddy-tools analyze CRAP, per-package tests. Also
anything excluded from the tree — uncommitted work, generated files, a leaf whose conflict was
resolved by hand, a PR whose boundary had to be reconstructed, a formatter run that may inflate
the rework figures.>
```
