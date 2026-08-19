//! Acceptance tests: there is no builtin agent, and no hardcoded agent behaviour
//! (`tddy_discovery::agent_def`, `tddy_discovery::subagent`).
//!
//! Feature: docs/ft/daemon/session-agent-roster.md (AC44, AC45)
//!
//! `resolve_agent_defs` used to seed itself from `builtin_agent_defs()`, so an empty
//! `<tddyhome>/agents/` still yielded one agent — `fastcontext` — with a hardcoded model, a
//! hardcoded endpoint and a `replaces` list that `subagent_replaced_tools` special-cased by name.
//! Every agent now comes from a def source an operator wrote. The last test here is a property of
//! the source tree rather than of a value, because "we deleted it" is exactly the claim that
//! decays back to false one convenience default at a time.

use std::path::{Path, PathBuf};

use tddy_discovery::agent_def::{load_agent_defs, resolve_agent_defs, SpecializedAgentDef};
use tddy_discovery::subagent::resolve_replaced_tools_for_defs;

// ---------------------------------------------------------------------------------------------
// Builders
// ---------------------------------------------------------------------------------------------

/// An agents directory holding exactly the YAML files a test names.
fn an_agents_dir_with(files: &[(&str, &str)]) -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("tempdir");
    for (filename, contents) in files {
        std::fs::write(dir.path().join(filename), contents).expect("write agent def");
    }
    dir
}

/// A minimal well-formed def, as YAML.
fn a_def_yaml(name: &str, model: &str) -> String {
    format!("name: {name}\nmodel: {model}\nbase_url: http://localhost:11434\n")
}

/// An in-memory def, for the pure replaced-set computation.
fn a_def(name: &str, replaces: &[&str]) -> SpecializedAgentDef {
    SpecializedAgentDef {
        name: name.to_string(),
        label: None,
        model: "qwen2.5-coder:7b".to_string(),
        base_url: "http://localhost:11434".to_string(),
        api_key: None,
        system_prompt: None,
        system_prompt_path: None,
        tools: Vec::new(),
        max_turns: 10,
        replaces: replaces.iter().map(|r| r.to_string()).collect(),
    }
}

// ---------------------------------------------------------------------------------------------
// Assertions
// ---------------------------------------------------------------------------------------------

trait DefSetAssertions {
    fn assert_names(&self, expected: &[&str]) -> &Self;
}

impl DefSetAssertions for Vec<SpecializedAgentDef> {
    fn assert_names(&self, expected: &[&str]) -> &Self {
        let mut actual: Vec<&str> = self.iter().map(|d| d.name.as_str()).collect();
        actual.sort_unstable();
        let mut expected_sorted = expected.to_vec();
        expected_sorted.sort_unstable();
        assert_eq!(actual, expected_sorted, "resolved agent def names mismatch");
        self
    }
}

// ---------------------------------------------------------------------------------------------
// AC44 — every agent comes from a def source
// ---------------------------------------------------------------------------------------------

/// The property the builtin removal is about: a host with no agent defs offers no agents. What
/// used to happen instead was one agent appearing out of the binary, with an endpoint on
/// `localhost:30000` that nothing on the host was necessarily serving.
#[test]
fn resolves_no_agents_at_all_from_an_empty_directory() {
    // Given
    let dir = an_agents_dir_with(&[]);

    // When
    let defs = resolve_agent_defs(dir.path());

    // Then
    defs.assert_names(&[]);
}

/// A directory that does not exist is the same answer as an empty one — a fresh `<tddyhome>` is
/// the common case, not an error, and it is still no agents rather than one.
#[test]
fn resolves_no_agents_from_a_directory_that_does_not_exist() {
    // Given
    let dir = an_agents_dir_with(&[]);
    let missing = dir.path().join("agents-that-were-never-created");

    // When
    let defs = resolve_agent_defs(&missing);

    // Then
    defs.assert_names(&[]);
}

/// What a directory defines is exactly what resolves — nothing is added underneath it.
#[test]
fn resolves_exactly_the_agents_a_directory_defines() {
    // Given
    let dir = an_agents_dir_with(&[
        ("explorer.yaml", &a_def_yaml("explorer", "qwen2.5-coder:7b")),
        ("linter.yaml", &a_def_yaml("linter", "qwen2.5-coder:14b")),
    ]);

    // When
    let defs = resolve_agent_defs(dir.path());

    // Then
    defs.assert_names(&["explorer", "linter"]);
}

/// A def named `fastcontext` is an ordinary def like any other — it resolves from the file and
/// carries the file's own model, with nothing merged in from a shipped default.
#[test]
fn treats_a_def_named_after_the_old_builtin_as_an_ordinary_def() {
    // Given
    let dir = an_agents_dir_with(&[(
        "fastcontext.yaml",
        &a_def_yaml("fastcontext", "my-local-model:8b"),
    )]);

    // When
    let defs = resolve_agent_defs(dir.path());

    // Then
    defs.assert_names(&["fastcontext"]);
    assert_eq!(
        defs[0].model, "my-local-model:8b",
        "the file's own model must win outright — there is no shipped default to merge with"
    );
    assert_eq!(defs[0].base_url, "http://localhost:11434");
}

/// `load_agent_defs` and `resolve_agent_defs` now answer identically, because resolution no longer
/// adds anything to what was loaded. Pinned so a future "just one convenience default" reopens a
/// failing test rather than a quiet behaviour change.
#[test]
fn resolving_adds_nothing_to_what_was_loaded() {
    // Given
    let dir = an_agents_dir_with(&[("explorer.yaml", &a_def_yaml("explorer", "qwen2.5-coder:7b"))]);

    // When
    let loaded = load_agent_defs(dir.path());
    let resolved = resolve_agent_defs(dir.path());

    // Then
    assert_eq!(
        loaded.len(),
        resolved.len(),
        "resolution must not add a def the directory does not hold"
    );
    resolved.assert_names(&["explorer"]);
}

/// The replaced set is computed from the defs handed in and from nothing else — no name is
/// special-cased, so an agent called `fastcontext` replaces what its own def says and no more.
#[test]
fn computes_a_replaced_set_only_from_the_defs_it_is_given() {
    // Given
    let defs = vec![
        a_def("fastcontext", &[]),
        a_def("explorer", &["Grep"]),
        a_def("linter", &["ReadLints"]),
    ];

    // When
    let replaced = resolve_replaced_tools_for_defs(&defs);

    // Then
    assert_eq!(
        replaced,
        vec!["Grep".to_string(), "ReadLints".to_string()],
        "only the tools the defs themselves declare may be withdrawn"
    );
}

/// No defs means nothing is withdrawn — a session with an empty roster keeps its whole tool set.
#[test]
fn withdraws_nothing_when_no_agent_is_attached() {
    // Given
    let defs: Vec<SpecializedAgentDef> = Vec::new();

    // When
    let replaced = resolve_replaced_tools_for_defs(&defs);

    // Then
    assert!(
        replaced.is_empty(),
        "an empty roster must withdraw nothing, got {replaced:?}"
    );
}

// ---------------------------------------------------------------------------------------------
// AC45 — the name is gone from the source tree
// ---------------------------------------------------------------------------------------------

/// The workspace root, from this crate's own compile-time manifest directory.
fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("the crate must live two levels below the workspace root")
        .to_path_buf()
}

/// Every `.rs` file under `packages/*/src/` — production code only. Tests are excluded
/// deliberately: a test may legitimately write a def *named* `fastcontext` to prove it is treated
/// as an ordinary def, which is exactly what one of the tests above does.
fn production_sources() -> Vec<PathBuf> {
    let packages = workspace_root().join("packages");
    let mut sources = Vec::new();
    let entries = std::fs::read_dir(&packages).expect("packages/ must be readable");
    for entry in entries {
        let src = entry
            .expect("a packages/ entry must be readable")
            .path()
            .join("src");
        if src.is_dir() {
            collect_rust_files(&src, &mut sources);
        }
    }
    sources.sort();
    sources
}

fn collect_rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let entries = std::fs::read_dir(dir).expect("a source directory must be readable");
    for entry in entries {
        let path = entry.expect("a source entry must be readable").path();
        if path.is_dir() {
            collect_rust_files(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

/// A hardcoded agent is not removed by deleting one constructor — it comes back as a default model
/// id, a default endpoint, a `match` arm on a name, or a CLI flag named after it. The whole class
/// is asserted away at once, case-insensitively, so `FastContext`, `fast_context` and
/// `FASTCONTEXT` are all covered.
#[test]
fn no_production_source_names_the_agent_that_used_to_be_builtin() {
    // Given
    let sources = production_sources();
    assert!(
        !sources.is_empty(),
        "the source scan found no files, so it could not have failed"
    );

    // When
    let offenders: Vec<String> = sources
        .iter()
        .filter(|path| {
            let contents = std::fs::read_to_string(path).expect("a source file must be readable");
            contents.to_ascii_lowercase().contains("fastcontext")
                || contents.to_ascii_lowercase().contains("fast_context")
        })
        .map(|path| {
            path.strip_prefix(workspace_root())
                .unwrap_or(path)
                .display()
                .to_string()
        })
        .collect();

    // Then
    assert_eq!(
        offenders,
        Vec::<String>::new(),
        "no production source may name the agent that used to be builtin"
    );
}

/// The model id the builtin shipped is the other half of the same hardcoding: a def source that
/// still defaults to it has only moved the builtin one indirection out.
#[test]
fn no_production_source_carries_the_builtin_agents_model_id() {
    // Given
    let sources = production_sources();
    assert!(
        !sources.is_empty(),
        "the source scan found no files, so it could not have failed"
    );

    // When
    let offenders: Vec<String> = sources
        .iter()
        .filter(|path| {
            std::fs::read_to_string(path)
                .expect("a source file must be readable")
                .contains("FastContext-1.0-4B-RL")
        })
        .map(|path| {
            path.strip_prefix(workspace_root())
                .unwrap_or(path)
                .display()
                .to_string()
        })
        .collect();

    // Then
    assert_eq!(
        offenders,
        Vec::<String>::new(),
        "no production source may default to the model the builtin agent shipped with"
    );
}
