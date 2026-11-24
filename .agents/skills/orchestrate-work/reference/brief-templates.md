# Brief templates

Fill every `<…>`. Keep the forbidden list verbatim. `<back channel>` is the child's reporting
channel — the exact route it reports on to you ([SKILL.md § 6.1](../SKILL.md#61-ask-for-reports-in-every-brief)).
Subscribe to the child's messages and its end right after starting it (§ 6.2).

## Shared footer (append to every brief)

```
REPORTING: via <back channel> only — no other route — on start, at each milestone, at once on any
blocker or decision, before every turn ends, and at least every <N> minutes: a one-line status.
Final: "✅ <PR> <sha> + full check counts (pass/fail)".
PRE-APPROVED: scoped build/test/lint, commits, leased pushes to <branch>, rebasing <branch>,
reading CI, deleting your own build outputs.
FORBIDDEN: merging; auto-merge / force-merge / force-build tokens; dispatching workflows;
unstacking or re-registering a stack; force-push (leased only, explicit SHA); killing processes
by pattern (only PIDs you recorded); reading .env or secrets; touching worktrees outside yours.
Finishing the planned scope is never a deferral — do not turn it into TODOs.
You are the main agent: do not hand the whole job to one subagent.
STOP and report if: the base moved unexpectedly, disk is under <floor>, a gate fails twice for the
same reason, or the task needs a decision outside its boundaries.
```

## Deliverable (one line in every brief — §3)

```
DELIVERABLE: <a PR READY FOR REVIEW — docs wrapped, marked ready, checks read; report
the PR number and CI state> | <a report over <back channel>, no PR>.
```

## Investigation (no change delivered)

```
TASK <title> — investigate / diagnose / answer: <question>. Work in <worktree>; change nothing.
DELIVERABLE: a report over <back channel>, no PR: findings, evidence (paths, logs, commands), and
what you recommend. Report "✅ <title>: <one-line answer>" and stop.
```

## [G] implement

```
TASK <title> — PR <#>, branch <branch>, base <base> @ <sha>.
Responsibility: <…>. Boundaries (do not implement): <…>.
Run /green (or equivalent) until the contract's failing tests pass. Scope every gate to touched
packages: <build/test/lint commands>. Commit and push each milestone (leased).
Done = scoped gates green and pushed. Report "✅ <PR> <sha>" and stop; do not start [W].
Model/effort: <model>/<effort> because <complexity reason>.
```

## [W]/[F] validate, wrap, fix

```
TASK <title> — PR <#>, branch <branch>, base <base> @ <sha>.
1. Rebase onto <base> if behind. 2. /validate-changes. 3. /pr-wrap (docs, cleanup), mark ready,
push. 4. Wait for predecessor <task> [W]+[F] to finish (orchestrator will tell you) — then
/fix-pr: poll once per push, fix failures as they appear, until THIS PR's check rollup is green
on the current head. Re-run cancelled runs. A flaky test is red: fix its race now.
Delete only count-only shape tests, one test at a time. Never delete behaviour tests.
Done = rollup green with full counts on head <sha>. ✅ <PR> <sha> + counts.
```

## Sweep

```
TASK sweep — PR <#>, branch <branch>, base <tail branch>.
Inputs: code issues added by the queue <list>, open TODOs <list>.
Close in place: oversized files, fixable TODOs, dead helpers. Re-run the analyzers; record the
final measurement. List every TODO you could not close with the reason — the operator decides.
Same stages and verification as any task. ✅ <PR> <sha> + counts.
```
