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
    // Given
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

    // When
    let config: LogConfig = serde_yaml::from_str(yaml).expect("parse log config");

    // Then
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
    // Given
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

    // Then
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
    // Given
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
    // Given
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

    // Then
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
    // Given
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

    // Then
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
    // Given
    std::env::remove_var("TDDY_QUIET");
    std::env::remove_var("RUST_LOG");

    let config = default_log_config(None, None);

    // Then
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
    // Given
    std::env::set_var("TDDY_QUIET", "1");
    let config = default_log_config(None, None);
    let default_logger = config.loggers.get("default").unwrap();

    // Then
    assert!(matches!(default_logger.output, LogOutput::Buffer));
    std::env::remove_var("TDDY_QUIET");
}

#[test]
fn default_log_config_respects_log_level_override() {
    // Given
    let config = default_log_config(Some(log::LevelFilter::Trace), None);

    // Then
    assert_eq!(config.default.level, log::LevelFilter::Trace);
}

fn log_config_with_default_output(output_yaml: &str) -> LogConfig {
    let yaml = format!(
        r#"
loggers:
  default:
    output: {output_yaml}
    format: "{{timestamp}} [{{level}}] [{{target}}] {{message}}"
default:
  level: debug
  logger: default
rotation:
  max_rotated: 0
"#
    );
    serde_yaml::from_str(&yaml).expect("parse log config")
}

#[test]
#[serial]
fn output_fan_out_to_stderr_and_a_file_writes_the_line_to_the_file() {
    // Given a logger configured the way dev.desktop.yaml configures its default logger:
    // visible in the terminal and recorded on disk at the same time.
    // (Only the file half is asserted here — capturing the process's own stderr would need a
    // process-wide fd redirect that would swallow the test harness's panic output.)
    let log_dir = tempfile::tempdir().expect("tempdir");
    let log_file = log_dir.path().join("daemon.log");
    let config = log_config_with_default_output(&format!(
        r#"[stderr, {{ file: "{}" }}]"#,
        log_file.display()
    ));

    // When
    init_tddy_logger(config);
    log::debug!(target: "fan_out_target", "fanned out to stderr and a file");

    // Then
    let content = std::fs::read_to_string(&log_file).expect("read fan-out log file");
    assert!(
        content.contains("[fan_out_target] fanned out to stderr and a file"),
        "got: {}",
        content
    );
}

#[test]
#[serial]
fn output_fan_out_writes_the_same_line_to_both_of_its_destinations() {
    // Given a logger that fans out to the in-memory buffer and a file — two destinations whose
    // contents a test can both read back, so "the same line reaches every destination" is proven
    // rather than assumed.
    let log_dir = tempfile::tempdir().expect("tempdir");
    let log_file = log_dir.path().join("daemon.log");
    let config = log_config_with_default_output(&format!(
        r#"[buffer, {{ file: "{}" }}]"#,
        log_file.display()
    ));
    let _ = tddy_core::take_buffered_logs();

    // When
    init_tddy_logger(config);
    log::debug!(target: "fan_out_target", "fanned out to a buffer and a file");

    // Then
    let buffered = tddy_core::take_buffered_logs();
    let file_lines: Vec<String> = std::fs::read_to_string(&log_file)
        .expect("read fan-out log file")
        .lines()
        .map(str::to_string)
        .collect();
    assert_eq!(buffered.len(), 1, "buffered: {:?}", buffered);
    assert_eq!(file_lines, buffered);
    assert!(
        buffered[0].contains("[fan_out_target] fanned out to a buffer and a file"),
        "got: {:?}",
        buffered
    );
}
