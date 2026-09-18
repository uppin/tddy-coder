---
description: Rank Rust symbols for restructuring by sweeping a crate past Jev, verify the top candidates by hand, and hand them to /code-restructuring
---

## Jev Restructuring — targeting a crate by fan-out

Sweeps every function in scope past Jev (TypeSafe System One), ranks them by semantic shape
rather than by size, and hands the verified survivors to `/code-restructuring`.

**Load the `jev-restructuring` skill (`.agents/skills/jev-restructuring/SKILL.md`) before
acting** — it owns the division of labour this command depends on, and the rules about what Jev
may and may not decide.

**This is a probe-stage tool.** Pass 1 is backtested against this repo's code-issue records;
**pass 2 is known broken**. Read `references/questions.md` before reporting any output.

## What this command does NOT do

- It does not edit code, write a restructure plan, or run an assist.
- It does not decide whether a seam can be cut — `restructure check --deep` does.
- It does not produce a `**Detected:**` measurement. Code-issue records take numbers from
  `tddy-tools analyze`; a Jev ranking is a lead, not a measurement.

## Prerequisites

- **`TYPESAFE_API_KEY`**, exported or in the gitignored repo-root `.env` as
  `TYPESAFE_API_KEY=apikey_...` (no `export ` prefix, no spaces around `=`). An exported value
  always wins over `.env`. `.env` is the **only** file that may hold it — never a changeset, a
  plan or a record.
- Green baseline: `./test -p <crate>`. Record pass/fail counts; do not sweep a red tree.

## Steps

### 0. Resolve the key

`scripts/jev-sweep.py` loads the repo-root `.env` itself, so there is nothing to source. If the
key is missing the script exits with the exact line to add and where to add it — **relay that to
the developer rather than working around it**, and never echo a key you find into the transcript.

### 1. Establish the baseline

```bash
./test -p <crate>
```

Scoped, per CLAUDE.md. State the counts in the output.

### 2. Sweep

```bash
scripts/jev-sweep.py classify packages/<crate>/src -o sweep.jsonl
scripts/jev-sweep.py classify packages/<crate>/src --query <substring> -o sweep.jsonl
```

Report: units in scope, how many cleared the gate, tokens, cost.

### 3. Run the CRAP pass alongside it

**Run `/analyze-code-issues` on the same crate.** The two disagree usefully — CRAP sees
complexity and untestedness, Jev sees shape. A flat 1,700-line builder scores high on CRAP and
last on Jev; an untested tangled dispatcher scores high on both. **The disagreements are the
finding**, and they belong in the targeting note.

### 4. Verify the top candidates by hand — MANDATORY

Open the top ~15 and the `category: unsure` rows. Confirm the category describes what each
function actually does. **An unverified finding is a lead, not an issue** — the same rule
`/analyze-code-issues` step 6 states, for the same reason.

Record what the sweep got wrong in `.agents/skills/jev-restructuring/references/questions.md`.
A failed question is the cheapest artifact in this workflow; losing it is the expensive part.

### 5. Read the bottom of the list

A function you expected to be flagged that scored near zero is evidence about the question set,
not about the function. Say so explicitly in the output.

### 6. Hand off — do not restructure here

Verified candidates go to `/code-restructuring` at its step 2 (targeting), carrying both the
CRAP note and the Jev note. That skill owns anchors, snapshot, plan and `--deep`.

Anything worth persisting becomes a record per the `deferred-work` skill — **after**
re-measuring it with `tddy-tools analyze`, never from the Jev score.

### 7. Targeting note

Summarise for the developer:

- Scope swept, units in scope, how many cleared the gate, cost and wall time
- Top candidates with score, `p_problem`, category — and **which were verified by hand**
- **Where Jev and CRAP disagree**, with the reading of each disagreement
- Rows the model marked `unsure`, and what the ambiguity was
- Any finding already **claimed** by an open PR — it stays open; say which PR
- Questions that misfired, and the wording change recorded
- **Hand off** — do not start restructuring in this command

## Output Format

```markdown
## 🎯 Jev Sweep — <crate>

**Baseline**: `./test -p <crate>` — N passed, M failed
**Scope**: <path> · <N> units · <K> above gate · <T> tokens · $<C> · <S>s

| rank | score | p_problem | category | symbol | verified |
|------|-------|-----------|----------|--------|----------|

### Jev ↔ CRAP disagreements
| symbol | CRAP | Jev | reading |

### Marked `unsure`
### Questions that misfired
### Recommendation
→ `/code-restructuring` on: <symbols>
→ Not worth restructuring: <symbols>, because <reason>
```

## Best Practices

✅ **Do:**
- Run `/analyze-code-issues` alongside and report the disagreements
- Verify the top candidates by hand before reporting them as issues
- Record misfired questions in `references/questions.md`
- Keep weights and thresholds in the one block at the top of `scripts/jev-sweep.py`

❌ **Don't:**
- Don't ask Jev for a count, or feed it one — that's `tddy-tools analyze`
- Don't let a Jev score close a record, gate a merge, or delete anything
- Don't write a restructure plan from a proposal — `anchors` → `snapshot` → `check --deep`
- Don't report pass 2 (`propose`) output as actionable; it is known broken
- Don't widen the state to improve an answer — that's the wrong question at that scope
- Don't write the API key into any file in the repo

## Related

**Skills**: `jev-restructuring`, `code-restructuring`, `analyze-code-issues`, `deferred-work`
**Commands**: `/analyze-code-issues` (run alongside), `/code-restructuring` (next step)
