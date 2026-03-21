//! Log backend: when TDDY_QUIET is set (TUI mode), buffer logs for display.
//! Otherwise write to stderr. When debug_output_path is set, redirect logs to file.
//! Prevents stderr output from breaking ratatui layout.
//!
//! Supports configurable log routing via LogConfig: selectors (target, module_path, heuristic)
//! map to level filters and output destinations (stderr, stdout, file, buffer, mute).

use log::{Level, Log, Metadata, Record};
use serde::Deserialize;
use std::collections::HashMap;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

const MAX_BUFFER_LINES: usize = 200;

const DEFAULT_LOG_FORMAT: &str = "{timestamp} [{level}] [{target}] {message}";

// -----------------------------------------------------------------------------
// Log config types (YAML-deserializable)
// -----------------------------------------------------------------------------

/// Top-level log configuration.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LogConfig {
    #[serde(default)]
    pub loggers: HashMap<String, LoggerDefinition>,
    pub default: DefaultLogPolicy,
    #[serde(default)]
    pub policies: Vec<LogPolicy>,
    #[serde(default)]
    pub rotation: Option<LogRotation>,
}

/// Named logger: output target and optional format.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LoggerDefinition {
    pub output: LogOutput,
    #[serde(default)]
    pub format: Option<String>,
}

/// Default policy when no selector matches.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DefaultLogPolicy {
    #[serde(default = "default_level")]
    pub level: log::LevelFilter,
    pub logger: String,
}

fn default_level() -> log::LevelFilter {
    log::LevelFilter::Info
}

/// Per-selector policy (first match wins).
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LogPolicy {
    pub selector: LogSelector,
    #[serde(default)]
    pub level: Option<log::LevelFilter>,
    #[serde(default)]
    pub logger: Option<String>,
}

/// Selector for matching log records.
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum LogSelector {
    Target { target: String },
    ModulePath { module_path: String },
    Heuristic { heuristic: HeuristicSelector },
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HeuristicSelector {
    pub message_contains: String,
}

/// Output destination for log lines.
#[derive(Debug, Clone)]
pub enum LogOutput {
    Stderr,
    Stdout,
    File(PathBuf),
    Buffer,
    Mute,
}

impl<'de> Deserialize<'de> for LogOutput {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum De {
            Str(String),
            File { file: PathBuf },
        }
        match De::deserialize(deserializer)? {
            De::Str(s) => match s.as_str() {
                "stderr" => Ok(LogOutput::Stderr),
                "stdout" => Ok(LogOutput::Stdout),
                "buffer" => Ok(LogOutput::Buffer),
                "mute" => Ok(LogOutput::Mute),
                _ => Err(serde::de::Error::custom(format!(
                    "unknown log output: {}",
                    s
                ))),
            },
            De::File { file } => Ok(LogOutput::File(file)),
        }
    }
}

/// Log rotation settings (startup rotation).
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LogRotation {
    #[serde(default = "default_max_rotated")]
    pub max_rotated: usize,
}

fn default_max_rotated() -> usize {
    5
}

/// Returns true if the record matches the selector.
pub fn matches_selector(record: &Record, selector: &LogSelector) -> bool {
    match selector {
        LogSelector::Target { target } => matches_target(record.target(), target),
        LogSelector::ModulePath { module_path } => record
            .module_path()
            .is_some_and(|mp| matches_pattern(mp, module_path)),
        LogSelector::Heuristic { heuristic } => {
            let msg = format!("{}", record.args());
            msg.contains(&heuristic.message_contains)
        }
    }
}

fn matches_target(actual: &str, pattern: &str) -> bool {
    matches_pattern(actual, pattern)
}

fn matches_pattern(value: &str, pattern: &str) -> bool {
    if let Some(prefix) = pattern.strip_suffix('*') {
        value.starts_with(prefix)
    } else {
        value == pattern
    }
}

/// Result of policy matching: either the default or a specific policy.
/// Holds resolved logger for output/format.
#[derive(Debug)]
pub enum MatchedPolicy<'a> {
    Default(&'a DefaultLogPolicy, &'a LoggerDefinition),
    Policy(&'a LogPolicy, &'a DefaultLogPolicy, &'a LoggerDefinition),
}

impl MatchedPolicy<'_> {
    pub fn level(&self) -> log::LevelFilter {
        match self {
            MatchedPolicy::Default(d, _) => d.level,
            MatchedPolicy::Policy(p, d, _) => p.level.unwrap_or(d.level),
        }
    }

    pub fn output(&self) -> &LogOutput {
        match self {
            MatchedPolicy::Default(_, logger) => &logger.output,
            MatchedPolicy::Policy(_, _, logger) => &logger.output,
        }
    }

    pub fn format(&self) -> Option<&str> {
        match self {
            MatchedPolicy::Default(_, logger) => logger.format.as_deref(),
            MatchedPolicy::Policy(_, _, logger) => logger.format.as_deref(),
        }
    }

    pub fn is_policy(&self) -> bool {
        matches!(self, MatchedPolicy::Policy(_, _, _))
    }
}

/// Resolve logger by name. Falls back to default_logger_name when policy_logger_name
/// is None or the named logger is not found.
pub fn resolve_logger<'a>(
    config: &'a LogConfig,
    policy_logger_name: Option<&str>,
    default_logger_name: &str,
) -> Option<&'a LoggerDefinition> {
    let name = policy_logger_name
        .and_then(|n| config.loggers.get(n))
        .map(|_| policy_logger_name.unwrap())
        .unwrap_or(default_logger_name);
    config.loggers.get(name)
}

/// Find the matching policy for a record (first match wins).
pub fn find_matching_policy<'a>(
    record: &Record,
    config: &'a LogConfig,
) -> Option<MatchedPolicy<'a>> {
    for policy in &config.policies {
        if matches_selector(record, &policy.selector) {
            let logger = resolve_logger(config, policy.logger.as_deref(), &config.default.logger)?;
            return Some(MatchedPolicy::Policy(policy, &config.default, logger));
        }
    }
    let logger = resolve_logger(config, None, &config.default.logger)?;
    Some(MatchedPolicy::Default(&config.default, logger))
}

/// Build default LogConfig when no YAML log section is present.
/// Creates implicit "default" logger in loggers map.
pub fn default_log_config(
    log_level_override: Option<log::LevelFilter>,
    _plan_dir: Option<&Path>,
) -> LogConfig {
    let level = log_level_override.unwrap_or_else(|| {
        std::env::var("RUST_LOG")
            .ok()
            .and_then(|s| {
                s.parse::<log::LevelFilter>()
                    .ok()
                    .or_else(|| match s.to_lowercase().as_str() {
                        "off" => Some(log::LevelFilter::Off),
                        "error" => Some(log::LevelFilter::Error),
                        "warn" => Some(log::LevelFilter::Warn),
                        "info" => Some(log::LevelFilter::Info),
                        "debug" => Some(log::LevelFilter::Debug),
                        "trace" => Some(log::LevelFilter::Trace),
                        _ => None,
                    })
            })
            .unwrap_or(log::LevelFilter::Info)
    });

    let output = if std::env::var("TDDY_QUIET").is_ok() {
        LogOutput::Buffer
    } else {
        LogOutput::Stderr
    };

    let mut loggers = HashMap::new();
    loggers.insert(
        "default".to_string(),
        LoggerDefinition {
            output,
            format: Some(DEFAULT_LOG_FORMAT.to_string()),
        },
    );

    LogConfig {
        loggers,
        default: DefaultLogPolicy {
            level,
            logger: "default".to_string(),
        },
        policies: vec![],
        rotation: None,
    }
}

// -----------------------------------------------------------------------------
// Log state and TddyLogger
// -----------------------------------------------------------------------------

static LOG_CONFIG: once_cell::sync::Lazy<Mutex<Option<LogConfig>>> =
    once_cell::sync::Lazy::new(|| Mutex::new(None));

static LOG_BUFFER: once_cell::sync::Lazy<Mutex<Vec<String>>> =
    once_cell::sync::Lazy::new(|| Mutex::new(Vec::with_capacity(64)));

static FILE_OUTPUTS: once_cell::sync::Lazy<
    Mutex<std::collections::HashMap<PathBuf, std::fs::File>>,
> = once_cell::sync::Lazy::new(|| Mutex::new(std::collections::HashMap::new()));

/// Legacy: for backward compat during transition.
static DEBUG_OUTPUT_FILE: once_cell::sync::Lazy<Mutex<Option<std::fs::File>>> =
    once_cell::sync::Lazy::new(|| Mutex::new(None));

static WEBRTC_DEBUG_OUTPUT_FILE: once_cell::sync::Lazy<Mutex<Option<std::fs::File>>> =
    once_cell::sync::Lazy::new(|| Mutex::new(None));

fn format_log_line(record: &Record, format_tpl: Option<&str>) -> String {
    let tpl = format_tpl.unwrap_or(DEFAULT_LOG_FORMAT);
    let ts = chrono::Local::now().format("%H:%M:%S%.3f");
    let target = record.target();
    let module = record.module_path().unwrap_or("");
    let msg = format!("{}", record.args());
    tpl.replace("{timestamp}", &ts.to_string())
        .replace("{level}", &record.level().to_string())
        .replace("{target}", target)
        .replace("{module}", module)
        .replace("{message}", &msg)
        + "\n"
}

fn write_to_output(line: &str, output: &LogOutput) {
    match output {
        LogOutput::Stderr => {
            eprint!("{}", line);
        }
        LogOutput::Stdout => {
            print!("{}", line);
        }
        LogOutput::File(path) => {
            if let Ok(mut guard) = FILE_OUTPUTS.lock() {
                if let Some(f) = guard.get_mut(path) {
                    let _ = f.write_all(line.as_bytes());
                    let _ = f.flush();
                }
            }
        }
        LogOutput::Buffer => {
            let mut buf = match LOG_BUFFER.lock() {
                Ok(b) => b,
                Err(e) => e.into_inner(),
            };
            buf.push(line.trim_end().to_string());
            let excess = buf.len().saturating_sub(MAX_BUFFER_LINES);
            if excess > 0 {
                buf.drain(0..excess);
            }
        }
        LogOutput::Mute => {}
    }
}

/// Logger that uses LogConfig when set, else legacy behavior.
pub struct TddyLogger;

static LOGGER: TddyLogger = TddyLogger;

impl Log for TddyLogger {
    fn enabled(&self, metadata: &Metadata) -> bool {
        if let Ok(guard) = LOG_CONFIG.lock() {
            if let Some(ref config) = *guard {
                let max = config
                    .policies
                    .iter()
                    .filter_map(|p| p.level)
                    .chain(std::iter::once(config.default.level))
                    .max_by_key(|l| *l as u8)
                    .unwrap_or(config.default.level);
                return metadata.level() <= max;
            }
        }
        metadata.level() <= Level::Debug
    }

    fn log(&self, record: &Record) {
        if let Ok(guard) = LOG_CONFIG.lock() {
            if let Some(ref config) = *guard {
                if let Some(matched) = find_matching_policy(record, config) {
                    if record.metadata().level() > matched.level() {
                        return;
                    }
                    let line = format_log_line(record, matched.format());
                    write_to_output(&line, matched.output());
                    return;
                }
            }
        }

        // Legacy path
        if !self.enabled(record.metadata()) {
            return;
        }
        let ts = chrono::Local::now().format("%H:%M:%S%.3f");
        let line = format!("{} [{}] {}\n", ts, record.level(), record.args());
        let is_webrtc = record.target() == "libwebrtc";

        if is_webrtc {
            if let Ok(mut guard) = WEBRTC_DEBUG_OUTPUT_FILE.lock() {
                if let Some(ref mut f) = *guard {
                    let _ = f.write_all(line.as_bytes());
                    let _ = f.flush();
                    return;
                }
            }
        }

        if let Ok(mut guard) = DEBUG_OUTPUT_FILE.lock() {
            if let Some(ref mut f) = *guard {
                let _ = f.write_all(line.as_bytes());
                let _ = f.flush();
                return;
            }
        }
        if std::env::var("TDDY_QUIET").is_ok() {
            let mut buf = match LOG_BUFFER.lock() {
                Ok(b) => b,
                Err(e) => e.into_inner(),
            };
            buf.push(line.trim_end().to_string());
            let excess = buf.len().saturating_sub(MAX_BUFFER_LINES);
            if excess > 0 {
                buf.drain(0..excess);
            }
        } else {
            eprint!("{}", line);
        }
    }

    fn flush(&self) {}
}

fn config_has_file_output_guard() -> bool {
    if let Ok(guard) = LOG_CONFIG.lock() {
        if let Some(ref config) = *guard {
            return config_has_file_output(config);
        }
    }
    false
}

/// Returns true if any logger in the config has file output.
pub fn config_has_file_output(config: &LogConfig) -> bool {
    config
        .loggers
        .values()
        .any(|def| matches!(def.output, LogOutput::File(_)))
}

/// Resolve conversation_output and debug_output defaults to plan_dir/logs/ when not set.
/// Returns the resolved conversation output path. Call when plan_dir becomes known.
/// When LogConfig already has file output, skips redirect to avoid overwriting user config.
pub fn resolve_log_defaults(
    conversation_output_path: Option<std::path::PathBuf>,
    debug_output_path: Option<impl AsRef<Path>>,
    plan_dir: &Path,
) -> Option<std::path::PathBuf> {
    let logs = plan_dir.join("logs");
    if debug_output_path.is_none() && !config_has_file_output_guard() {
        let _ = std::fs::create_dir_all(&logs);
        redirect_debug_output(&logs.join("debug.log"));
        log::set_max_level(log::LevelFilter::Debug);
    }
    if conversation_output_path.is_none() {
        let _ = std::fs::create_dir_all(&logs);
        Some(logs.join("conversation.jsonl"))
    } else {
        conversation_output_path
    }
}

/// Redirect default log output to a file. Use when plan_dir becomes known after init_tddy_logger.
/// When using LogConfig, updates the default logger's output in config.loggers and opens the file.
/// Flushes any buffered log lines (from TUI mode) into the file so early messages are preserved.
pub fn redirect_debug_output(path: &Path) {
    let path_buf = path.to_path_buf();
    if let Ok(mut guard) = LOG_CONFIG.lock() {
        if let Some(ref mut config) = *guard {
            if let Some(def) = config.loggers.get_mut(&config.default.logger) {
                def.output = LogOutput::File(path_buf.clone());
            }
            if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(path) {
                let buffered = take_buffered_logs();
                for line in &buffered {
                    let _ = writeln!(f, "{}", line);
                }
                if !buffered.is_empty() {
                    let _ = f.flush();
                }
                if let Ok(mut files) = FILE_OUTPUTS.lock() {
                    files.insert(path_buf, f);
                }
            }
            return;
        }
    }
    match OpenOptions::new().create(true).append(true).open(path) {
        Ok(mut f) => {
            let buffered = take_buffered_logs();
            for line in &buffered {
                let _ = writeln!(f, "{}", line);
            }
            if !buffered.is_empty() {
                let _ = f.flush();
            }
            if let Ok(mut guard) = DEBUG_OUTPUT_FILE.lock() {
                *guard = Some(f);
            }
        }
        Err(e) => {
            eprintln!(
                "[tddy-core] warning: could not open debug output file {:?}: {}",
                path, e
            );
        }
    }
}

fn validate_log_config(config: &LogConfig) -> Result<(), String> {
    if !config.loggers.contains_key(&config.default.logger) {
        return Err(format!(
            "log config: default logger '{}' not found in loggers",
            config.default.logger
        ));
    }
    for (i, policy) in config.policies.iter().enumerate() {
        let name = policy.logger.as_deref().unwrap_or(&config.default.logger);
        if !config.loggers.contains_key(name) {
            return Err(format!(
                "log config: policy {} references logger '{}' which is not defined",
                i, name
            ));
        }
    }
    Ok(())
}

/// Initialize the tddy logger with LogConfig. Call once at startup.
/// Replaces the legacy 3-arg init; uses config for routing.
pub fn init_tddy_logger(config: LogConfig) {
    if let Err(msg) = validate_log_config(&config) {
        panic!("{}", msg);
    }
    rotate_log_files(&config);

    let file_paths: Vec<PathBuf> = collect_file_outputs(&config);
    for path in &file_paths {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        match OpenOptions::new().create(true).append(true).open(path) {
            Ok(f) => {
                if let Ok(mut guard) = FILE_OUTPUTS.lock() {
                    guard.insert(path.clone(), f);
                }
            }
            Err(e) => {
                eprintln!(
                    "[tddy-core] warning: could not open log file {:?}: {}",
                    path, e
                );
            }
        }
    }

    let max_level = compute_max_level_from_config(&config);
    if let Ok(mut guard) = LOG_CONFIG.lock() {
        *guard = Some(config);
    }
    let _ = log::set_logger(&LOGGER).map(|()| log::set_max_level(max_level));
}

fn collect_file_outputs(config: &LogConfig) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    for def in config.loggers.values() {
        if let LogOutput::File(ref p) = def.output {
            if !paths.contains(p) {
                paths.push(p.clone());
            }
        }
    }
    paths
}

fn compute_max_level_from_config(config: &LogConfig) -> log::LevelFilter {
    config
        .policies
        .iter()
        .filter_map(|p| p.level)
        .chain(std::iter::once(config.default.level))
        .max_by_key(|l| *l as u8)
        .unwrap_or(config.default.level)
}

/// Rotate existing log files on startup: rename to {stem}.{ISO-8601}.{ext}, prune beyond max_rotated.
fn rotate_log_files(config: &LogConfig) {
    let max_rotated = config.rotation.as_ref().map(|r| r.max_rotated).unwrap_or(5);
    if max_rotated == 0 {
        return;
    }

    for path in collect_file_outputs(config) {
        if !path.exists() {
            continue;
        }
        let parent = path.parent().unwrap_or(Path::new("."));
        let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("log");
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| format!(".{}", e))
            .unwrap_or_else(|| "".to_string());
        let timestamp = chrono::Local::now().format("%Y-%m-%dT%H-%M-%S");
        let rotated_name = format!("{}.{}{}", stem, timestamp, ext);
        let rotated_path = parent.join(&rotated_name);
        if std::fs::rename(&path, &rotated_path).is_err() {
            continue;
        }

        let prefix = format!("{}.", stem);
        let entries = match std::fs::read_dir(parent) {
            Ok(rd) => rd,
            Err(e) => {
                eprintln!(
                    "[tddy-core] warning: cannot read log dir {:?} for rotation: {}",
                    parent, e
                );
                continue;
            }
        };
        let mut rotated: Vec<_> = entries
            .filter_map(|e| e.ok())
            .filter(|e| {
                let name = e.file_name();
                let s = name.to_str().unwrap_or("");
                s.starts_with(&prefix)
                    && s != path.file_name().and_then(|n| n.to_str()).unwrap_or("")
            })
            .collect();
        rotated.sort_by_key(|e| e.path());
        let to_remove = rotated.len().saturating_sub(max_rotated);
        for e in rotated.into_iter().take(to_remove) {
            let _ = std::fs::remove_file(e.path());
        }
    }
}

/// Legacy: Initialize the tddy logger with boolean/path args. Deprecated; use init_tddy_logger(LogConfig).
/// When `debug` is true (--debug flag), uses Debug level. Otherwise respects RUST_LOG or defaults to Info.
/// When `debug_output_path` is Some, enables Debug level and redirects logs to that file.
/// When `webrtc_debug_output_path` is Some, libwebrtc logs (connection.cc, etc.) go to that file instead.
#[allow(dead_code)]
pub fn init_tddy_logger_legacy(
    debug: bool,
    debug_output_path: Option<&Path>,
    webrtc_debug_output_path: Option<&Path>,
) {
    if let Some(path) = debug_output_path {
        match OpenOptions::new().create(true).append(true).open(path) {
            Ok(f) => {
                if let Ok(mut guard) = DEBUG_OUTPUT_FILE.lock() {
                    *guard = Some(f);
                }
            }
            Err(e) => {
                eprintln!(
                    "[tddy-core] warning: could not open debug output file {:?}: {}",
                    path, e
                );
            }
        }
    }
    if let Some(path) = webrtc_debug_output_path {
        match OpenOptions::new().create(true).append(true).open(path) {
            Ok(f) => {
                if let Ok(mut guard) = WEBRTC_DEBUG_OUTPUT_FILE.lock() {
                    *guard = Some(f);
                }
            }
            Err(e) => {
                eprintln!(
                    "[tddy-core] warning: could not open webrtc debug output file {:?}: {}",
                    path, e
                );
            }
        }
    }
    let level = if debug || debug_output_path.is_some() || webrtc_debug_output_path.is_some() {
        log::LevelFilter::Debug
    } else if std::env::var("TDDY_QUIET").is_ok() {
        // TUI mode: enable Debug from the start so early messages are buffered.
        // They'll be flushed to the debug file when redirect_debug_output opens it.
        log::LevelFilter::Debug
    } else {
        std::env::var("RUST_LOG")
            .ok()
            .and_then(|s| {
                s.parse::<log::LevelFilter>()
                    .ok()
                    .or_else(|| match s.to_lowercase().as_str() {
                        "off" => Some(log::LevelFilter::Off),
                        "error" => Some(log::LevelFilter::Error),
                        "warn" => Some(log::LevelFilter::Warn),
                        "info" => Some(log::LevelFilter::Info),
                        "debug" => Some(log::LevelFilter::Debug),
                        "trace" => Some(log::LevelFilter::Trace),
                        _ => None,
                    })
            })
            .unwrap_or(log::LevelFilter::Info)
    };

    let _ = log::set_logger(&LOGGER).map(|()| log::set_max_level(level));
}

/// Return recent buffered log lines for TUI display. Clears the buffer.
pub fn take_buffered_logs() -> Vec<String> {
    let mut buf = match LOG_BUFFER.lock() {
        Ok(b) => b,
        Err(e) => e.into_inner(),
    };
    std::mem::take(&mut *buf)
}

/// Return a clone of buffered log lines without clearing. For display during TUI draw.
pub fn get_buffered_logs() -> Vec<String> {
    let buf = match LOG_BUFFER.lock() {
        Ok(b) => b,
        Err(e) => e.into_inner(),
    };
    buf.clone()
}
