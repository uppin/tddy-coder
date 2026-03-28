//! Session directories and shared path helpers (goal-agnostic).
//! Structured JSON parsing and TDD artifact writers live in `tddy-workflow-recipes`.

mod writer;

pub use writer::{
    create_session_dir_in, create_session_dir_under, create_session_dir_with_id,
    inject_cross_references, new_session_dir, plan_artifacts_root, read_impl_session_file,
    read_session_file, sessions_base_path, slugify_directory_name, write_impl_session_file,
    write_session_file, SESSIONS_SUBDIR, TDDY_SESSIONS_DIR_ENV,
};
