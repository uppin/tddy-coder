//! Harness for the repo-root build cache resolver, `scripts/build-cache-env.sh`.
//!
//! The script is pure: it reads a config and prints `export` lines, touching nothing. So these
//! helpers run it for real against a config written into a temp dir, rather than grepping its
//! source — what matters is the environment a caller ends up with, not the text that produced it.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

/// Repo root, from this crate's manifest.
pub fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
}

/// The resolver script under test.
pub fn resolver_path() -> PathBuf {
    repo_root().join("scripts").join("build-cache-env.sh")
}

/// One run of the resolver: what it exported, what it said, and whether it succeeded.
#[derive(Debug)]
pub struct Resolution {
    pub exports: BTreeMap<String, String>,
    pub notices: String,
    pub succeeded: bool,
}

impl Resolution {
    /// The value the run exported under `name`, if it exported one at all.
    pub fn export(&self, name: &str) -> Option<&str> {
        self.exports.get(name).map(String::as_str)
    }

    /// True when the run wired sccache up as the compiler wrapper.
    pub fn enables_sccache(&self) -> bool {
        self.export("RUSTC_WRAPPER") == Some("sccache")
    }

    /// Panics unless the run exported `name` with exactly `value`.
    pub fn expect_export(&self, name: &str, value: &str) {
        assert_eq!(
            self.export(name),
            Some(value),
            "expected {name}={value}; got exports {:?} and notices {:?}",
            self.exports,
            self.notices
        );
    }

    /// Panics unless the run exported nothing under `name`.
    pub fn expect_no_export(&self, name: &str) {
        assert_eq!(
            self.export(name),
            None,
            "expected no {name}; got exports {:?}",
            self.exports
        );
    }

    /// Panics unless the run's notices mention `fragment`.
    pub fn expect_notice_containing(&self, fragment: &str) {
        assert!(
            self.notices.contains(fragment),
            "expected a notice mentioning {fragment:?}; got {:?}",
            self.notices
        );
    }
}

/// A resolver invocation under construction.
///
/// The host's own `~/.tddy/build-cache.yaml` and any inherited `TDDY_BUILD_CACHE` /
/// `ACTIONS_*` must not reach the script, or a developer's machine would decide the
/// result — so a run starts from a temp `HOME`, a config path that does not exist,
/// and those variables cleared.
pub struct ResolverRun {
    home: PathBuf,
    config_path: PathBuf,
    config: Option<String>,
    env: Vec<(String, String)>,
    quiet: bool,
}

/// Start building a resolver invocation. `home` is a temp dir the caller owns.
pub fn resolve_in(home: &Path) -> ResolverRun {
    ResolverRun {
        home: home.to_path_buf(),
        config_path: home.join("absent-build-cache.yaml"),
        config: None,
        env: Vec::new(),
        quiet: false,
    }
}

impl ResolverRun {
    /// Give the run a `~/.tddy/build-cache.yaml` holding `yaml`.
    pub fn with_config(mut self, yaml: &str) -> Self {
        self.config = Some(yaml.to_string());
        self.config_path = self.home.join("build-cache.yaml");
        self
    }

    /// Set an environment variable for the run.
    pub fn with_env(mut self, name: &str, value: &str) -> Self {
        self.env.push((name.to_string(), value.to_string()));
        self
    }

    /// Pass `--quiet`.
    pub fn quiet(mut self) -> Self {
        self.quiet = true;
        self
    }

    /// Run it.
    pub fn run(self) -> Resolution {
        if let Some(yaml) = &self.config {
            std::fs::write(&self.config_path, yaml).unwrap_or_else(|e| {
                panic!("write {}: {e}", self.config_path.display());
            });
        }

        let script = resolver_path();
        let mut command = Command::new("bash");
        command.arg(&script);
        if self.quiet {
            command.arg("--quiet");
        }
        command
            .env("HOME", &self.home)
            .env("TDDY_BUILD_CACHE_CONFIG", &self.config_path)
            .env_remove("TDDY_BUILD_CACHE")
            .env_remove("ACTIONS_RESULTS_URL")
            .env_remove("ACTIONS_RUNTIME_TOKEN")
            .env_remove("SCCACHE_DIR")
            .env_remove("SCCACHE_REDIS")
            .env_remove("RUSTC_WRAPPER");
        for (name, value) in &self.env {
            command.env(name, value);
        }

        let output = command
            .output()
            .unwrap_or_else(|e| panic!("spawn {}: {e}", script.display()));
        into_resolution(output)
    }
}

fn into_resolution(output: Output) -> Resolution {
    let stdout = String::from_utf8(output.stdout).expect("resolver stdout must be UTF-8");
    let notices = String::from_utf8(output.stderr).expect("resolver stderr must be UTF-8");
    Resolution {
        exports: parse_exports(&stdout),
        notices,
        succeeded: output.status.success(),
    }
}

/// Read `export NAME='value'` lines back into a map, undoing the script's quoting.
fn parse_exports(stdout: &str) -> BTreeMap<String, String> {
    stdout
        .lines()
        .filter_map(|line| line.strip_prefix("export "))
        .filter_map(|assignment| assignment.split_once('='))
        .map(|(name, quoted)| (name.to_string(), unquote(quoted)))
        .collect()
}

/// `'a'\''b'` — the only quoting the script emits — back to `a'b`.
fn unquote(quoted: &str) -> String {
    let inner = quoted
        .strip_prefix('\'')
        .and_then(|rest| rest.strip_suffix('\''))
        .unwrap_or(quoted);
    inner.replace("'\\''", "'")
}

/// `bash -n` must accept the script.
pub fn verify_syntax(path: &Path) {
    let path_str = path.to_str().expect("script path must be UTF-8");
    let status = Command::new("bash")
        .args(["-n", path_str])
        .status()
        .unwrap_or_else(|e| panic!("spawn bash -n for {path_str}: {e}"));
    assert!(status.success(), "bash -n rejected {path_str}");
}

/// Returns true if the caller resolves the build cache through the shared script.
pub fn wires_build_cache_resolver(contents: &str) -> bool {
    let ok = contents.contains("build-cache-env.sh");
    log::debug!("wires_build_cache_resolver ok={ok}");
    ok
}

/// The assignments inside every **job-level** `env:` block of a workflow file, comments stripped.
///
/// Indentation is the discriminator: a job's `env:` sits at four spaces with its entries at six,
/// while a step's sits deeper. That distinction is the whole point — the two blocks do not have
/// the same contexts available to them.
///
/// Comments come out because they are prose, and prose about a context is not a use of it.
pub fn job_level_env_lines(workflow: &str) -> Vec<String> {
    let mut lines = Vec::new();
    let mut inside = false;
    for line in workflow.lines() {
        if line == "    env:" {
            inside = true;
            continue;
        }
        if !inside {
            continue;
        }
        let is_entry = line.starts_with("      ") && !line.starts_with("       ");
        if !is_entry {
            inside = false;
            continue;
        }
        let assignment = strip_comment(line);
        if !assignment.trim().is_empty() {
            lines.push(assignment);
        }
    }
    lines
}

/// Drop a whole-line `#` comment, or an inline one — which in YAML is a `#` preceded by a space.
fn strip_comment(line: &str) -> String {
    if line.trim_start().starts_with('#') {
        return String::new();
    }
    match line.find(" #") {
        Some(at) => line[..at].to_string(),
        None => line.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::job_level_env_lines;

    const WORKFLOW: &str = "\
jobs:
  build:
    runs-on: ubuntu-latest
    env:
      # runner. mentioned in prose is not a use of it
      GOOD: value # trailing note
      BAD: ${{ runner.arch }}
    steps:
      - uses: actions/checkout@v7
        env:
          STEP_LEVEL: ${{ runner.arch }}
";

    #[test]
    fn reads_a_jobs_env_entries_without_their_comments() {
        // Given / When
        let lines = job_level_env_lines(WORKFLOW);

        // Then the prose comment is gone and the inline note is trimmed off its assignment
        assert_eq!(
            lines,
            vec!["      GOOD: value", "      BAD: ${{ runner.arch }}"]
        );
    }

    #[test]
    fn a_job_level_runner_reference_is_visible() {
        // Given / When
        let offenders: Vec<_> = job_level_env_lines(WORKFLOW)
            .into_iter()
            .filter(|line| line.contains("runner."))
            .collect();

        // Then only the real assignment is reported — not the comment, and not the step's `env:`,
        // where the runner context is perfectly legal
        assert_eq!(offenders, vec!["      BAD: ${{ runner.arch }}"]);
    }
}
