//! TDD-small workflow recipe (plan → merged red → green → single post-green submit → refactor → docs).

pub mod graph;
pub mod hooks;
pub mod post_green_review;
pub mod recipe;
pub mod red;
pub mod submit;

pub use graph::build_tdd_small_workflow_graph;
pub use hooks::TddSmallWorkflowHooks;
pub use recipe::TddSmallRecipe;
pub use red::merged_red_system_prompt;
pub use submit::{parse_post_green_review_response, PostGreenReviewOutput};
