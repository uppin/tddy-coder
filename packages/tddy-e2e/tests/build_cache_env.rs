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
             endpoint: rediss://cache.internal:16380\n    \
             password: hunter2\n    \
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
    resolved.expect_no_export("SCCACHE_REDIS_ENDPOINT");
    resolved.expect_no_export("SCCACHE_REDIS_PASSWORD");
    resolved.expect_no_export("SCCACHE_REDIS_KEY_PREFIX");
}

#[test]
fn redis_backend_exports_its_endpoint_and_prefix() {
    // Given the same file with the selector flipped
    let home = TempDir::new().unwrap();

    // When
    let resolved = resolve(&home, &all_backends_selecting("redis"));

    // Then
    resolved.expect_export("SCCACHE_REDIS_ENDPOINT", "rediss://cache.internal:16380");
    resolved.expect_export("SCCACHE_REDIS_PASSWORD", "hunter2");
    resolved.expect_export("SCCACHE_REDIS_KEY_PREFIX", "tddy");
    resolved.expect_no_export("SCCACHE_DIR");
    assert!(resolved.enables_sccache());
}

#[test]
fn redis_notice_does_not_echo_the_password() {
    // Given a redis backend with credentials
    let home = TempDir::new().unwrap();

    // When
    let resolved = resolve(&home, &all_backends_selecting("redis"));

    // Then the password reaches sccache but never a build log. It is exported apart from the
    // endpoint for exactly this reason: sccache's older single-URL form prints the URL it was
    // given — password and all — in `sccache --show-stats`.
    resolved.expect_export("SCCACHE_REDIS_PASSWORD", "hunter2");
    assert!(
        !resolved.notices.contains("hunter2"),
        "the notice leaked the password: {:?}",
        resolved.notices
    );
    resolved.expect_notice_containing("rediss://cache.internal:16380");
}

#[test]
fn redis_without_an_endpoint_is_refused() {
    // Given a selector with no coordinates behind it
    let home = TempDir::new().unwrap();

    // When
    let resolved = resolve(&home, "sccache:\n  type: redis\n");

    // Then the run stops rather than compiling uncached under a config that claims a cache
    assert!(!resolved.succeeded);
    resolved.expect_notice_containing("redis.endpoint");
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
fn ci_caches_through_redis_and_keeps_the_password_a_secret() {
    // Given the CI workflow
    let ci = fs::read_to_string(repo_root().join(".github/workflows/ci.yml")).unwrap();

    // When
    let selectors = ci.matches("TDDY_BUILD_CACHE: redis").count();

    // Then all four Rust jobs — lint, test, build, build-arm64 — cache. Redis rather than the
    // Actions cache: sccache writes one entry per compilation unit, and this repo's Actions
    // cache is already at GitHub's hard 10 GB ceiling, where those entries evict
    // `Swatinem/rust-cache`'s multi-GB `target/` archives. See docs/dev/guides/build-cache.md.
    assert_eq!(
        selectors, 4,
        "expected all four Rust jobs to name the redis backend"
    );

    // And the credential is only ever a secret reference. A literal here would be committed,
    // and would also defeat the runner's log masking.
    assert!(
        ci.contains("SCCACHE_REDIS_PASSWORD: ${{ secrets.SCCACHE_REDIS_PASSWORD }}"),
        "the redis password must come from a repo secret, never a literal"
    );
    assert!(
        !ci.contains("SCCACHE_REDIS_ENDPOINT: rediss://:"),
        "the endpoint must not carry inline credentials — sccache prints the endpoint in its stats"
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

#[test]
fn redis_backend_takes_its_coordinates_from_the_environment() {
    // Given a runner: the backend named by env, the password arriving from a secret, and no
    // per-host file anywhere
    let home = TempDir::new().unwrap();

    // When
    let resolved = resolve_in(home.path())
        .with_env("TDDY_BUILD_CACHE", "redis")
        .with_env("SCCACHE_REDIS_ENDPOINT", "rediss://cache.internal:16380")
        .with_env("SCCACHE_REDIS_PASSWORD", "from-a-secret")
        .with_env("SCCACHE_REDIS_KEY_PREFIX", "tddy-ci")
        .run();

    // Then
    resolved.expect_export("SCCACHE_REDIS_ENDPOINT", "rediss://cache.internal:16380");
    resolved.expect_export("SCCACHE_REDIS_PASSWORD", "from-a-secret");
    resolved.expect_export("SCCACHE_REDIS_KEY_PREFIX", "tddy-ci");
    assert!(resolved.enables_sccache());
    assert!(
        !resolved.notices.contains("from-a-secret"),
        "the notice leaked the secret: {:?}",
        resolved.notices
    );
}

#[test]
fn a_config_endpoint_beats_one_from_the_environment() {
    // Given both a per-host file and an inherited environment
    let home = TempDir::new().unwrap();

    // When
    let resolved = resolve_in(home.path())
        .with_config(&all_backends_selecting("redis"))
        .with_env("SCCACHE_REDIS_ENDPOINT", "rediss://wrong.example:16380")
        .with_env("SCCACHE_REDIS_PASSWORD", "wrong")
        .run();

    // Then the file wins — the environment is the fallback for a host that has no file, not an
    // override of one that does
    resolved.expect_export("SCCACHE_REDIS_ENDPOINT", "rediss://cache.internal:16380");
    resolved.expect_export("SCCACHE_REDIS_PASSWORD", "hunter2");
}
