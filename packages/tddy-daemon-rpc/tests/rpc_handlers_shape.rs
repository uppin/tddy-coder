//! The shape the RPC-handler move leaves behind.
//!
//! The families' suites prove behaviour, but they would pass just as well with every body still in
//! `tddy-session-lifecycle` behind a re-export. These assert what a reviewer checks instead: that
//! the implementations left, where the PR-stack trait now lives, that the edge between the two
//! crates points one way only, that each handler holds no more than it uses, and that no crate this
//! stack feeds grows past the size budget it is being carved to.
//!
//! Production lines are counted the way the `#carve` size target defines them: everything before
//! the first `#[cfg(test)]` that opens a `mod`, with `*_tests.rs`, `tests.rs` and `test_util.rs`
//! excluded. Cutting at the first bare `#[cfg(test)]` instead would stop at a test-only `use` near
//! the top of a file and under-measure it (see
//! `docs/dev/todo/2026-09-19-the-file-length-gate-stops-at-the-first-cfg-test-use.md`).

use std::path::{Path, PathBuf};

/// Where the lifecycle crate stood when this change was planned.
const LIFECYCLE_PRODUCTION_LINES_BEFORE: usize = 22_067;
/// What this change has to take out of it, at the least.
const LIFECYCLE_LINES_THIS_CHANGE_SHEDS: usize = 2_500;
/// The ceiling every crate this stack moves code into is held to.
const PRODUCTION_LINE_BUDGET: usize = 10_000;

// ---------------------------------------------------------------------------------------------
// Reading the tree
// ---------------------------------------------------------------------------------------------

fn package(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("the packages directory")
        .join(name)
}

fn source_of(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap_or_default()
}

fn rust_files_under(dir: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut pending = vec![dir.to_path_buf()];
    while let Some(next) = pending.pop() {
        for entry in std::fs::read_dir(&next).expect("a readable source directory") {
            let path = entry.expect("a readable directory entry").path();
            if path.is_dir() {
                pending.push(path);
            } else if path.extension().is_some_and(|ext| ext == "rs") {
                found.push(path);
            }
        }
    }
    found
}

fn is_test_file(path: &Path) -> bool {
    let stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or_default();
    stem.ends_with("_tests") || stem == "tests" || stem == "test_util"
}

/// The lines of one file before its test module opens.
fn production_lines_of(source: &str) -> usize {
    let lines: Vec<&str> = source.lines().collect();
    lines
        .windows(2)
        .position(|pair| {
            let opens_a_test_module = pair[1].trim_start().starts_with("mod ")
                || pair[1].trim_start().starts_with("pub mod ")
                || pair[1].trim_start().starts_with("pub(crate) mod ");
            pair[0].trim() == "#[cfg(test)]" && opens_a_test_module
        })
        .unwrap_or(lines.len())
}

/// A crate's production lines under `src/`.
fn production_lines_of_crate(name: &str) -> usize {
    rust_files_under(&package(name).join("src"))
        .into_iter()
        .filter(|path| !is_test_file(path))
        .map(|path| production_lines_of(&source_of(&path)))
        .sum()
}

/// The `[dependencies]` table of a crate's manifest, without its dev- or build-dependencies.
fn normal_dependencies_of(name: &str) -> String {
    let manifest = source_of(&package(name).join("Cargo.toml"));
    manifest
        .split("\n[")
        .find(|table| table.starts_with("dependencies]"))
        .unwrap_or_default()
        .to_string()
}

/// The body of `pub struct <name> { … }` in `source`, one line per field.
fn fields_of_struct(source: &str, name: &str) -> Vec<String> {
    let opening = format!("pub struct {name} {{");
    source
        .lines()
        .skip_while(|line| !line.contains(&opening))
        .skip(1)
        .take_while(|line| line.trim() != "}")
        .filter(|line| {
            let line = line.trim();
            !line.starts_with("//") && !line.starts_with('#') && line.contains(':')
        })
        .map(|line| line.trim().to_string())
        .collect()
}

/// Every `impl <Trait> for DaemonSessionHost` line in the lifecycle crate naming one of `traits`.
fn host_impls_of(traits: &[&str]) -> Vec<String> {
    rust_files_under(&package("tddy-session-lifecycle").join("src"))
        .into_iter()
        .flat_map(|path| {
            let file = path.display().to_string();
            source_of(&path)
                .lines()
                .filter(|line| line.trim_start().starts_with("impl"))
                .filter(|line| line.contains(" for DaemonSessionHost"))
                .filter(|line| traits.iter().any(|t| line.contains(&format!("{t} for"))))
                .map(|line| format!("{file}: {}", line.trim()))
                .collect::<Vec<_>>()
        })
        .collect()
}

// ---------------------------------------------------------------------------------------------
// AC3 — the families left the host
// ---------------------------------------------------------------------------------------------

#[test]
fn the_session_host_no_longer_implements_any_of_the_four_families() {
    // Given the five traits the four families are served through
    let family_traits = [
        "ProjectHandler",
        "ProjectService",
        "CatalogHandler",
        "ExecToolHandler",
        "PrStackHandler",
    ];

    // When the lifecycle crate is searched for their impls on the host
    let remaining = host_impls_of(&family_traits);

    // Then none is left — each family is its own handler in `tddy-daemon-rpc`
    assert!(
        remaining.is_empty(),
        "`DaemonSessionHost` still implements a family that moved to `tddy-daemon-rpc`:\n{}",
        remaining.join("\n")
    );
}

#[test]
fn the_family_implementation_files_have_left_the_lifecycle_crate() {
    // Given the files that held the four families' bodies
    let connection_service = package("tddy-session-lifecycle").join("src/connection_service");
    let moved = [
        "svc_project_ports.rs",
        "project_coordinate_handlers.rs",
        "svc_catalog_ports.rs",
        "svc_exec_tool_ports.rs",
        "svc_pr_stack_ports.rs",
        "svc_family_entries.rs",
    ];

    // When each is looked for where it used to be
    let still_there: Vec<&str> = moved
        .into_iter()
        .filter(|file| connection_service.join(file).exists())
        .collect();

    // Then every one has moved out
    assert!(
        still_there.is_empty(),
        "still in `tddy-session-lifecycle/src/connection_service/`: {still_there:?}"
    );
}

// ---------------------------------------------------------------------------------------------
// AC4, AC5 — each handler holds only what it uses, and never the host
// ---------------------------------------------------------------------------------------------

#[test]
fn no_handler_holds_the_session_host() {
    // Given the new crate's sources
    let holders: Vec<String> = rust_files_under(&package("tddy-daemon-rpc").join("src"))
        .into_iter()
        .filter(|path| {
            let source = source_of(path);
            source.contains("Arc<DaemonSessionHost>") || source.contains(": DaemonSessionHost,")
        })
        .map(|path| path.display().to_string())
        .collect();

    // Then none keeps the host as state — a handler holding it would be the old monolith again,
    // reached one hop further away
    assert!(holders.is_empty(), "a handler holds the host: {holders:?}");
}

#[test]
fn each_handler_holds_no_more_fields_than_its_budget() {
    // Given each handler's measured transitive field set
    let budgets = [
        ("project.rs", "ProjectRpcHandler", 6),
        ("catalog.rs", "CatalogRpcHandler", 5),
        ("exec_tool.rs", "ExecToolRpcHandler", 9),
        ("pr_stack.rs", "PrStackRpcHandler", 7),
    ];
    let src = package("tddy-daemon-rpc").join("src");

    // When each struct's fields are counted
    let over_budget: Vec<String> = budgets
        .into_iter()
        .map(|(file, name, budget)| {
            (
                name,
                budget,
                fields_of_struct(&source_of(&src.join(file)), name),
            )
        })
        .filter(|(_, budget, fields)| fields.is_empty() || fields.len() > *budget)
        .map(|(name, budget, fields)| format!("{name}: {} fields, budget {budget}", fields.len()))
        .collect();

    // Then none is missing and none has grown past what it uses
    assert!(over_budget.is_empty(), "{}", over_budget.join("\n"));
}

// ---------------------------------------------------------------------------------------------
// AC6, AC7 — where the PR-stack trait lives, and which way the edges point
// ---------------------------------------------------------------------------------------------

#[test]
fn the_pr_stack_handler_trait_is_defined_by_the_pr_stack_crate() {
    // Given both crates' sources
    let defines_it = |crate_name: &str| {
        rust_files_under(&package(crate_name).join("src"))
            .into_iter()
            .any(|path| source_of(&path).contains("pub trait PrStackHandler"))
    };

    // Then the trait is `tddy-pr-stack`'s, and the lifecycle crate keeps only a facade
    assert!(
        defines_it("tddy-pr-stack"),
        "`tddy-pr-stack` does not define `PrStackHandler`"
    );
    assert!(
        !defines_it("tddy-session-lifecycle"),
        "`tddy-session-lifecycle` still defines `PrStackHandler` rather than re-exporting it"
    );
}

#[test]
fn the_pr_stack_crate_serves_its_own_rpc_family() {
    // Given the PR-stack crate's normal dependencies
    let dependencies = normal_dependencies_of("tddy-pr-stack");

    // Then it names the transport and the proto types its service adapter is built from
    for needed in ["tddy-rpc", "tddy-service"] {
        assert!(
            dependencies.contains(needed),
            "`tddy-pr-stack` does not depend on `{needed}`, so it cannot hold `PrStackServiceImpl`"
        );
    }
}

#[test]
fn the_pr_stack_crate_depends_on_neither_the_lifecycle_crate_nor_the_recipes() {
    // Given the PR-stack crate's normal dependencies
    let dependencies = normal_dependencies_of("tddy-pr-stack");

    // Then neither crate it sits beneath is among them — either edge would close a cycle
    for forbidden in ["tddy-session-lifecycle", "tddy-workflow-recipes"] {
        assert!(
            !dependencies.contains(forbidden),
            "`tddy-pr-stack` depends on `{forbidden}`"
        );
    }
}

#[test]
fn the_lifecycle_crate_has_no_edge_to_the_crate_above_it() {
    // Given the lifecycle crate's whole manifest — dev-dependencies included, because a dev edge
    // would build a second copy of the lifecycle crate into its tests
    let manifest = source_of(&package("tddy-session-lifecycle").join("Cargo.toml"));

    // Then it never names `tddy-daemon-rpc`
    assert!(
        !manifest.contains("tddy-daemon-rpc"),
        "`tddy-session-lifecycle` depends on `tddy-daemon-rpc` — the edge must point one way only"
    );
}

// ---------------------------------------------------------------------------------------------
// AC8 — the binary serves the handlers
// ---------------------------------------------------------------------------------------------

#[test]
fn the_binary_runtime_serves_the_four_families_through_their_own_handlers() {
    // Given the bundle of services the binary mounts on its local socket
    let runtime = source_of(&package("tddy-daemon").join("src/runtime.rs"));
    let bundle: String = runtime
        .lines()
        .skip_while(|line| !line.contains("type BinaryLocalSocketServices"))
        .take_while(|line| line.trim() != ">;")
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        !bundle.is_empty(),
        "`runtime.rs` has no `BinaryLocalSocketServices`"
    );

    // Then each of the four families is served by its own handler type
    for handler in [
        "ProjectRpcHandler",
        "CatalogRpcHandler",
        "ExecToolRpcHandler",
        "PrStackRpcHandler",
    ] {
        assert!(
            bundle.contains(handler),
            "`BinaryLocalSocketServices` does not serve `{handler}`:\n{bundle}"
        );
    }
}

// ---------------------------------------------------------------------------------------------
// AC11, AC12 — the size budget
// ---------------------------------------------------------------------------------------------

#[test]
fn the_lifecycle_crate_sheds_at_least_2500_production_lines() {
    // Given the lifecycle crate's size when this change was planned
    let ceiling = LIFECYCLE_PRODUCTION_LINES_BEFORE - LIFECYCLE_LINES_THIS_CHANGE_SHEDS;

    // When it is measured now
    let measured = production_lines_of_crate("tddy-session-lifecycle");

    // Then the four families' bodies are out of it
    assert!(
        measured <= ceiling,
        "`tddy-session-lifecycle` has {measured} production lines; this change must bring it to \
         {ceiling} or fewer (from {LIFECYCLE_PRODUCTION_LINES_BEFORE})"
    );
}

/// A cap, not a specification of missing behaviour: it holds today because the receivers are
/// small, and it must still hold once the bodies have landed in them.
#[test]
fn every_crate_receiving_code_stays_within_10k_production_lines() {
    // Given every crate this change moves code into
    let receivers = ["tddy-daemon-rpc", "tddy-pr-stack", "tddy-daemon"];

    // When each is measured
    let over_budget: Vec<String> = receivers
        .into_iter()
        .map(|name| (name, production_lines_of_crate(name)))
        .filter(|(_, lines)| *lines > PRODUCTION_LINE_BUDGET)
        .map(|(name, lines)| format!("{name}: {lines} production lines"))
        .collect();

    // Then none has grown past the budget the stack is carving to
    assert!(
        over_budget.is_empty(),
        "over the {PRODUCTION_LINE_BUDGET}-line budget:\n{}",
        over_budget.join("\n")
    );
}
