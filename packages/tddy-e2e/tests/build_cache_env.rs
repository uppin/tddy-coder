//! What `scripts/build-cache-env.sh` puts in the environment, per host config.
//!
//! Every case runs the real script against a config in a temp `HOME`, so what is asserted is the
//! environment a `./dev` invocation would actually inherit.

use std::fs;

use tddy_e2e::build_cache_contract::{
    job_level_env_lines, repo_root, resolve_in, resolver_path, verify_syntax,
    wires_build_cache_resolver, Resolution,
};
use tempfile::TempDir;

/// A config selecting `kind` while leaving every backend's coordinates in place — the shape the
/// guide documents, and the one that proves unselected blocks are ignored.
fn all_backends_selecting(kind: &str) -> String {
    format!(
        "# per-host build cache\n\
         sccache:\n  \
           type: {kind}        # local | redis | github-actions\n\
         \n  \
           local:\n    \
             dir: ~/.cache/sccache\n    \
             max_size: 20G\n\
         \n  \
           redis:\n    \
             url: redis://127.0.0.1:6379\n    \
             key_prefix: tddy\n"
    )
}

fn resolve(home: &TempDir, yaml: &str) -> Resolution {
    resolve_in(home.path()).with_config(yaml).run()
}

#[test]
fn resolver_bash_syntax() {
    // Given
    let path = resolver_path();

    // When / Then
    verify_syntax(&path);
}

#[test]
fn absent_config_leaves_sccache_off_and_says_so() {
    // Given a host that has never written ~/.tddy/build-cache.yaml
    let home = TempDir::new().unwrap();

    // When
    let resolved = resolve_in(home.path()).run();

    // Then the build still runs — it just runs uncached, and the notice says why
    assert!(
        resolved.succeeded,
        "an absent config must not fail the build"
    );
    resolved.expect_no_export("RUSTC_WRAPPER");
    resolved.expect_notice_containing("sccache off");
    resolved.expect_notice_containing("docs/dev/guides/build-cache.md");
}

#[test]
fn absent_config_writes_nothing_to_disk() {
    // Given
    let home = TempDir::new().unwrap();

    // When
    resolve_in(home.path()).run();

    // Then no cache directory is created behind the developer's back
    let entries: Vec<_> = fs::read_dir(home.path()).unwrap().collect();
    assert!(
        entries.is_empty(),
        "resolving an absent config must not create anything: {entries:?}"
    );
}

#[test]
fn local_backend_defaults_to_a_cache_under_home() {
    // Given a config naming the backend and nothing else
    let home = TempDir::new().unwrap();

    // When
    let resolved = resolve(&home, "sccache:\n  type: local\n");

    // Then
    let expected = home.path().join(".cache/sccache");
    resolved.expect_export("SCCACHE_DIR", expected.to_str().unwrap());
    resolved.expect_export("SCCACHE_CACHE_SIZE", "10G");
    assert!(resolved.enables_sccache());
}

#[test]
fn local_backend_takes_its_coordinates_from_the_config() {
    // Given
    let home = TempDir::new().unwrap();

    // When
    let resolved = resolve(&home, &all_backends_selecting("local"));

    // Then the `~/` is expanded against this host's home, and the size is the configured one
    let expected = home.path().join(".cache/sccache");
    resolved.expect_export("SCCACHE_DIR", expected.to_str().unwrap());
    resolved.expect_export("SCCACHE_CACHE_SIZE", "20G");
}

#[test]
fn selecting_one_backend_ignores_the_others_coordinates() {
    // Given a config carrying both a local and a redis block, with local selected
    let home = TempDir::new().unwrap();

    // When
    let resolved = resolve(&home, &all_backends_selecting("local"));

    // Then the redis coordinates sit there unused — switching back is a one-line edit
    resolved.expect_no_export("SCCACHE_REDIS");
    resolved.expect_no_export("SCCACHE_REDIS_KEY_PREFIX");
}

#[test]
fn redis_backend_exports_its_endpoint_and_prefix() {
    // Given the same file with the selector flipped
    let home = TempDir::new().unwrap();

    // When
    let resolved = resolve(&home, &all_backends_selecting("redis"));

    // Then
    resolved.expect_export("SCCACHE_REDIS", "redis://127.0.0.1:6379");
    resolved.expect_export("SCCACHE_REDIS_KEY_PREFIX", "tddy");
    resolved.expect_no_export("SCCACHE_DIR");
    assert!(resolved.enables_sccache());
}

#[test]
fn redis_notice_does_not_echo_the_password() {
    // Given a redis URL carrying credentials
    let home = TempDir::new().unwrap();
    let yaml =
        "sccache:\n  type: redis\n  redis:\n    url: redis://admin:hunter2@cache.internal:6379/1\n";

    // When
    let resolved = resolve(&home, yaml);

    // Then the endpoint reaches sccache but the secret never reaches a build log
    resolved.expect_export(
        "SCCACHE_REDIS",
        "redis://admin:hunter2@cache.internal:6379/1",
    );
    assert!(
        !resolved.notices.contains("hunter2"),
        "the notice leaked the password: {:?}",
        resolved.notices
    );
    resolved.expect_notice_containing("redis://cache.internal:6379/1");
}

#[test]
fn redis_without_an_endpoint_is_refused() {
    // Given a selector with no coordinates behind it
    let home = TempDir::new().unwrap();

    // When
    let resolved = resolve(&home, "sccache:\n  type: redis\n");

    // Then the run stops rather than compiling uncached under a config that claims a cache
    assert!(!resolved.succeeded);
    resolved.expect_notice_containing("redis.url");
}

#[test]
fn github_actions_backend_uses_the_runners_credentials() {
    // Given a runner, which names the backend by env and never writes a config file
    let home = TempDir::new().unwrap();

    // When
    let resolved = resolve_in(home.path())
        .with_env("TDDY_BUILD_CACHE", "github-actions")
        .with_env("ACTIONS_RESULTS_URL", "https://results.actions.example/")
        .with_env("ACTIONS_RUNTIME_TOKEN", "runner-token")
        .run();

    // Then
    resolved.expect_export("SCCACHE_GHA_ENABLED", "true");
    resolved.expect_export("ACTIONS_RESULTS_URL", "https://results.actions.example/");
    resolved.expect_export("ACTIONS_RUNTIME_TOKEN", "runner-token");
    assert!(resolved.enables_sccache());
}

#[test]
fn github_actions_backend_without_credentials_is_refused() {
    // Given the backend named on a runner that never exposed the cache service
    let home = TempDir::new().unwrap();

    // When
    let resolved = resolve_in(home.path())
        .with_env("TDDY_BUILD_CACHE", "github-actions")
        .run();

    // Then it fails loudly — an sccache that cannot reach its cache compiles everything anyway,
    // and the only symptom would be a slow job
    assert!(!resolved.succeeded);
    resolved.expect_notice_containing("ACTIONS_RESULTS_URL");
}

#[test]
fn the_env_selector_beats_the_config_file() {
    // Given a host config selecting a local cache
    let home = TempDir::new().unwrap();

    // When the environment names a different backend
    let resolved = resolve_in(home.path())
        .with_config(&all_backends_selecting("local"))
        .with_env("TDDY_BUILD_CACHE", "off")
        .run();

    // Then the file is not consulted at all
    assert!(resolved.succeeded);
    resolved.expect_no_export("SCCACHE_DIR");
    resolved.expect_no_export("RUSTC_WRAPPER");
}

#[test]
fn an_explicit_off_is_silent() {
    // Given a host that has deliberately opted out
    let home = TempDir::new().unwrap();

    // When
    let resolved = resolve(&home, "sccache:\n  type: off\n");

    // Then nothing is exported and nothing is said — the notice exists to prompt a setup that
    // this host has already answered
    assert!(resolved.succeeded);
    resolved.expect_no_export("RUSTC_WRAPPER");
    assert_eq!(resolved.notices, "");
}

#[test]
fn an_unknown_backend_is_refused() {
    // Given a typo, or a backend nobody wired up
    let home = TempDir::new().unwrap();

    // When
    let resolved = resolve(&home, "sccache:\n  type: s3\n");

    // Then
    assert!(!resolved.succeeded);
    resolved.expect_notice_containing("unknown sccache.type 's3'");
}

#[test]
fn a_config_without_a_selector_is_refused() {
    // Given coordinates but nothing selecting them
    let home = TempDir::new().unwrap();

    // When
    let resolved = resolve(&home, "sccache:\n  local:\n    dir: /var/cache/sccache\n");

    // Then
    assert!(!resolved.succeeded);
    resolved.expect_notice_containing("no 'sccache.type'");
}

#[test]
fn every_enabled_backend_turns_off_incremental_compilation() {
    // Given each backend that actually caches
    let cases = [
        ("sccache:\n  type: local\n".to_string(), Vec::new()),
        (all_backends_selecting("redis"), Vec::new()),
        (
            String::new(),
            vec![
                ("TDDY_BUILD_CACHE", "github-actions"),
                ("ACTIONS_RESULTS_URL", "https://results.actions.example/"),
                ("ACTIONS_RUNTIME_TOKEN", "runner-token"),
            ],
        ),
    ];

    for (yaml, env) in cases {
        // When
        let home = TempDir::new().unwrap();
        let mut run = resolve_in(home.path());
        if !yaml.is_empty() {
            run = run.with_config(&yaml);
        }
        for (name, value) in &env {
            run = run.with_env(name, value);
        }
        let resolved = run.run();

        // Then — sccache declines any unit carrying `-C incremental`, so leaving cargo's dev
        // default on would keep the cache permanently empty
        resolved.expect_export("CARGO_INCREMENTAL", "0");
    }
}

#[test]
fn quiet_suppresses_the_notice_but_not_the_exports() {
    // Given the direnv path, which runs on every cd into the tree
    let home = TempDir::new().unwrap();

    // When
    let resolved = resolve_in(home.path())
        .with_config(&all_backends_selecting("local"))
        .quiet()
        .run();

    // Then
    assert_eq!(resolved.notices, "");
    assert!(resolved.enables_sccache());
}

#[test]
fn dev_and_direnv_both_resolve_through_the_shared_script() {
    // Given the two ways into the dev shell
    let dev = fs::read_to_string(repo_root().join("dev")).unwrap();
    let envrc = fs::read_to_string(repo_root().join(".envrc")).unwrap();

    // When / Then — neither may grow its own answer, or the two paths would drift
    assert!(
        wires_build_cache_resolver(&dev),
        "`dev` must resolve the build cache"
    );
    assert!(
        wires_build_cache_resolver(&envrc),
        "`.envrc` must resolve the build cache"
    );
}

#[test]
fn ci_names_the_backend_for_every_rust_job() {
    // Given the CI workflow
    let ci = fs::read_to_string(repo_root().join(".github/workflows/ci.yml")).unwrap();

    // When
    let selectors = ci.matches("TDDY_BUILD_CACHE: github-actions").count();
    let credentials = ci.matches("Expose the Actions cache to sccache").count();

    // Then every Rust job — lint, test, build, build-arm64 — is preconfigured, and each has the
    // credential step without which the resolver refuses to run
    assert_eq!(
        selectors, 4,
        "expected all four Rust jobs to name the backend"
    );
    assert_eq!(
        credentials, 4,
        "expected all four Rust jobs to expose the cache service"
    );
}

#[test]
fn no_job_level_env_reaches_for_the_runner_context() {
    // Given every workflow in the repo
    let workflows = fs::read_dir(repo_root().join(".github/workflows")).unwrap();

    for entry in workflows {
        let path = entry.unwrap().path();
        let workflow = fs::read_to_string(&path).unwrap();

        // When
        let offenders: Vec<_> = job_level_env_lines(&workflow)
            .into_iter()
            .filter(|line| line.contains("runner."))
            .collect();

        // Then — a job's `env:` is evaluated before a runner is assigned, so naming that context
        // there is not a bad value but an invalid workflow: GitHub rejects the whole file and
        // starts no jobs at all, which reads as a red check with nothing in it to look at.
        // `runner.*` belongs in a step, or in a step that exports it.
        assert!(
            offenders.is_empty(),
            "{} uses the runner context in a job-level env: {offenders:?}",
            path.display()
        );
    }
}
