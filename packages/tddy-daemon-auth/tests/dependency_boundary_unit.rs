//! The check that proves the extraction is real rather than a re-export.
//!
//! `tddy-daemon-auth` exists to be the daemon's identity boundary. If `tddy-daemon` were still on
//! its dependency path, every symbol it "left behind" would still be compiled into it, the crate
//! split would be a directory rename, and the module cycle the split exists to cut would be intact
//! — while the changeset read as done. Cargo would not complain: a crate may depend on a crate
//! that depends on nothing of it.
//!
//! Asserted against the manifests rather than described in prose, and *transitively*, because a
//! direct edge is the one shape a reviewer would notice anyway.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

/// The crate no crate in this graph may reach.
const THE_GOD_CRATE: &str = "tddy-daemon";

/// This crate, as its directory is named.
const THIS_CRATE: &str = "tddy-daemon-auth";

#[test]
fn does_not_declare_the_daemon_in_any_section_of_its_own_manifest() {
    // Given this crate's manifest, dev-dependencies and target-specific sections included
    let manifest = manifest_of(THIS_CRATE);

    // When every dependency name it declares anywhere is read
    let declared: BTreeSet<String> = dependency_names(&manifest).collect();

    // Then the daemon is not among them
    assert!(
        !declared.contains(THE_GOD_CRATE),
        "{THIS_CRATE} must not depend on {THE_GOD_CRATE} — a crate that does has not left it. \
         It declares: {declared:?}"
    );
}

#[test]
fn has_no_path_from_itself_to_the_daemon_through_any_workspace_crate() {
    // Given the whole build-time dependency closure of this crate
    let reachable = crates_reachable_from(THIS_CRATE);

    // When the daemon is looked for anywhere in it
    let reaches_the_daemon = reachable.contains(THE_GOD_CRATE);

    // Then it is absent — no intermediate crate re-introduces the edge
    assert!(
        !reaches_the_daemon,
        "{THE_GOD_CRATE} is on {THIS_CRATE}'s dependency path; it is reachable through \
         {reachable:?}"
    );
}

#[test]
fn reaches_the_kernel_it_takes_its_session_resolver_from() {
    // Given the same closure
    let reachable = crates_reachable_from(THIS_CRATE);

    // When the crate publishing `SessionUserResolver` is looked for
    // Then it is there — otherwise the walk above proves nothing, because it walked nothing
    assert!(
        reachable.contains("tddy-daemon-kernel"),
        "{THIS_CRATE} resolves a session token's login through tddy-daemon-kernel, so the \
         dependency walk must find it; it found {reachable:?}"
    );
}

/// The `packages/` directory every workspace crate lives under.
fn packages_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("the crate directory has a parent")
        .to_path_buf()
}

fn manifest_of(crate_dir: &str) -> String {
    let path = packages_dir().join(crate_dir).join("Cargo.toml");
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{} is readable: {e}", path.display()))
}

/// Every workspace crate reachable from `root` through the dependencies cargo actually builds.
///
/// `dev-dependencies` are followed for `root` itself — a test binary links them, so a dev edge
/// back to the daemon would put it in this crate's build after all — and not for the crates below
/// it, which is cargo's own rule: a dependency's dev-dependencies are never built.
fn crates_reachable_from(root: &str) -> BTreeSet<String> {
    let mut reached = BTreeSet::new();
    let mut pending = vec![(root.to_string(), true)];

    while let Some((crate_dir, with_dev)) = pending.pop() {
        for dependency in workspace_dependencies(&manifest_of(&crate_dir), with_dev) {
            if reached.insert(dependency.clone()) {
                pending.push((dependency, false));
            }
        }
    }
    reached
}

/// The workspace crates one manifest declares — the `name = { path = "../name" }` entries.
///
/// A registry dependency names no directory here, so it cannot lead back to a workspace crate and
/// is not followed.
fn workspace_dependencies(manifest: &str, with_dev: bool) -> Vec<String> {
    let mut found = Vec::new();
    let mut in_a_followed_section = false;

    for line in manifest.lines() {
        let line = line.trim();
        if let Some(section) = line.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
            in_a_followed_section = is_followed(section, with_dev);
            continue;
        }
        if in_a_followed_section && line.contains("path = \"..") {
            if let Some(name) = line.split('=').next() {
                found.push(name.trim().to_string());
            }
        }
    }
    found
}

/// Whether a manifest section declares dependencies that end up in the build.
fn is_followed(section: &str, with_dev: bool) -> bool {
    let section = section.rsplit('.').next().unwrap_or(section);
    match section {
        "dependencies" | "build-dependencies" => true,
        "dev-dependencies" => with_dev,
        _ => false,
    }
}

/// Every dependency name a manifest declares, in every section including `dev-dependencies`.
fn dependency_names(manifest: &str) -> impl Iterator<Item = String> + '_ {
    let mut in_a_dependency_section = false;
    manifest.lines().filter_map(move |line| {
        let line = line.trim();
        if let Some(section) = line.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
            in_a_dependency_section = section
                .rsplit('.')
                .next()
                .unwrap_or(section)
                .ends_with("dependencies");
            return None;
        }
        if !in_a_dependency_section || !line.contains('=') || line.starts_with('#') {
            return None;
        }
        line.split('=').next().map(|name| name.trim().to_string())
    })
}
