---
name: jev-restructuring
description: Rank Rust symbols for restructuring by sweeping them past Jev (TypeSafe System One), then hand the survivors to code-restructuring. Use when choosing what to restructure across a whole crate, rather than verifying a shortlist by hand.
---

# Jev Restructuring (Rust) — targeting by fan-out

**Probe stage.** Pass 1 is backtested against this repo's own code-issue records; pass 2 is
known broken. Read [`references/questions.md`](references/questions.md) before trusting any
output. Do not promote this to `tddy-tools` until pass 2 is fixed and pass 1 has been run
against more than two files.

An alternative **targeting** pass to [`analyze-code-issues`](../analyze-code-issues/SKILL.md),
not an alternative to [`code-restructuring`](../code-restructuring/SKILL.md). It answers
*which symbols are worth restructuring*, over a whole crate, for fractions of a cent. It never
edits code, never writes a plan, and never decides whether a seam can be cut.

| | `analyze-code-issues` | `jev-restructuring` |
|---|---|---|
| Signal | CRAP — complexity × untestedness | semantic shape of the body |
| Blind to | a flat 1,700-line builder scores high | anything that needs counting |
| Verification | agent opens each function | agent opens the top ~15 |
| Cost for 49 functions | an agent read per function | $0.002, 7.7s |

**Run both.** They disagree usefully: CRAP flags `Presenter::new` (63 lines, 37 field
initialisations), Jev ranks it 49th of 49. The disagreement is the finding.

## Division of labour — do not blur this

| Question | Answered by |
|---|---|
| How long / how complex / how covered? | `tddy-tools analyze`. **Never Jev** |
| Is this tangled, or merely long? | Jev |
| Can this seam actually be cut? | `tddy-tools restructure check --deep`. **Never Jev** |
| Should we cut it? | you, reading both |

Jev decides *worth*, never *possible*. `--deep` already answers possible, deterministically,
and it is the only thing that can.

## Prerequisites

### The API key

**`TYPESAFE_API_KEY`** — the name TypeSafe's own SDKs use (`constants.API_KEY_ENV`). Either
export it, or put it in the **repo-root `.env`**, which is gitignored (`.gitignore:65`):

```
TYPESAFE_API_KEY=apikey_...
```

No `export ` prefix and no spaces around `=` — the repo's `.env` loaders take the literal text
left of the first `=` as the variable name, so `export FOO=bar` defines a variable called
`export FOO`. `scripts/jev-sweep.py` uses the same semantics as `./web-dev` and
`./run-vm-testkit`: **an already-set variable always wins**, so `.env` never silently overrides
what you exported. Keys: https://console.typesafe.ai/keys

`TYPESAFE_BASE_URL` and `TYPESAFE_DEFAULT_MODEL` are read from the same places if set.

⚠ **`.env` is the only file in this repo that may hold the key.** Never a changeset, a plan, a
code-issue record, a test fixture or a commit message. If a sweep output is pasted anywhere,
check it carries no key — it should not, but check.

### The rest

- A **green baseline** for the crate: `./test -p <crate>`. Do not target a red tree.
- For pass 3 only, a **warm index**: `eval $(./run-index-daemon | grep '^export ')`.

## Workflow

### 1. Scope it

A crate directory, a module, or one file. State the scope in the output. `--query` filters by
substring on path or symbol name.

```bash
scripts/jev-sweep.py classify packages/tddy-core/src/presenter --query presenter -o sweep.jsonl
```

Functions shorter than 12 lines are skipped — a restructure is not worth a request. Everything
under a `#[cfg(test)]` module is skipped.

### 2. Read the ranking, including the bottom

```
rank  score  p_problem  p_construction  category           symbol
1     0.782  0.93       0.03            tangled_dispatch   handle_intent
...
49    -0.51  0.03       0.97            long_but_flat      new
```

- **`p_problem` is the gate** (default 0.70). Below it, not a candidate at any score.
- **`category: unsure`** means the choice confidence fell under 0.60 — the model is telling you
  the options do not fit. That is information, not noise; those are worth a human look.
- **Read the bottom of the list too.** A function you expected to be flagged and which scored
  near zero is either a question that missed or an assumption of yours that was wrong.

### 3. Verify by hand — mandatory, and the same rule as `analyze-code-issues`

**An unverified finding is a lead, not an issue.** Open the top ~15, confirm the category
describes what the function actually does, and record what the sweep got wrong in
`references/questions.md`. A ranking is not evidence.

Everything a record needs beyond this point — the measurement, the metrics line — comes from
`tddy-tools analyze`, not from here. Jev output is **not** a `**Detected:**` measurement: it is
not reproducible run to run in the way a CRAP score is, and a code-issue record's contract is a
number anyone can re-derive.

### 4. Hand off

Verified candidates go to [`code-restructuring`](../code-restructuring/SKILL.md) at step 2
(targeting), with the CRAP note beside the Jev note. That skill owns anchors, the plan, the
snapshot and `--deep`. **Do not write a plan here.**

For anything worth persisting, write a record per
[`deferred-work`](../deferred-work/SKILL.md) — after re-measuring it with `analyze`.

### Pass 2 — propose (⚠ broken, see references/questions.md)

```bash
scripts/jev-sweep.py propose sweep.jsonl --top 20
```

Returns `extract_method` for nearly everything, because three of the five operations anchor on
item groups or `impl` blocks and cannot apply to a function body — so the Choice collapses and
picks the least-bad survivor. **Treat the output as noise** until the scope-matched option sets
described in `references/questions.md` are built.

## Rules

- **Never feed Jev a count, and never ask it for one.** Lines, fields, methods, nesting,
  complexity, coverage — all of it comes from `tddy-tools analyze`.
- **Never let Jev gate a merge, close a record, or delete anything.** It ranks; people and
  numbers decide. `/pr-wrap` step 7.5 deletes on a re-measured number, never on a judgement, and
  that stays true.
- **Never promote a proposal straight to a plan.** `anchors` → `snapshot` → `check --deep`, in
  that order, in `code-restructuring`. Prove before you pay.
- **Never widen the state to make an answer better.** Accuracy falls as the state grows with
  material the question does not need. If a question needs more context, it is the wrong
  question at that scope.
- **The weights are the criteria.** They live in one block at the top of `scripts/jev-sweep.py`.
  Change a weight before you change a question; changing prose to chase a result is how a
  question set stops meaning anything.

## Cost

$0.042 per million input tokens, output free, questions batched against one state.

| Scope | Tokens | Cost | Wall |
|---|---|---|---|
| 49 functions (`presenter_impl.rs`) | 47,403 | $0.0020 | 7.7s |
| 6 functions (`telegram_bot.rs`) | 8,812 | $0.0004 | ~2s |
| ~1,250 functions (`tddy-core`, projected) | ~1.2M | ~$0.05 | ~3 min |

Cost is not the constraint at any scope this repo has. **Question quality is.**

## References

- [`references/questions.md`](references/questions.md) — the criteria, the backtest, the failures
- [`code-restructuring`](../code-restructuring/SKILL.md) — where verified candidates go
- [`analyze-code-issues`](../analyze-code-issues/SKILL.md) — the CRAP pass; run both
- [`deferred-work`](../deferred-work/SKILL.md) — the record format
- Jaggedness: https://docs.typesafe.ai/model-jaggedness/jev-1.13
