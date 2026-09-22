//! What `#carve` 9/9 delivers: the PR-stack data model in its own crate, and the recipe left behind.
//!
//! `pr_stack` and `orchestrate_pr_stack` are **79 of ~190** cross-crate references to this crate,
//! and neither is a recipe — twelve crates pull in every workflow recipe to reach a data model.
//! `pr_stack/mod.rs` has a clean internal line: `PrStackRecipe` and its two impls at 131–418, pure
//! stack operations from 418.
//!
//! The extraction is only valid if the moving set names **nothing** recipe-side, or it closes a
//! cycle. Two edges cross the seam, and both have a measured cut:
//!
//! - `reseed_stack_from_plan_if_unspawned` is the only operation after 418 reaching
//!   `plan_pr_stack`, which is itself mutually referenced with `pr_stack` and so cannot come. It
//!   **stays** — it is a plan→stack bridge, recipe-side by nature.
//! - `crate::writer::EXPLORATION_BASENAME` is two references to one `&str`, and `writer.rs` depends
//!   on `crate::parser`, so `writer` cannot move either.

use std::path::{Path, PathBuf};

fn package(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("the packages directory")
        .join(name)
}

fn production(text: &str) -> String {
    match text
        .lines()
        .position(|line| line.trim_start().starts_with("#[cfg(test)]"))
    {
        Some(at) => text.lines().take(at).collect::<Vec<_>>().join("\n"),
        None => text.to_string(),
    }
}

/// Whether `manifest` declares `name` as a dependency key — `name = …` or `name.workspace = …` —
/// rather than merely containing it: `tddy-git` is a prefix of `tddy-github`, and a comment naming a
/// crate declares nothing.
fn declares_dependency(manifest: &str, name: &str) -> bool {
    manifest.lines().any(|line| {
        line.trim_start()
            .strip_prefix(name)
            .map(str::trim_start)
            .is_some_and(|rest| rest.starts_with('=') || rest.starts_with('.'))
    })
}

/// Every `.rs` file under `dir`, at any depth — a module split into a directory is still source.
fn rust_sources(dir: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    for entry in std::fs::read_dir(dir)
        .expect("a readable source directory")
        .flatten()
    {
        let path = entry.path();
        if path.is_dir() {
            found.extend(rust_sources(&path));
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            found.push(path);
        }
    }
    found
}

/// AC1 — the new crate exists and depends on the three crates the moved code actually needs.
#[test]
fn the_stack_crate_depends_on_what_the_moved_code_names() {
    // Given the new crate's manifest
    let text =
        std::fs::read_to_string(package("tddy-pr-stack").join("Cargo.toml")).unwrap_or_default();
    assert!(
        !text.is_empty(),
        "`packages/tddy-pr-stack` has no manifest — the crate does not exist yet"
    );

    // Then it names the three it consumes
    for needed in ["tddy-core", "tddy-git", "tddy-github"] {
        assert!(
            declares_dependency(&text, needed),
            "`tddy-pr-stack` does not depend on `{needed}`, which the moved operations name"
        );
    }
}

/// AC1 — and not on the crate it left, which would close a cycle.
#[test]
fn the_stack_crate_does_not_depend_on_the_recipes_it_left() {
    // Given its manifest, which must exist for the claim to mean anything
    let text = std::fs::read_to_string(package("tddy-pr-stack").join("Cargo.toml"))
        .expect("`packages/tddy-pr-stack/Cargo.toml` — the crate does not exist yet");

    // Then the origin is not among its dependencies
    assert!(
        !declares_dependency(&text, "tddy-workflow-recipes"),
        "`tddy-pr-stack` depends on the crate it left, which is the cycle the seam exists to avoid"
    );
}

/// AC2 — nothing in the new crate names the recipe-side modules that could not come.
///
/// This is the criterion the whole seam was measured against.
#[test]
fn the_stack_crate_names_nothing_recipe_side() {
    // Given every source file of the new crate, at any depth
    let source = package("tddy-pr-stack").join("src");
    assert!(
        source.exists(),
        "`packages/tddy-pr-stack/src` does not exist yet"
    );

    // When each is searched for what had to stay behind
    let reaching: Vec<String> = rust_sources(&source)
        .into_iter()
        .filter(|path| {
            let text = production(&std::fs::read_to_string(path).unwrap_or_default());
            ["plan_pr_stack", "::writer", "::parser"]
                .iter()
                .any(|name| text.contains(name))
        })
        .map(|path| {
            path.strip_prefix(&source)
                .unwrap_or(&path)
                .display()
                .to_string()
        })
        .collect();

    // Then none of them does
    assert!(
        reaching.is_empty(),
        "these name a recipe-side module that could not move: {reaching:?}"
    );
}

/// AC4 — `PrStackRecipe` and its two impls stay where the recipes are.
///
/// The recipe is a recipe. Moving it would have made the new crate depend on the workflow machinery
/// it exists to be independent of.
#[test]
fn the_recipe_stays_with_the_recipes() {
    // Given the module the recipe lives in
    let text =
        std::fs::read_to_string(package("tddy-workflow-recipes").join("src/pr_stack/mod.rs"))
            .unwrap_or_default();

    // Then it still declares the recipe and both of its impls
    assert!(
        text.contains("pub struct PrStackRecipe"),
        "`PrStackRecipe` left `tddy-workflow-recipes`; only the data model was meant to"
    );
    assert!(
        text.contains("impl WorkflowRecipe for PrStackRecipe"),
        "the recipe impl left with it"
    );
}

/// AC4 — and so does the one operation that reaches into the recipe side.
#[test]
fn the_plan_to_stack_bridge_stays_behind() {
    // Given the recipe's own module
    let text =
        std::fs::read_to_string(package("tddy-workflow-recipes").join("src/pr_stack/mod.rs"))
            .unwrap_or_default();

    // Then the one operation that could not travel is still here
    assert!(
        text.contains("pub fn reseed_stack_from_plan_if_unspawned"),
        "`reseed_stack_from_plan_if_unspawned` moved, but it names `plan_pr_stack`, which is \
         mutually referenced with `pr_stack` and cannot come"
    );
}
