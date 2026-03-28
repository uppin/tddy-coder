//! Declarative goal transition conditions evaluated against [`Context`](crate::workflow::context::Context).
//!
//! Recipes attach data-driven conditions (e.g. boolean keys) without embedding product-specific
//! branching in the presenter.

use crate::workflow::context::Context;

/// Describes a boolean transition condition backed by a single context key.
#[derive(Debug, Clone)]
pub struct BoolContextCondition {
    pub key: String,
    /// When the key is missing, use this value instead of treating missing as `false`.
    pub default_if_missing: bool,
}

/// Evaluate whether the transition condition holds (truthy bool in session context).
pub fn evaluate_bool_context_condition(ctx: &Context, cond: &BoolContextCondition) -> bool {
    let value = ctx
        .get_sync::<bool>(&cond.key)
        .unwrap_or(cond.default_if_missing);
    log::debug!(
        target: "tddy_core::workflow::goal_conditions",
        "evaluate_bool_context_condition key={} value={} default_if_missing={}",
        cond.key,
        value,
        cond.default_if_missing
    );
    value
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workflow::graph::GraphBuilder;
    use crate::workflow::task::{EchoTask, EndTask};
    use std::sync::Arc;

    #[test]
    fn evaluate_bool_context_condition_respects_true_in_context() {
        let ctx = Context::new();
        ctx.set_sync("run_optional_step_x", true);
        let cond = BoolContextCondition {
            key: "run_optional_step_x".to_string(),
            default_if_missing: false,
        };
        assert!(
            evaluate_bool_context_condition(&ctx, &cond),
            "expected declarative evaluator to return true when context key is true"
        );
    }

    #[test]
    fn conditional_edge_follows_true_branch_when_context_key_true() {
        let start = Arc::new(EchoTask::new("start"));
        let branch_true = Arc::new(EchoTask::new("branch_true"));
        let branch_false = Arc::new(EchoTask::new("branch_false"));
        let end = Arc::new(EndTask::new("end"));

        let cond = BoolContextCondition {
            key: "flag".to_string(),
            default_if_missing: false,
        };
        let cond_edge = cond.clone();
        let graph = GraphBuilder::new("test_decl_cond")
            .add_task(start)
            .add_task(branch_true.clone())
            .add_task(branch_false.clone())
            .add_task(end)
            .add_conditional_edge(
                "start",
                move |ctx| evaluate_bool_context_condition(ctx, &cond_edge),
                "branch_true",
                "branch_false",
            )
            .add_edge("branch_true", "end")
            .add_edge("branch_false", "end")
            .build();

        let ctx = Context::new();
        ctx.set_sync("flag", true);
        let next = graph
            .next_task_id("start", &ctx)
            .expect("next task from start");
        assert_eq!(
            next, "branch_true",
            "when flag is true, graph must follow the true branch"
        );
    }
}
