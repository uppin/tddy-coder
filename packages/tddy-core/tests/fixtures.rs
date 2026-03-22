//! Shared JSON fixtures for MockBackend integration tests.
//! All outputs are tddy-tools submit format (raw JSON, no XML/delimiters).
//! Each test file uses a subset; allow dead_code to avoid per-file unused warnings.

#![allow(dead_code)]

pub const PLAN_JSON: &str = "{\"goal\":\"plan\",\"prd\":\"# Feature PRD\\n\\n## Summary\\nUser authentication system with login and logout.\\n\\n## Acceptance Criteria\\n- [ ] Login with email/password\\n- [ ] Logout clears session\\n\\n## TODO\\n\\n- [ ] Create auth module\\n- [ ] Implement login endpoint\\n- [ ] Implement logout endpoint\\n- [ ] Add session management\",\"branch_suggestion\":\"feature/auth\",\"worktree_suggestion\":\"feature-auth\"}";

pub const PLAN_JSON_WITH_DISCOVERY: &str = r##"{"goal":"plan","prd":"# PRD\n## Summary\nAuth feature.\n\n## TODO\n\n- [ ] Task 1","discovery":{"toolchain":{"rust":"1.78.0","cargo":"from Cargo.toml"},"scripts":{"test":"cargo test","lint":"cargo clippy"},"doc_locations":["docs/ft/","packages/*/docs/"],"relevant_code":[{"path":"src/workflow/mod.rs","reason":"state machine"}],"test_infrastructure":{"runner":"cargo test","conventions":"tests/*.rs"}},"demo_plan":{"demo_type":"cli","setup_instructions":"Run cargo build","steps":[{"description":"Run the CLI","command_or_action":"cargo run","expected_result":"See output"}],"verification":"CLI runs without error"}}"##;

pub const PLAN_JSON_WITH_NAME: &str = r##"{"goal":"plan","name":"Auth Feature","prd":"# PRD\n## Summary\nAuth feature.\n\n## TODO\n\n- [ ] Task 1","discovery":{"toolchain":{"rust":"1.78.0"},"scripts":{"test":"cargo test"},"doc_locations":["docs/"]},"demo_plan":{"demo_type":"cli","setup_instructions":"Run cargo build","steps":[{"description":"Run CLI","command_or_action":"cargo run","expected_result":"See output"}],"verification":"OK"}}"##;

pub const ACCEPTANCE_TESTS_JSON: &str = r#"{"goal":"acceptance-tests","summary":"Created 2 acceptance tests. All failing (Red state) as expected.","tests":[{"name":"login_stores_session_token","file":"packages/auth/tests/session.it.rs","line":15,"status":"failing"},{"name":"logout_clears_session","file":"packages/auth/tests/session.it.rs","line":28,"status":"failing"}]}"#;

pub const ACCEPTANCE_TESTS_JSON_MINIMAL: &str = r#"{"goal":"acceptance-tests","summary":"Created 2 tests.","tests":[{"name":"login_stores_session_token","file":"packages/auth/tests/session.it.rs","line":15,"status":"failing"},{"name":"logout_clears_session","file":"packages/auth/tests/session.it.rs","line":28,"status":"failing"}]}"#;

pub const RED_JSON: &str = r#"{"goal":"red","summary":"Created 2 skeleton methods and 3 failing unit tests. All tests failing as expected.","tests":[{"name":"auth_service_validates_email","file":"packages/auth/src/service.rs","line":42,"status":"failing"},{"name":"auth_service_rejects_empty_email","file":"packages/auth/src/service.rs","line":55,"status":"failing"},{"name":"session_store_persists_token","file":"packages/auth/tests/session_it.rs","line":22,"status":"failing"}],"skeletons":[{"name":"AuthService","file":"packages/auth/src/service.rs","line":10,"kind":"struct"},{"name":"validate_email","file":"packages/auth/src/service.rs","line":25,"kind":"method"}]}"#;

pub const RED_JSON_MINIMAL: &str = r#"{"goal":"red","summary":"Created skeletons and failing tests.","tests":[{"name":"test_auth","file":"src/auth.rs","line":10,"status":"failing"}],"skeletons":[{"name":"AuthService","file":"src/auth.rs","line":5,"kind":"struct"}]}"#;

pub const RED_JSON_WITH_MARKERS: &str = r#"{"goal":"red","summary":"Created with markers.","tests":[{"name":"test_auth","file":"src/auth.rs","line":10,"status":"failing"}],"skeletons":[{"name":"AuthService","file":"src/auth.rs","line":5,"kind":"struct"}],"test_command":"cargo test","sequential_command":"cargo test -- --test-threads=1","logging_command":"RUST_LOG=debug cargo test"}"#;

/// Red output with marker definitions and marker_results (for red_goal_adds_logging_markers test).
pub const RED_JSON_WITH_LOGGING_MARKERS: &str = r#"{"goal":"red","summary":"Created skeletons and failing tests with logging markers.","tests":[{"name":"test_auth","file":"src/auth.rs","line":10,"status":"failing"}],"skeletons":[{"name":"AuthService","file":"src/auth.rs","line":5,"kind":"struct"}],"markers":[{"marker_id":"M001","test_name":"test_auth","scope":"auth_service::validate","data":{"user":"test@example.com"}}],"marker_results":[{"marker_id":"M001","test_name":"test_auth","scope":"auth_service::validate","collected":true,"investigation":null}]}"#;

pub const RED_JSON_VALID: &str = r#"{"goal":"red","summary":"Created 2 skeletons and 1 failing test.","tests":[{"name":"test_foo","file":"src/foo.rs","line":10,"status":"failing"}],"skeletons":[{"name":"Foo","file":"src/foo.rs","line":5,"kind":"struct"},{"name":"bar","file":"src/foo.rs","line":8,"kind":"method"}]}"#;

pub const RED_JSON_INVALID: &str = r#"{"goal":"red","summary":"Created skeletons.","tests":[{"name":"test_foo","file":"src/foo.rs","line":"ten","status":"failing"}],"skeletons":[]}"#;

pub const GREEN_JSON: &str = r#"{"goal":"green","summary":"Implemented production code. All tests passing.","tests":[{"name":"auth_service_validates_email","file":"packages/auth/src/service.rs","line":42,"status":"passing"},{"name":"auth_service_rejects_empty_email","file":"packages/auth/src/service.rs","line":55,"status":"passing"},{"name":"session_store_persists_token","file":"packages/auth/tests/session_it.rs","line":22,"status":"passing"}],"implementations":[{"name":"AuthService","file":"packages/auth/src/service.rs","line":10,"kind":"struct"},{"name":"validate_email","file":"packages/auth/src/service.rs","line":25,"kind":"method"}],"test_command":"cargo test","prerequisite_actions":"None","run_single_or_selected_tests":"cargo test <name>"}"#;

pub const GREEN_JSON_MINIMAL: &str = r#"{"goal":"green","summary":"Done.","tests":[{"name":"test_auth","file":"src/auth.rs","line":10,"status":"passing"}],"implementations":[{"name":"AuthService","file":"src/auth.rs","line":5,"kind":"struct"}],"test_command":"cargo test","prerequisite_actions":"None","run_single_or_selected_tests":"cargo test <name>"}"#;

pub const GREEN_JSON_SOME_FAIL: &str = r#"{"goal":"green","summary":"Implemented partial. Some tests still failing.","tests":[{"name":"test_auth","file":"src/auth.rs","line":10,"status":"passing"},{"name":"test_logout","file":"src/auth.rs","line":25,"status":"failing"}],"implementations":[{"name":"AuthService","file":"src/auth.rs","line":5,"kind":"struct"}],"test_command":"cargo test","prerequisite_actions":"None","run_single_or_selected_tests":"cargo test <name>"}"#;

pub const EVALUATE_JSON: &str = r#"{"goal":"evaluate-changes","summary":"Evaluated. All criteria met.","risk_level":"low","build_results":[{"package":"tddy-core","status":"pass","notes":null}],"issues":[],"changeset_sync":{"status":"synced","items_updated":0,"items_added":0},"files_analyzed":[],"test_impact":{"tests_affected":0,"new_tests_needed":0},"changed_files":[],"affected_tests":[],"validity_assessment":"OK"}"#;

pub const VALIDATE_JSON: &str = r#"{"goal":"validate","summary":"All 3 subagents completed.","tests_report_written":true,"prod_ready_report_written":true,"clean_code_report_written":true,"refactoring_plan_written":true,"refactoring_plan":"\n# Refactoring Plan\n\n## Tasks\n\n- None required.\n"}"#;

pub const REFACTOR_JSON: &str = r#"{"goal":"refactor","summary":"Completed. All tests passing.","tasks_completed":5,"tests_passing":true}"#;

pub const UPDATE_DOCS_JSON: &str =
    r#"{"goal":"update-docs","summary":"Updated 2 docs.","docs_updated":2}"#;

pub const VALIDATE_REFACTOR_JSON: &str = r#"{"goal":"validate","summary":"All 3 subagents completed.","tests_report_written":true,"prod_ready_report_written":true,"clean_code_report_written":true,"refactoring_plan_written":true,"refactoring_plan":"\n# Refactoring Plan\n\n## Tasks\n\n- None required.\n"}"#;
