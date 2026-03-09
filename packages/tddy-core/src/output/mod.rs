//! Output parsing and artifact writing.

mod parser;
mod writer;

pub use parser::{
    extract_last_structured_block, parse_acceptance_tests_response, parse_demo_response,
    parse_evaluate_response, parse_green_response, parse_planning_output, parse_planning_response,
    parse_red_response, parse_refactor_response, parse_validate_response,
    parse_validate_subagents_response, AcceptanceTestInfo, AcceptanceTestsOutput, DemoOutput,
    DemoPlan, DemoResults, DemoStep, EvaluateAffectedTest, EvaluateChangedFile, EvaluateOutput,
    GreenOutput, GreenTestResult, ImplementationInfo, MarkerInfo, MarkerResult, PlanningOutput,
    RedOutput, RedTestInfo, RefactorOutput, SkeletonInfo, StructuredBlock, ValidateBuildResult,
    ValidateChangesetSync, ValidateFileAnalyzed, ValidateIssue, ValidateOutput,
    ValidateSubagentsOutput, ValidateTestImpact,
};
pub use writer::{
    read_impl_session_file, read_session_file, slugify_directory_name,
    update_acceptance_tests_file, update_progress_file, write_acceptance_tests_file,
    write_artifacts, write_demo_plan_file, write_demo_results_file, write_evaluation_report,
    write_impl_session_file, write_progress_file, write_red_output_file, write_session_file,
    write_validation_report,
};
