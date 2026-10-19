//! Acceptance tests: `tddy-coder` carries no hardcoded agent.
//!
//! Feature: docs/ft/daemon/session-agent-roster.md (AC45, AC46)
//!
//! Replaces `fastcontext_backend_acceptance.rs`, which asserted the opposite: that the CLI accepted
//! one hardcoded agent name and that `dev.daemon.yaml` listed it under `allowed_agents`.
//!
//! What is checked here is only what the *binary* exposes — its argument surface and its help text.
//! Whether `create_backend` builds a backend from a def and refuses an unknown name is checked in
//! `run.rs`'s own test module, where the function is callable directly and no process needs
//! spawning to observe a return value.

use assert_cmd::cargo::cargo_bin_cmd;
use assert_cmd::Command;

mod common;

// ---------------------------------------------------------------------------
// Builders
// ---------------------------------------------------------------------------

/// A `<tddyhome>` whose `agents/` directory holds one def.
fn a_tddy_home_defining(name: &str) -> tempfile::TempDir {
    let home = tempfile::tempdir().expect("tddyhome tempdir");
    let agents = home.path().join("agents");
    std::fs::create_dir_all(&agents).expect("create agents dir");
    std::fs::write(
        agents.join(format!("{name}.yaml")),
        format!(
            "name: {name}\nlabel: \"{name}\"\nmodel: qwen2.5-coder:7b\n\
             base_url: http://localhost:11434\ntools: [READ, GLOB, GREP]\n\
             max_turns: 4\nreplaces: []\n"
        ),
    )
    .expect("write agent def");
    home
}

fn run_coder(home: &tempfile::TempDir, args: &[&str]) -> std::process::Output {
    let mut cmd: Command = cargo_bin_cmd!("tddy-coder");
    cmd.args(["--tddy-data-dir", home.path().to_str().expect("utf-8 path")])
        .args(args);
    cmd.output().expect("tddy-coder binary must be runnable")
}

fn repo_root() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .find(|p| p.join("dev.daemon.yaml").exists())
        .expect("dev.daemon.yaml must exist in an ancestor of tddy-coder")
        .to_path_buf()
}

// ---------------------------------------------------------------------------
// AC46 — a def is the whole configuration
// ---------------------------------------------------------------------------

/// The endpoint override is gone. While it existed, a session could run against a model the
/// operator never configured while still reporting the def's name — the def said one thing and the
/// process did another.
#[test]
#[cfg(unix)]
fn no_longer_accepts_a_flag_that_could_override_a_defs_endpoint() {
    // Given
    let home = a_tddy_home_defining("my-explorer");

    // When
    let output = run_coder(
        &home,
        &[
            "--agent",
            "my-explorer",
            "--fastcontext-url",
            "http://elsewhere:9999",
        ],
    );

    // Then
    assert!(
        !output.status.success(),
        "the removed flag must be rejected"
    );
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("--fastcontext-url"),
        "clap must name the argument it does not recognise, was: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

/// The model override goes with it — one flag left behind is one hardcoded default still alive.
#[test]
#[cfg(unix)]
fn no_longer_accepts_a_flag_that_could_override_a_defs_model() {
    // Given
    let home = a_tddy_home_defining("my-explorer");

    // When
    let output = run_coder(
        &home,
        &[
            "--agent",
            "my-explorer",
            "--fastcontext-model",
            "something-else",
        ],
    );

    // Then
    assert!(
        !output.status.success(),
        "the removed flag must be rejected"
    );
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("--fastcontext-model"),
        "clap must name the argument it does not recognise"
    );
}

/// And the turn-budget override, for the same reason.
#[test]
#[cfg(unix)]
fn no_longer_accepts_a_flag_that_could_override_a_defs_turn_budget() {
    // Given
    let home = a_tddy_home_defining("my-explorer");

    // When
    let output = run_coder(
        &home,
        &["--agent", "my-explorer", "--fastcontext-max-turns", "99"],
    );

    // Then
    assert!(
        !output.status.success(),
        "the removed flag must be rejected"
    );
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("--fastcontext-max-turns"),
        "clap must name the argument it does not recognise"
    );
}

/// An operator-defined agent is still an accepted `--agent` value — the flags went away, the
/// ability to name your own agent did not.
#[test]
#[cfg(unix)]
fn still_accepts_an_operator_defined_agent_name() {
    // Given
    let home = a_tddy_home_defining("my-explorer");

    // When — `--help` exits before any backend is constructed, so this checks argument acceptance
    let output = run_coder(&home, &["--agent", "my-explorer", "--help"]);

    // Then
    assert!(
        output.status.success(),
        "`--agent my-explorer --help` must exit 0; stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

// ---------------------------------------------------------------------------
// AC45 — the name is gone from the shipped surface too
// ---------------------------------------------------------------------------

/// `--help` is the surface an operator reads to learn what exists. A hardcoded agent surviving
/// there is a hardcoded agent, whatever the code does.
#[test]
#[cfg(unix)]
fn the_help_text_names_no_hardcoded_agent() {
    // Given
    let home = a_tddy_home_defining("my-explorer");

    // When
    let output = run_coder(&home, &["--help"]);

    // Then
    let help = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        !help.to_ascii_lowercase().contains("fastcontext"),
        "--help must not name a hardcoded agent; got:\n{help}"
    );
}

/// The shipped dev config listed the builtin under `allowed_agents`, which is how it stayed
/// startable without any def file. Removing the builtin means removing that entry too, or the
/// hardcoding has simply moved into YAML we ship.
#[test]
fn the_shipped_dev_daemon_config_lists_no_hardcoded_agent() {
    // Given
    let config_path = repo_root().join("dev.daemon.yaml");

    // When
    let contents = std::fs::read_to_string(&config_path)
        .unwrap_or_else(|e| panic!("must be able to read {}: {e}", config_path.display()));

    // Then
    assert!(
        !contents.to_ascii_lowercase().contains("fastcontext"),
        "dev.daemon.yaml must not name a hardcoded agent; contents:\n{contents}"
    );
}
