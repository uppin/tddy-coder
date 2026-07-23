//! Verifies the engine emits adequate logging at its key seams. Runs in its own
//! test binary because `log` has a single process-global logger.

use std::sync::{Mutex, OnceLock};

use tddy_build::discovery::discover_build_manifests;
use tddy_build::executor::{execute_target, ExecuteOptions};
use tddy_build::graph::BuildGraph;
use tddy_build::plugin::PluginRegistry;

static LOG_BUFFER: OnceLock<Mutex<Vec<String>>> = OnceLock::new();

fn buffer() -> &'static Mutex<Vec<String>> {
    LOG_BUFFER.get_or_init(|| Mutex::new(Vec::new()))
}

struct CaptureLogger;

impl log::Log for CaptureLogger {
    fn enabled(&self, _: &log::Metadata) -> bool {
        true
    }
    fn log(&self, record: &log::Record) {
        buffer()
            .lock()
            .unwrap()
            .push(format!("{} {}", record.level(), record.args()));
    }
    fn flush(&self) {}
}

static LOGGER: CaptureLogger = CaptureLogger;

fn install_logger() {
    let _ = log::set_logger(&LOGGER);
    log::set_max_level(log::LevelFilter::Trace);
}

fn captured() -> Vec<String> {
    buffer().lock().unwrap().clone()
}

fn assert_logged(needle: &str) {
    let lines = captured();
    assert!(
        lines.iter().any(|l| l.contains(needle)),
        "expected a log line containing {needle:?}; captured:\n{}",
        lines.join("\n")
    );
}

const SCRIPT_YAML: &str = r#"
schema_version: 1
targets:
  - id: "log:demo"
    name: "Log Demo"
    actions:
      - id: "write"
        type: command
        command: ["sh", "-c", "echo run >> marker.txt"]
        inputs:
          - include: ["input.txt"]
            root: "."
        outputs:
          - path: "marker.txt"
            kind: file
"#;

const CYCLE_YAML: &str = r#"
schema_version: 1
targets:
  - id: "a:t"
    name: A
    deps: ["b:t"]
    config: { type: script, command: ["true"] }
  - id: "b:t"
    name: B
    deps: ["a:t"]
    config: { type: script, command: ["true"] }
"#;

#[tokio::test]
async fn engine_logs_discovery_lowering_cache_and_cycles() {
    // Given
    install_logger();
    let repo = tempfile::tempdir().expect("tempdir");
    let root = repo.path();
    std::fs::write(root.join("BUILD.yaml"), SCRIPT_YAML).expect("write BUILD.yaml");
    std::fs::write(root.join("input.txt"), "seed").expect("seed input");

    // When — discovery
    let discovered = discover_build_manifests(root).expect("discover");
    let manifests = discovered.into_iter().map(|(_, m)| m).collect::<Vec<_>>();

    // Then
    assert_logged("manifest");

    // When — first run: lowering + cache miss + execution
    let graph = BuildGraph::from_manifests(manifests).expect("graph");
    let opts = ExecuteOptions::default();
    let registry = PluginRegistry::new();
    let first = execute_target(
        root,
        &graph,
        "log:demo",
        &opts,
        tddy_build::BuildMode::Compile,
        &registry,
    )
    .await
    .expect("first run");

    // Then
    assert!(!first.actions[0].cached);
    assert_logged("lowered target");
    assert_logged("building target");
    assert_logged("cache miss");

    // When — second run: cache hit
    let second = execute_target(
        root,
        &graph,
        "log:demo",
        &opts,
        tddy_build::BuildMode::Compile,
        &registry,
    )
    .await
    .expect("second run");

    // Then
    assert!(second.actions[0].cached);
    assert_logged("cache hit");

    // When — cycle detection
    let cyclic = tddy_build::load_build_manifest(CYCLE_YAML).expect("parse cyclic");
    assert!(BuildGraph::from_manifests(vec![cyclic]).is_err());

    // Then
    assert_logged("cycle");
}
