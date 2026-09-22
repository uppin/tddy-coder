# The question set — and what it measures

This file is the review surface. The questions below, and the weights in
`scripts/jev-sweep.py`, **are** the repo's restructuring criteria written down. Jev is not
fine-tuned and carries no memory of this codebase; the only channels are `state`,
`instructions`/`criteria`, and composition in code. Nothing here is learned — it is
transcribed, and it is wrong until a backtest says otherwise.

Change a **weight** before you change a question. A question that needs rewriting is a
question whose criteria were never written out; a weight that needs changing is a priority
that shifted.

## The count/judgement line

| Half | Owner | Never |
|---|---|---|
| Lines, fields, methods, nesting, params, complexity, CRAP | `tddy-tools analyze`, a parser | **Never Jev** |
| "a reader must trace several branches", "the arms carry substantial logic", "no decisions to make" | Jev | Never a regex |

This repo already has the scar that proves the first row. The `god-object-presenter` record (closed
by #495; the note survives in `docs/dev/changesets/2026-09-22-carve-presenter-split.md`) recorded an
earlier pass that reported **44 fields and 79 methods** for a struct with **37 and 46** — the
fields were eyeballed and the `grep -c` ran over the test module. Jev counts worse than that
pass did. Jaggedness #2 is not a style preference here.

## Pass 1 — classify (VALIDATED)

One request per function. All five questions batch against one `state`, so the four beyond the
gate cost their own question text and nothing more — the speculative fan-out pattern.

| id | type | asks |
|---|---|---|
| `has_structural_problem` | noul | Must a reader hold several branch conditions in mind at once? **This is the gate.** |
| `dispatch_shape` | noul | Does it route on a matched value, with substantial logic in the arms? |
| `pure_construction` | noul | Is it assembling a value with no branching? **Negative weight** — this is the false-positive detector. |
| `contiguous_seam` | noul | Is there a run of statements whose values the rest of the body never reads again? |
| `category` | choice | `god_function` · `tangled_dispatch` · `long_but_flat` · `pure_mapping` · `none` |

The category options are the taxonomy already in `packages/*/docs/code-issues/` filenames, and
their descriptions are lifted from those records' *"Why it matters here"* sections.

### Backtest, 2026-09-18, `jev-1.13.0`

**`packages/tddy-session-lifecycle/src/telegram_bot.rs`** — labels from
`crap-telegram-bot-handlers.md` (CRAP 7,832 and 2,756, both untested):

| rank | score | p_problem | category | symbol |
|---|---|---|---|---|
| **1** | 0.751 | 0.92 | tangled_dispatch | **`telegram_callback_handler`** ← labelled worst |
| 2 | 0.631 | 0.88 | tangled_dispatch | `maybe_dispatch_tcp_chain_parent_callback` |
| **3** | 0.599 | 0.84 | tangled_dispatch | **`telegram_message_handler`** ← labelled second |
| 6 | −0.094 | 0.27 | none | `run_telegram_bot` |

Both labelled functions in the top 3. Rank 2 is **not** in any record — either a finding the
structural audit missed or a false positive; **unverified**.

**`packages/tddy-core/src/presenter/presenter_impl.rs`** — 49 functions, 7.7s, $0.002:

| rank | score | p_problem | p_construction | category | symbol |
|---|---|---|---|---|---|
| 1 | 0.782 | 0.93 | 0.03 | tangled_dispatch | `handle_intent` |
| 2 | 0.766 | 0.91 | 0.04 | tangled_dispatch | `poll_workflow` |
| 3 | 0.757 | 0.91 | 0.04 | tangled_dispatch | `try_handle_start_slash_line` |
| **49/49** | **−0.51** | **0.03** | **0.97** | long_but_flat | **`Presenter::new`** |

`Presenter::new` is the discriminating case: 63 lines of 37 field initialisations, which every
length-based and CRAP-based metric flags and which is **not refactorable**. It ranked last.
Separating *long* from *tangled* is the entire value of this pass.

`handle_intent` was verified by hand: a `match intent` whose arms perform file writes, channel
sends and nested error handling. The classification is correct.

## Pass 2 — propose (⚠ NOT VALIDATED — KNOWN BROKEN)

**Do not act on pass 2 output.** The backtest on the presenter's top 8 returned
`extract_method` for six of eight units at confidence 0.52–0.77, and `p_worth_acting` sat in a
0.63–0.80 band with no separation.

The cause is a design error, not a model limit. Of the five operations offered:

- `extract_module` anchors a **range over a selection of items**
- `extract_trait` anchors a **caret on an `impl` keyword**
- `inline_method` needs a symbol with one caller

None of the three can apply to a single function body. So the option set collapses to
`extract_method` versus `leave`, and a Choice is **relative** — it settles *which*, never
*whether* (jaggedness #8). It picks the least-bad survivor and reports moderate confidence
doing it.

**The fix, when this is next worked on:** scope-matched option sets, which means separate
sweeps rather than one.

| Scope | State | Operations |
|---|---|---|
| function body | the body | `extract_method` · `inline_method` · `leave` |
| `impl` block | signatures + doc comments | `extract_trait` · split impl · `leave` |
| file / item group | item list + coupling facts | `extract_module` (± `to_file`, `reexport`) · `leave` |

And the *whether* must move out of the Choice into a Noul gate, the way pass 1 does it — the
pattern their skill-suggestion cookbook uses: Noul decides whether to act, Choice picks which.

This is the same "you cannot aggregate upward" constraint from the design discussion,
resurfacing as a concrete bug: a module-scope verdict is not a rollup of its functions' scores,
and a module-scope *operation* cannot be chosen from a function-scope state.

## Questions that failed, and why

Keep these. A failed question is the cheapest thing in this file.

**`separable_block`** (first draft, discarded):

> "The body contains a run of consecutive statements that read and write only values declared
> inside that run, and could be lifted into a helper without adding parameters beyond a handful."

Scored **0.65 vs 0.63** on the labelled pair — no separation at all. Two defects, both
self-inflicted:

1. **Compound** — "reads and writes only values declared inside" *and* "could be lifted without
   adding parameters" are two judgements in one question.
2. **"a handful"** is a count wearing a word.

Replaced by `contiguous_seam`, which asks one thing and counts nothing.

## The repair loop

Jaggedness #1: *when you look at a wrong answer and find yourself explaining what you really
meant, that explanation is the missing half of the instruction.* Put the explanation in
`criteria`, re-run the backtest, and record the delta here. The 16 records in
`packages/*/docs/code-issues/` are the label set; they are small, hand-verified, and the only
ground truth this repo has.
