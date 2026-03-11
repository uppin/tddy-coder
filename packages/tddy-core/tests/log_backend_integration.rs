//! Integration tests for log_backend: redirect_debug_output and resolve_log_defaults.
//!
//! These tests touch global logger state (DEBUG_OUTPUT_FILE). The #[serial] attribute
//! ensures they run one at a time to avoid races when one test's redirect overwrites another's.

use serial_test::serial;
use tddy_core::{redirect_debug_output, resolve_log_defaults};

#[test]
#[serial]
fn resolve_log_defaults_returns_conversation_path_when_none() {
    let tmp = std::env::temp_dir().join("tddy-resolve-log-defaults");
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).unwrap();

    let result = resolve_log_defaults(None, None::<&std::path::Path>, &tmp);

    assert!(
        result.is_some(),
        "should return Some when conversation_output is None"
    );
    let path = result.unwrap();
    assert_eq!(
        path,
        tmp.join("logs").join("conversation.jsonl"),
        "path should be plan_dir/logs/conversation.jsonl"
    );
    assert!(tmp.join("logs").is_dir(), "logs dir should be created");
    assert_eq!(
        log::max_level(),
        log::LevelFilter::Debug,
        "resolve_log_defaults should bump log level to Debug when no explicit debug_output_path"
    );

    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
#[serial]
fn resolve_log_defaults_returns_explicit_path_when_set() {
    let tmp = std::env::temp_dir().join("tddy-resolve-log-explicit");
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).unwrap();

    let explicit = tmp.join("custom.jsonl");
    let result = resolve_log_defaults(Some(explicit.clone()), None::<&std::path::Path>, &tmp);

    assert_eq!(
        result,
        Some(explicit),
        "should return explicit path when set"
    );
    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
#[serial]
fn redirect_debug_output_writes_to_file() {
    let tmp = std::env::temp_dir().join("tddy-redirect-debug");
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).unwrap();

    let log_file = tmp.join("debug.log");
    tddy_core::init_tddy_logger(true, None);
    redirect_debug_output(&log_file);

    log::debug!("test redirect message");

    let content = std::fs::read_to_string(&log_file).unwrap_or_default();
    assert!(
        content.contains("test redirect message"),
        "log should be written to file, got: {}",
        content
    );

    let _ = std::fs::remove_dir_all(&tmp);
}
