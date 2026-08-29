pub const STACK_PLAN_BASENAME: &str = "stack-plan.yaml";
pub const PR_STACK_PLAN_MD_BASENAME: &str = "pr-stack-plan.md";

/// System prompts for the stack-planning steps, shared by **pr-stack** (`PrStackRecipe`, the
/// recipe every CLI name resolves to) and the superseded **plan-pr-stack**. `recipe` names the
/// workflow in the opening line; everything else — including the [PR boundary contract] (each node
/// self-contained: API + implementation + tests, never a layer split) — is identical for both, so
/// the two cannot drift apart.
///
/// [PR boundary contract]: ../../../../../docs/ft/coder/pr-stacking.md
pub fn analyze_stack_system_prompt(recipe: &str) -> String {
    "You are assisting with the **{recipe}** workflow **analyze-stack** step.\n\n\
     ## Task: Analyze PR stack decomposition\n\n\
     Analyze the feature request and determine how to decompose it into a stack of pull requests. \
     Consider dependencies between PRs and identify which can be built in parallel (DAG structure).\n\n\
     This is a **read-only** analysis phase — do not write code or create files. \
     Focus on understanding the feature scope and identifying the optimal PR decomposition strategy, \
     noting which PRs depend on others and which can be developed concurrently.\n\n\
     ## Scoping rules: every PR is self-contained\n\n\
     Each PR must stand on its own — a reviewer can judge it, and it can merge, without waiting for \
     a later node in the stack. That means one node ships the **API/schema change, the code that \
     implements it, and its tests, together**.\n\n\
     **Do not split by layer.** These are the same node, never two:\n\
     - a schema/proto/interface change and the behavior behind it\n\
     - a backend endpoint and the handler that serves it\n\
     - a data model and the migration or persistence that uses it\n\
     - a function signature and its body; a type and its consumers\n\n\
     A node that only declares surface — new RPCs that return `unimplemented`, a field nothing \
     reads, a trait with stub impls — is **not** a valid PR. It cannot be reviewed for correctness \
     (there is no behavior to check), it cannot be tested beyond compiling, and it strands a \
     contract in the codebase that lies about what the system does.\n\n\
     **When a vertical slice feels too large, split by capability, not by layer.** Cut along \
     user-visible increments where each part is still end-to-end: one source variant instead of \
     all of them, one scope/enum case instead of the full set, one screen or one entry point, the \
     happy path before the edge cases. Each such PR ships its own contract plus behavior plus \
     tests, and the next one extends it.\n\n\
     **The only exceptions** — a node may omit implementation when it is:\n\
     - a purely mechanical rename, move, or extraction with no behavior change, or\n\
     - regenerating already-committed generated code, exposing no new surface.\n\n\
     If you believe a case warrants a third exception, put the reasoning in the PR's description \
     and let a human decide — do not invent one silently.\n\n\
     For each proposed PR, identify:\n\
     1. A stable slug (`node_id`, e.g. `auth-store`, `api-client`)\n\
     2. A concise title\n\
     3. A description of what it implements\n\
     4. Its dependencies (which other PRs must merge first)\n\
     5. A branch name suggestion grouped under one shared stack namespace, \
     `feature/<stack-slug>/<node>` (e.g. `feature/auth/token-store`), so the stack's branches \
     group together\n\
     6. The child recipe to use (default: `tdd`)\n"
        .replace("{recipe}", recipe)
}

pub fn write_stack_plan_system_prompt(recipe: &str) -> String {
    r##"You are assisting with the **{recipe}** workflow **write-stack-plan** step.

## Task: Emit structured PR stack plan

Based on the prior analysis, deliver the plan by running:

  tddy-tools submit --goal write-stack-plan --data-stdin << 'EOF'
  <your JSON output>
  EOF

Use `--data-stdin` and a heredoc. Do NOT use `--data` with inline JSON. Do NOT use Write, cat, or
python to build the JSON first — only `Bash(tddy-tools *)` is auto-approved; other Bash commands
require permission.

Run `tddy-tools get-schema write-stack-plan` for the authoritative shape. The plan must conform to
this contract:

```json
{
  "goal": "write-stack-plan",
  "version": 1,
  "prs": [
    {
      "node_id": "n1",
      "title": "Auth token store",
      "description": "Store tokens securely in the keyring",
      "branch_suggestion": "feature/auth/token-store",
      "parents": [],
      "child_recipe": "tdd"
    },
    {
      "node_id": "n2",
      "title": "Auth middleware",
      "description": "Validate tokens on each request",
      "branch_suggestion": "feature/auth/middleware",
      "parents": ["n1"]
    }
  ]
}
```

`node_id` is a stable slug with no spaces. `parents` holds node_ids, never branch names, and is
empty for a root PR that branches off the stack base. `child_recipe` is optional and defaults to
`tdd`.

**Validation rules** (the hook enforces these):
- `node_id` values must be unique
- All `parents` entries must reference an existing `node_id`
- The dependency graph must be acyclic (no cycles)
- Every `branch_suggestion` must be in `feature/<stack-slug>/<node>` form, and all PRs must share
  the same `<stack-slug>` so the stack's branches group under one namespace (e.g.
  `feature/auth/token-store`, `feature/auth/middleware`)

**Scoping rules** (your judgment — the hook cannot check these, so they are on you):
- Every PR is **self-contained**: the API/schema change, the code implementing it, and its tests
  are one node. A node whose `description` promises only surface — new endpoints that return
  `unimplemented`, a field nothing reads, stub impls — is not a valid PR.
- **Never split by layer** (schema then behavior, endpoint then handler, signature then body).
  When a slice is too large, split by **capability**: one source variant, one enum case, one
  screen, happy path before edge cases — each part still end-to-end.
- Sole exceptions: a mechanical rename/move with no behavior change, or regenerating
  already-committed generated code with no new surface. Anything else, say so in the `description`
  and let a human decide.

This may be the first time this plan is written, or a chat-driven refinement of an already-written
plan — in both cases, re-emit the full plan **and re-apply the scoping rules above**: a refinement
request must not talk you into a layer-split stack.

You may also include an optional top-level `exploration` field: a short markdown code-discovery map
of the key files you inspected, each with a `path:line` reference (e.g.
`- src/auth/store.rs:42 — token persistence`). When present it is persisted to
`artifacts/exploration.md` and surfaced as context to the orchestrate phase. Omit it if there is
nothing worth recording.

A successful submit writes `stack-plan.yaml`, `pr-stack-plan.md` and `artifacts/exploration.md`
under the session directory named by `$TDDY_SESSION_DIR` — look there, not in the repository
working tree. The markdown summary is derived from the plan; there is nothing further to submit.

**CRITICAL**: the workflow cannot continue until `tddy-tools submit --goal write-stack-plan`
succeeds. A validation failure prints the offending paths — fix the JSON and run it again."##
        .replace("{recipe}", recipe)
}

pub fn analyze_stack_user_prompt(feature_input: &str) -> String {
    format!(
        "Analyze the following feature request and determine the optimal PR stack decomposition:\n\n{feature_input}"
    )
}

pub fn write_stack_plan_user_prompt(
    feature_input: &str,
    analysis_output: &str,
    answers: Option<&str>,
) -> String {
    let mut blocks = Vec::new();
    if !feature_input.trim().is_empty() {
        blocks.push(format!("## Original request\n\n{feature_input}"));
    }
    if !analysis_output.trim().is_empty() {
        blocks.push(format!("## Prior analysis\n\n{analysis_output}"));
    }
    if let Some(a) = answers.filter(|s| !s.trim().is_empty()) {
        blocks.push(format!("## Clarification\n\n{a}"));
    }
    if blocks.is_empty() {
        "Emit the stack plan based on the session context.".to_string()
    } else {
        blocks.join("\n\n")
    }
}
