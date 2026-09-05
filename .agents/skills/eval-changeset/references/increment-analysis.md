# Per-node increment analysis

The mechanics behind [`SKILL.md`](../SKILL.md) § 5. The question: **did each PR of the stack add to
what its predecessors built, or did it rewrite their work?**

A perfect stack is a set of disjoint increments — every line the changeset ships was written once,
reviewed once, and survived. Every deviation from that costs twice: the author wrote a version that
did not last, and a reviewer approved it.

## Find each PR's own starting point

**Do not assume a PR forks from its predecessor's final tip.** Predecessors get rebased and amended
after their successors are cut, so the recorded base ref describes intent, not history. Find the real
integration point — the commit just before this PR's first *own* commit:

```bash
# for PR N with predecessor P
FIRST_OWN=$(git rev-list --reverse "origin/$N" --not "origin/$P" | head -1)
INTEGRATION=$(git rev-parse "$FIRST_OWN^")
```

Two caveats:

- If `$FIRST_OWN` is a **merge** (the branch pulled its base in rather than rebasing onto it), that
  merge *is* the integration point — take it, and re-derive from the next own commit. `^` on a merge
  silently follows the first parent only.
- If the PR was squashed or rewritten so it shares no history with its predecessor,
  `git merge-base "origin/$P" "origin/$N"` is the fallback. Say in the report that the PR's boundary
  was reconstructed.

A PR's **own diff** is then `git diff "$INTEGRATION".."origin/$N"`.

## The three figures

### Inflation — the whole-stack headline

```bash
# per PR
git diff --numstat "$INTEGRATION".."origin/$N" | awk '{a+=$1; d+=$2} END {print a+d}'
# integrated
git diff --numstat "$BASE"..eval/<slug>       | awk '{a+=$1; d+=$2} END {print a+d}'
```

`inflation = Σ per-PR changed lines ÷ integrated changed lines`

- **1.0** — every PR a pure increment. The ideal.
- **1.1–1.2** — normal: some churn from review feedback and rebases.
- **> 1.3** — a third of the work never reached the final tree. Find out why; that is § 5's verdict
  question.

Inflation is cheap, needs no blame, and is the number to compute first. It says *how much* rework
happened; the blame pass below says *where* and *whose*.

**Watch for a formatter.** If `cargo fmt` or a lint autofix ran mid-stack, reformatted lines count as
rework. Re-run the numstats with `-w` and report both if the gap is large.

### File overlap — the cheap pre-filter

```bash
git diff --name-only "$INTEGRATION".."origin/$N" | sort > /tmp/eval/$N.files
cat /tmp/eval/<each predecessor>.files | sort -u > /tmp/eval/$N.pred
comm -12 /tmp/eval/$N.files /tmp/eval/$N.pred          # files this node re-entered
```

Overlap alone is **not** rework — a PR that appends to a module a predecessor created is doing
exactly what a stack is for. Overlap is the list of files worth blaming.

### Rework — the direct measure

Only **removed or replaced** lines can be rework; pure insertions never are. A hunk header
`@@ -a,b +c,d @@` with `b = 0` is a pure insertion — skip it.

```bash
F=<overlapping file>
git diff -U0 "$INTEGRATION".."origin/$N" -- "$F" | grep '^@@'
# for each hunk with b > 0, blame those old lines at the integration point:
git blame -L "$a,+$b" --porcelain "$INTEGRATION" -- "$F" \
  | awk '/^[0-9a-f]{40} /{print $1}' | sort -u
```

Then classify each blamed commit — did it come from the trunk, or from this stack?

```bash
git merge-base --is-ancestor "$sha" "$BASE" \
  && echo "pre-existing code"        \
  || echo "REWORK — written inside this stack"
```

Attribute the in-stack ones to the PR that wrote them: the lowest `Ni` in the stack for which
`git merge-base --is-ancestor "$sha" "origin/$Ni"` holds.

Report per PR: **rework lines**, **rework share** (rework ÷ own changed lines), and **which
predecessor** the rework landed on. "PR 5 rewrote 180 of PR 2's lines" is the sentence this whole
reference exists to let you write.

## The summary table

One row per PR in stack order, integrated total last — so the comparison is on one screen:

| PR | Own +/- | Files | Overlap | Rework | Rework % | Rewrote |
|---|---|---|---|---|---|---|
| 1/3 `feature/x/store` | +420 / -12 | 6 | — | 0 | 0% | — |
| 2/3 `feature/x/service` | +310 / -95 | 9 | 4 | 78 | 19% | 1/3 |
| 3/3 `feature/x/ui` | +260 / -8 | 5 | 1 | 0 | 0% | — |
| **Integrated** | **+890 / -40** | **17** | | | **inflation 1.24** | |

## Reading it

Once the numbers exist, [`SKILL.md`](../SKILL.md) § 5 assigns each rework site to **planning**,
**system**, **discovery** or **linearization**. The discriminating test is a single question, asked
per site:

> Would a *different* cut of the same work have avoided this rework?

- **Yes, and the better cut is nameable** → planning. Say what the cut should have been.
- **No — any decomposition hits this file** → system. It becomes a § 7 friction site and earns a
  redesign proposal.
- **Only in hindsight, because the requirement appeared later** → discovery. Report what would have
  surfaced it earlier, and exonerate both the plan and the design.
- **No, because the two PRs were independent work flattened into a line** → linearization. `gh stack`
  models a line, so this is the price of a reviewable stack — unless another linear order would have
  avoided it, which is worth checking and saying either way.

A stack with inflation near 1.0 needs none of this. Say it was a clean increment and move on — that
is the finding.
