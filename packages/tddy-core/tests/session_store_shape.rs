//! What `#carve` 7/11 delivers: the session storage layer in its own crates, and `sqlx` out of the
//! god-crate's dependency tree.
//!
//! **35 crates depend on `tddy-core`, and every one of them compiles `sqlx` with bundled SQLite.**
//! It is named by exactly four files, all under `session_catalog/`, and nothing else in `tddy-core`
//! touches a database.
//!
//! The group hinges on one DTO. `session_catalog` needs `session_actions`, which needs `output` and
//! `atomic_file`, which need `error` — and `error.rs` has exactly one edge out of the group:
//! `use crate::backend::ClarificationQuestion;`, for one `WorkflowError` variant. `#carve` 5/11 moves
//! that DTO, and the group becomes a closed DAG.
//!
//! These assertions read manifests, because "which crate owns this dependency" is not a question the
//! type system answers once everything compiles.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

fn package(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("the packages directory")
        .join(name)
}

/// A manifest, or empty when the crate does not exist — for asserting that it exists.
fn manifest(name: &str) -> String {
    std::fs::read_to_string(package(name).join("Cargo.toml")).unwrap_or_default()
}

/// A manifest that must exist — for asserting what it does or does not contain, which an empty
/// string would satisfy vacuously.
fn required_manifest(name: &str) -> String {
    std::fs::read_to_string(package(name).join("Cargo.toml"))
        .unwrap_or_else(|err| panic!("`packages/{name}/Cargo.toml` could not be read: {err}"))
}

/// The crate names a manifest declares as dependencies, under one rule:
///
/// - **Tables.** Only `[dependencies]`, `[dev-dependencies]`, `[build-dependencies]` and their
///   `[target.<cfg>.…]` forms count. A `[<kind>.<name>]` header (for example
///   `[dependencies.sqlx]`) declares `<name>`. Every other table is ignored.
/// - **Keys.** `name = …`, `name.workspace = true` and `name.path = …` all declare `name`: the key
///   is split on `.` and its first segment is the dependency.
/// - **Renames.** `alias = { package = "real-name", … }`, `alias.package = "real-name"` and a
///   `package = "real-name"` key inside a `[<kind>.alias]` table all declare `real-name`, which is
///   the crate actually depended on.
/// - **Comments** (`#` outside a string) are ignored, so a comment naming a crate neither adds nor
///   hides it. Continuation lines of a multi-line array or inline-table value are skipped, not
///   read as keys.
///
/// Names are returned sorted and de-duplicated, because the same crate may appear in several tables.
fn workspace_dependencies(manifest: &str) -> Vec<String> {
    let mut names = BTreeSet::new();
    let mut table = Table::Other;
    let mut open_brackets = 0i32;
    for raw in manifest.lines() {
        let line = strip_comment(raw).trim();
        if open_brackets > 0 {
            open_brackets += bracket_balance(line);
            continue;
        }
        if line.is_empty() {
            continue;
        }
        if line.starts_with('[') {
            if let Table::Single(name) = std::mem::replace(&mut table, Table::Other) {
                names.insert(name);
            }
            table = Table::from_header(line);
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let (key, value) = (key.trim(), value.trim());
        open_brackets = bracket_balance(value);
        match &mut table {
            Table::Other => {}
            Table::Single(name) => {
                if key == "package" {
                    *name = unquote(value);
                }
            }
            Table::Dependencies => {
                names.insert(declared_crate(key, value));
            }
        }
    }
    if let Table::Single(name) = table {
        names.insert(name);
    }
    names.into_iter().collect()
}

/// The kind of manifest table a line sits in, as far as [`workspace_dependencies`] cares.
enum Table {
    /// `[dependencies]` and its dev, build and target forms: each key is a dependency.
    Dependencies,
    /// `[dependencies.<name>]`: the table itself is one dependency, renamable by a `package` key.
    Single(String),
    /// Anything else.
    Other,
}

impl Table {
    fn from_header(header: &str) -> Self {
        let inner = header.trim_start_matches('[').trim_end_matches(']');
        let segments = dotted_segments(inner);
        let rest = match segments.first().map(String::as_str) {
            Some("target") if segments.len() >= 3 => &segments[2..],
            _ => &segments[..],
        };
        match rest {
            [kind] if is_dependency_table(kind) => Table::Dependencies,
            [kind, name] if is_dependency_table(kind) => Table::Single(name.clone()),
            _ => Table::Other,
        }
    }
}

fn is_dependency_table(kind: &str) -> bool {
    matches!(
        kind,
        "dependencies" | "dev-dependencies" | "build-dependencies"
    )
}

/// The crate a `key = value` line inside a dependency table declares.
fn declared_crate(key: &str, value: &str) -> String {
    let segments = dotted_segments(key);
    if segments.get(1).map(String::as_str) == Some("package") {
        return unquote(value);
    }
    inline_package(value).unwrap_or_else(|| segments[0].clone())
}

/// The `package = "…"` entry of an inline table value, if it has one.
fn inline_package(value: &str) -> Option<String> {
    let inner = value.strip_prefix('{')?.trim_end_matches('}');
    inner.split(',').find_map(|entry| {
        let (key, value) = entry.split_once('=')?;
        (key.trim() == "package").then(|| unquote(value.trim()))
    })
}

/// A dotted TOML key split into its segments, with quotes removed: `target.'cfg(unix)'.dependencies`
/// → `target`, `cfg(unix)`, `dependencies`. A `.` inside quotes does not split.
fn dotted_segments(key: &str) -> Vec<String> {
    let mut segments = vec![String::new()];
    let mut quote = None;
    for c in key.chars() {
        match (quote, c) {
            (None, '"' | '\'') => quote = Some(c),
            (Some(open), _) if c == open => quote = None,
            (None, '.') => segments.push(String::new()),
            (None, c) if c.is_whitespace() => {}
            _ => segments.last_mut().expect("at least one segment").push(c),
        }
    }
    segments
}

/// A line up to its first `#` outside a string.
fn strip_comment(line: &str) -> &str {
    let mut quote = None;
    for (at, c) in line.char_indices() {
        match (quote, c) {
            (None, '"' | '\'') => quote = Some(c),
            (Some(open), _) if c == open => quote = None,
            (None, '#') => return &line[..at],
            _ => {}
        }
    }
    line
}

/// How many more brackets (`[` or `{`) than closers a value opens, outside strings.
fn bracket_balance(value: &str) -> i32 {
    let mut quote = None;
    let mut balance = 0;
    for c in value.chars() {
        match (quote, c) {
            (None, '"' | '\'') => quote = Some(c),
            (Some(open), _) if c == open => quote = None,
            (None, '[' | '{') => balance += 1,
            (None, ']' | '}') => balance -= 1,
            _ => {}
        }
    }
    balance
}

fn unquote(value: &str) -> String {
    value
        .trim()
        .trim_matches(|c| c == '"' || c == '\'')
        .to_string()
}

/// A dependency that is one of this workspace's own crates.
fn is_workspace_crate(name: &str) -> bool {
    package(name).join("Cargo.toml").is_file()
}

/// The workspace crates `name` depends on, directly or through other workspace crates.
fn workspace_closure(name: &str) -> BTreeSet<String> {
    let mut reached = BTreeSet::new();
    let mut pending = vec![name.to_string()];
    while let Some(next) = pending.pop() {
        for dependency in workspace_dependencies(&required_manifest(&next)) {
            if is_workspace_crate(&dependency) && reached.insert(dependency.clone()) {
                pending.push(dependency);
            }
        }
    }
    reached
}

/// AC1 — `tddy-core` depends on neither `sqlx` nor SQLite.
///
/// This is the whole point of the node. The manifest alone does not prove the win — a path
/// dependency can re-introduce it, which is what AC2's `cargo tree` covers at `/green` — but a
/// manifest that still declares it proves the loss.
#[test]
fn the_god_crate_no_longer_declares_a_database() {
    // Given its manifest
    let text = required_manifest("tddy-core");

    // When its dependencies are listed
    let dependencies = workspace_dependencies(&text);

    // Then neither the driver nor the engine is among them
    assert!(
        !dependencies.iter().any(|name| name == "sqlx"),
        "`tddy-core` still declares `sqlx`, so all 35 of its dependents still compile SQLite"
    );
    assert!(
        !dependencies.iter().any(|name| name == "libsqlite3-sys"),
        "`tddy-core` declares `libsqlite3-sys` directly, which brings SQLite back without `sqlx`"
    );
}

/// AC1 — and it keeps `jsonschema`, which does **not** leave.
///
/// `session_actions/validate.rs` moves, but `session_action_pipeline.rs` stays and still names it.
/// Claiming otherwise would be wrong, and an earlier note in this stack did.
#[test]
fn the_god_crate_keeps_the_dependency_that_does_not_leave() {
    // Given its manifest
    let text = required_manifest("tddy-core");

    // When its dependencies are listed
    let dependencies = workspace_dependencies(&text);

    // Then `jsonschema` is still among them, because a module that stays still needs it
    assert!(
        dependencies.iter().any(|name| name == "jsonschema"),
        "`jsonschema` was removed, but `session_action_pipeline.rs` stays in `tddy-core` and names it"
    );
}

/// The only workspace crates `tddy-session-store` may depend on. None of them depends on
/// `tddy-core`, so none can close a cycle back into the crate the storage layer left.
const STORAGE_CRATE_WORKSPACE_DEPENDENCIES: [&str; 3] =
    ["tddy-workflow", "tddy-actions", "tddy-task"];

/// The allowlisted crates that reach `tddy-core`, directly or through another workspace crate.
fn allowlisted_crates_reaching_the_god_crate() -> Vec<&'static str> {
    STORAGE_CRATE_WORKSPACE_DEPENDENCIES
        .into_iter()
        .filter(|name| workspace_closure(name).contains("tddy-core"))
        .collect()
}

/// AC3 — `tddy-session-store` depends on the vocabulary and the action runtime, and nothing else of
/// ours.
///
/// `tddy-workflow` is where `#carve` 5/11 puts `ClarificationQuestion`, which `error.rs` names.
/// `session_actions/runtime.rs` runs every manifest on the action runtime (`tddy-actions`) and
/// tracks it in the task registry (`tddy-task`). Any other workspace dependency means the seam was
/// cut in the wrong place.
#[test]
fn the_storage_crate_depends_on_no_workspace_crate_outside_its_allowlist() {
    // Given the new crate's manifest
    let text = manifest("tddy-session-store");
    assert!(
        !text.is_empty(),
        "`packages/tddy-session-store` has no manifest — the crate does not exist yet"
    );

    // When its workspace dependencies are listed
    let unexpected: Vec<String> = workspace_dependencies(&text)
        .into_iter()
        .filter(|name| is_workspace_crate(name))
        .filter(|name| !STORAGE_CRATE_WORKSPACE_DEPENDENCIES.contains(&name.as_str()))
        .collect();

    // Then only the allowlisted crates are among them
    assert!(
        unexpected.is_empty(),
        "`tddy-session-store` depends on workspace crates outside \
         {STORAGE_CRATE_WORKSPACE_DEPENDENCIES:?}: {unexpected:?}"
    );

    // And none of the allowlisted crates reaches back into `tddy-core`
    let reaching = allowlisted_crates_reaching_the_god_crate();
    assert!(
        reaching.is_empty(),
        "these allowlisted crates depend on `tddy-core`, so the storage crate closes a cycle \
         through them: {reaching:?}"
    );
}

/// AC4 — the catalog crate takes `sqlx` with it, and depends back on nothing.
#[test]
fn the_catalog_crate_owns_the_database_and_depends_on_the_store() {
    // Given the new crate's manifest
    let text = manifest("tddy-session-catalog");
    assert!(
        !text.is_empty(),
        "`packages/tddy-session-catalog` has no manifest — the crate does not exist yet"
    );

    // When its dependencies are listed
    let dependencies = workspace_dependencies(&text);

    // Then it owns the database
    assert!(
        dependencies.iter().any(|name| name == "sqlx"),
        "`tddy-session-catalog` does not declare `sqlx` — the dependency went somewhere else"
    );

    // And it is built on the storage crate
    assert!(
        dependencies.iter().any(|name| name == "tddy-session-store"),
        "`tddy-session-catalog` does not depend on `tddy-session-store`, which it is built on"
    );

    // And it does not depend back on the crate it left
    assert!(
        !dependencies.iter().any(|name| name == "tddy-core"),
        "`tddy-session-catalog` depends on `tddy-core`, which is the edge this node removes"
    );
}
