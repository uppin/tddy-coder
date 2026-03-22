//! Output parsing and artifact writing.

mod parser;
mod writer;

pub use parser::{
    parse_acceptance_tests_response, parse_demo_response, parse_evaluate_response,
    parse_green_response, parse_planning_response, parse_planning_response_with_base,
    parse_red_response, parse_refactor_response, parse_update_docs_response,
    parse_validate_subagents_response, validate_red_marker_source_paths, AcceptanceTestInfo,
    AcceptanceTestsOutput, DemoOutput, DemoPlan, DemoResults, DemoStep, EvaluateAffectedTest,
    EvaluateBuildResult, EvaluateChangedFile, EvaluateChangesetSync, EvaluateFileAnalyzed,
    EvaluateIssue, EvaluateOutput, EvaluateTestImpact, GreenOutput, GreenTestResult,
    ImplementationInfo, MarkerInfo, MarkerResult, PlanningOutput, RedOutput, RedTestInfo,
    RefactorOutput, SkeletonInfo, UpdateDocsOutput, ValidateSubagentsOutput,
};
pub use writer::{
    create_session_dir_in, create_session_dir_under, create_session_dir_with_id,
    read_impl_session_file, read_session_file, sessions_base_path, slugify_directory_name,
    update_acceptance_tests_file, update_progress_file, write_acceptance_tests_file,
    write_artifacts, write_demo_plan_file, write_demo_results_file, write_evaluation_report,
    write_impl_session_file, write_progress_file, write_red_output_file, write_session_file,
    SESSIONS_SUBDIR, TDDY_SESSIONS_DIR_ENV,
};
