pub const STACK_PLAN_BASENAME: &str = "stack-plan.yaml";
pub const PR_STACK_PLAN_MD_BASENAME: &str = "pr-stack-plan.md";

pub fn analyze_stack_system_prompt() -> String {
    "You are assisting with the **plan-pr-stack** workflow **analyze-stack** step.\n\n\
     ## Task: Analyze PR stack decomposition\n\n\
     Analyze the feature request and determine how to decompose it into a stack of pull requests. \
     Consider dependencies between PRs and identify which can be built in parallel (DAG structure).\n\n\
     This is a **read-only** analysis phase — do not write code or create files. \
     Focus on understanding the feature scope and identifying the optimal PR decomposition strategy, \
     noting which PRs depend on others and which can be developed concurrently.\n\n\
     For each proposed PR, identify:\n\
     1. A stable slug (`node_id`, e.g. `auth-store`, `api-client`)\n\
     2. A concise title\n\
     3. A description of what it implements\n\
     4. Its dependencies (which other PRs must merge first)\n\
     5. A branch name suggestion (e.g. `feature/auth-store`)\n\
     6. The child recipe to use (default: `tdd`)\n"
        .to_string()
}

pub fn write_stack_plan_system_prompt() -> String {
    "You are assisting with the **plan-pr-stack** workflow **write-stack-plan** step.\n\n\
     ## Task: Emit structured PR stack plan\n\n\
     Based on the prior analysis, emit a structured PR stack plan using the `submit` tool \
     with key `stack-plan`. The YAML must conform to this contract:\n\n\
     ```yaml\n\
     version: 1\n\
     prs:\n\
       - node_id: n1          # stable slug, no spaces\n\
         title: \"Auth token store\"\n\
         description: \"Store tokens securely in the keyring\"\n\
         branch_suggestion: \"feature/auth-store\"\n\
         parents: []          # empty = root PR, off the stack base branch\n\
         child_recipe: tdd    # optional; default is tdd\n\
       - node_id: n2\n\
         title: \"Auth middleware\"\n\
         description: \"Validate tokens on each request\"\n\
         branch_suggestion: \"feature/auth-middleware\"\n\
         parents: [n1]        # depends on n1; use node_ids, not branch names\n\
     ```\n\n\
     **Validation rules** (the hook enforces these):\n\
     - `node_id` values must be unique\n\
     - All `parents` entries must reference an existing `node_id`\n\
     - The dependency graph must be acyclic (no cycles)\n\n\
     Also submit a human-readable plan summary using key `stack-plan-md`.\n"
        .to_string()
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
