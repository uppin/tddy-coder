# Changeset: pr-stack PR boundary rule

**PRD**: `docs/ft/coder/pr-stacking.md` § PR boundary contract
**Branch**: `feature/session-attach-docs/attach-proto`

The pr-stack planning agent had **no** guidance on PR size or self-containment — the only normative constraints on splitting were dependency ordering, parallelism, and branch naming. So it could legitimately plan "node 1: add the proto RPCs / node 2: implement them", producing a PR that ships surface with no behavior: unreviewable for correctness, untestable beyond compiling, and leaving a contract in the tree that misrepresents the system.

This is not hypothetical — the attachments work in PR #351 was planned exactly that way, which is what prompted the fix.

## Checklist

- [x] `pr_stack/hooks.rs`: "Scoping rules" block in `analyze_stack_system_prompt`
- [x] `pr_stack/hooks.rs`: restated "Scoping rules" in `write_stack_plan_system_prompt` (re-run on every chat refinement)
- [x] `pr_stack/mod.rs`: one-sentence rule in the non-orchestrate `orchestration_system_prompt` fallback
- [x] `plan_pr_stack/prompt.rs`: doc comment marking the superseded copy and pointing at the live one
- [x] `docs/ft/coder/pr-stacking.md`: PR boundary contract section + `analyze-stack` cross-reference
- [x] Test pinning the rule at the `before_task` seam
- [ ] Machine enforcement in `validate_stack_plan` — **deliberately not attempted** (see Design decisions)

## Files modified

| File | Change |
|------|--------|
| `packages/tddy-workflow-recipes/src/pr_stack/hooks.rs` | Scoping rules in both planning system prompts; new `pr_boundary_scoping_rule_tests` module |
| `packages/tddy-workflow-recipes/src/pr_stack/mod.rs` | Self-containment sentence in the fallback planning prompt |
| `packages/tddy-workflow-recipes/src/plan_pr_stack/prompt.rs` | Doc comment: superseded copy, port the rule before reviving |
| `docs/ft/coder/pr-stacking.md` | New "PR boundary contract: every node is self-contained" section |

## Design decisions

**Hard rule with two named exceptions.** A mechanical rename/move with no behavior change, and regenerating already-committed codegen with no new surface. Naming them explicitly matters more than the rule itself: an agent told only "never split" will invent its own exceptions when a slice looks large, so the prompt closes that door and routes a third case to a human via the node `description`.

**Paired with the correct split axis.** A prohibition alone would leave an oversized vertical slice nowhere to go, and the agent would layer-split anyway. The rule is stated together with the alternative — split by **capability** (one source variant, one enum case, one screen, happy path first), each part still end-to-end.

**Stated in both planning prompts, on purpose.** `write-stack-plan` is the goal `plan_refinement_goal()` returns, so it re-runs on every chat-driven refinement. A rule living only in `analyze-stack` would be silently dropped the first time an operator refined the plan — and the refinement prompt explicitly says a refinement request must not talk the agent into a layer-split stack.

**Advisory, not enforced.** `validate_stack_plan` checks graph shape and branch naming; it cannot distinguish a vertical slice from a layer split without understanding the diff a node will produce. Adding a keyword heuristic ("reject a description containing 'proto only'") would be trivially gamed and would reject legitimate plans, so the rule is prompt-carried and the PRD says so plainly rather than implying a gate exists.

**Test drives the `before_task` seam, not the constants.** `seeded_system_prompt` runs the real `before_analyze_stack` / `before_write_stack_plan` and reads `system_prompt` off the `Context`, so the test fails if a future refactor stops delivering the prompt — not merely if the string changes.

## Known limitation

The rule is guidance to a model, so it constrains behavior without guaranteeing it. A planning agent can still emit a layer-split stack; the PRD's `description` escape hatch is what surfaces that to a human reviewer. If layer splits keep appearing in practice, the next step is a plan-review gate rather than a validator regex.
