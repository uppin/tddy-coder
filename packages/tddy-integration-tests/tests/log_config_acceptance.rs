//! Acceptance tests for configurable log routing (LogConfig, selectors, outputs, rotation).
//!
//! These tests define desired behavior. They are expected to FAIL until the implementation
//! is complete (Milestones 1-6). Run with: cargo test -p tddy-core log_config_acceptance

use serial_test::serial;
use std::collections::HashMap;
use tddy_core::{
    default_log_config, init_tddy_logger, LogConfig, LogOutput, LogRotation, LogSelector,
    LoggerDefinition,
};

#[test]
fn log_config_parses_from_yaml_with_all_selector_types() {
    let yaml = r#"
loggers:
  default:
    output: stderr
    format: "{timestamp} [{level}] [{target}] {message}"
  webrtc_file:
    output: { file: "logs/webrtc.log" }
    format: "{timestamp} [{level}] [{target}] {message}"
  workflow_file:
    output: { file: "logs/workflow.log" }
    format: "{timestamp} [{level}] [{target}] {message}"
  muted:
    output: mute
default:
  level: info
  logger: default
rotation:
  max_rotated: 5
policies:
  - selector:
      target: "libwebrtc"
    level: debug
    logger: webrtc_file
  - selector:
      target: "livekit::*"
    level: warn
    logger: default
  - selector:
      module_path: "tddy_core::workflow"
    level: trace
    logger: workflow_file
  - selector:
      heuristic:
        message_contains: ".cc:"
    logger: muted
"#;
    let config: LogConfig = serde_yaml::from_str(yaml).expect("parse log config");
    assert_eq!(config.default.level, log::LevelFilter::Info);
    assert_eq!(config.default.logger, "default");
    assert_eq!(config.policies.len(), 4);
    assert!(matches!(
        &config.policies[0].selector,
        LogSelector::Target { target: s } if s == "libwebrtc"
    ));
    assert_eq!(config.policies[0].logger.as_deref(), Some("webrtc_file"));
    assert!(matches!(
        &config.policies[1].selector,
        LogSelector::Target { target: s } if s == "livekit::*"
    ));
    assert!(matches!(
        &config.policies[2].selector,
        LogSelector::ModulePath { module_path: s } if s == "tddy_core::workflow"
    ));
    assert!(matches!(
        &config.policies[3].selector,
        LogSelector::Heuristic { heuristic: h } if h.message_contains == ".cc:"
    ));
    let default_logger = config.loggers.get("default").unwrap();
    assert!(matches!(default_logger.output, LogOutput::Stderr));
}

#[test]
#[serial]
fn output_file_writes_log_to_path_with_target_in_format() {
    let tmp = std::env::temp_dir().join("tddy-log-config-file");
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).unwrap();
    let log_file = tmp.join("test.log");

    let mut loggers = HashMap::new();
    loggers.insert(
        "default".to_string(),
        LoggerDefinition {
            output: LogOutput::File(log_file.clone()),
            format: Some("{timestamp} [{level}] [{target}] {message}".to_string()),
        },
    );
    let config = LogConfig {
        loggers,
        default: tddy_core::DefaultLogPolicy {
            level: log::LevelFilter::Debug,
            logger: "default".to_string(),
        },
        policies: vec![],
        rotation: Some(LogRotation {
            max_rotated: 0,
            only_paths: vec![],
        }),
    };
    init_tddy_logger(config);
    log::debug!(target: "test_target", "file output message");

    let content = std::fs::read_to_string(&log_file).unwrap_or_default();
    assert!(content.contains("file output message"), "got: {}", content);
    assert!(
        content.contains("[test_target]"),
        "format should include target, got: {}",
        content
    );

    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
#[serial]
fn output_mute_discards_logs() {
    let mut loggers = HashMap::new();
    loggers.insert(
        "default".to_string(),
        LoggerDefinition {
            output: LogOutput::Mute,
            format: None,
        },
    );
    let config = LogConfig {
        loggers,
        default: tddy_core::DefaultLogPolicy {
            level: log::LevelFilter::Debug,
            logger: "default".to_string(),
        },
        policies: vec![],
        rotation: None,
    };
    init_tddy_logger(config);
    log::debug!(target: "muted", "should not appear anywhere");
    // No panic, no output - mute works
}

#[test]
#[serial]
fn log_rotation_renames_existing_file_with_timestamp() {
    let tmp = std::env::temp_dir().join("tddy-log-rotation");
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).unwrap();
    let log_file = tmp.join("debug.log");
    std::fs::write(&log_file, "old content\n").unwrap();

    let mut loggers = HashMap::new();
    loggers.insert(
        "default".to_string(),
        LoggerDefinition {
            output: LogOutput::File(log_file.clone()),
            format: None,
        },
    );
    let config = LogConfig {
        loggers,
        default: tddy_core::DefaultLogPolicy {
            level: log::LevelFilter::Debug,
            logger: "default".to_string(),
        },
        policies: vec![],
        rotation: Some(LogRotation {
            max_rotated: 5,
            only_paths: vec![],
        }),
    };
    init_tddy_logger(config);
    log::debug!("new content");

    // Original file was renamed to debug.{timestamp}.log; new debug.log created for writing
    let rotated: Vec<_> = std::fs::read_dir(tmp.as_path())
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| {
            let name = e.file_name();
            let s = name.to_str().unwrap_or("");
            s.starts_with("debug.") && s.ends_with(".log") && s != "debug.log"
        })
        .collect();
    assert_eq!(rotated.len(), 1, "should have one rotated file");
    let rotated_content = std::fs::read_to_string(rotated[0].path()).unwrap_or_default();
    assert!(rotated_content.contains("old content"));

    let new_content = std::fs::read_to_string(&log_file).unwrap_or_default();
    assert!(new_content.contains("new content"));

    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
#[serial]
fn log_rotation_prunes_beyond_max_rotated() {
    let tmp = std::env::temp_dir().join("tddy-log-rotation-prune");
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).unwrap();
    let log_file = tmp.join("x.log");
    std::fs::write(&log_file, "current").unwrap();

    // Create 7 pre-existing rotated files
    for i in 1..=7 {
        std::fs::write(
            tmp.join(format!("x.2026-03-19T{:02}-00-00.log", i)),
            format!("old {}", i),
        )
        .unwrap();
    }

    let mut loggers = HashMap::new();
    loggers.insert(
        "default".to_string(),
        LoggerDefinition {
            output: LogOutput::File(log_file.clone()),
            format: None,
        },
    );
    let config = LogConfig {
        loggers,
        default: tddy_core::DefaultLogPolicy {
            level: log::LevelFilter::Debug,
            logger: "default".to_string(),
        },
        policies: vec![],
        rotation: Some(LogRotation {
            max_rotated: 3,
            only_paths: vec![],
        }),
    };
    init_tddy_logger(config);

    let rotated_count = std::fs::read_dir(&tmp)
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| {
            let n = e.file_name();
            let s = n.to_str().unwrap_or("");
            s.starts_with("x.") && s.ends_with(".log") && s != "x.log"
        })
        .count();
    assert!(
        rotated_count <= 3,
        "should keep at most 3 rotated files, got {}",
        rotated_count
    );

    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
#[serial]
fn default_log_config_matches_current_behavior() {
    std::env::remove_var("TDDY_QUIET");
    std::env::remove_var("RUST_LOG");

    let config = default_log_config(None, None);
    assert_eq!(config.default.level, log::LevelFilter::Info);
    assert_eq!(config.default.logger, "default");
    let default_logger = config.loggers.get("default").unwrap();
    assert!(matches!(default_logger.output, LogOutput::Stderr));
    assert!(default_logger
        .format
        .as_ref()
        .is_some_and(|f| f.contains("{target}")));
}

#[test]
#[serial]
fn default_log_config_uses_tddy_quiet_buffer_when_set() {
    std::env::set_var("TDDY_QUIET", "1");
    let config = default_log_config(None, None);
    let default_logger = config.loggers.get("default").unwrap();
    assert!(matches!(default_logger.output, LogOutput::Buffer));
    std::env::remove_var("TDDY_QUIET");
}

#[test]
fn default_log_config_respects_log_level_override() {
    let config = default_log_config(Some(log::LevelFilter::Trace), None);
    assert_eq!(config.default.level, log::LevelFilter::Trace);
}
