//! Resolving declared tools against a host: the `env` override, the `PATH` search, symlink
//! canonicalization, and the dep closure — on a fixture host, so nothing depends on this machine.
//!
//! PRD: `docs/ft/coder/1-WIP/PRD-2026-10-31-declared-tools.md` (`#agent-tools 1/12`).

mod support;

use std::path::PathBuf;

use pretty_assertions::assert_eq;
use support::{a_host, a_tool_yaml, a_tools_dir};
use tddy_actions::tool::{ResolvedTool, ToolError, ToolRegistry};

fn a_registry_with(tools: &support::AToolsDir) -> ToolRegistry {
    ToolRegistry::load(ToolRegistry::builtin().unwrap(), tools.path()).expect("the tools load")
}

#[test]
fn the_env_override_wins_over_the_path_search() {
    // Given — `rg` is on PATH, and TDDY_RG_BINARY names another one
    let registry = ToolRegistry::builtin().unwrap();
    let host = a_host()
        .with_path(&["/usr/bin"])
        .with_executable("/usr/bin/rg")
        .with_executable("/opt/rg-13/rg")
        .with_env("TDDY_RG_BINARY", "/opt/rg-13/rg");

    // When
    let resolved = registry.resolve_closure(&["rg"], &host).unwrap();

    // Then
    assert_eq!(resolved[0].binary, PathBuf::from("/opt/rg-13/rg"));
}

#[test]
fn a_path_hit_through_a_symlink_resolves_to_the_real_file() {
    // Given — Homebrew's `bin/rg` links into `Cellar`
    let registry = ToolRegistry::builtin().unwrap();
    let host = a_host()
        .with_path(&["/usr/bin", "/opt/homebrew/bin"])
        .with_link(
            "/opt/homebrew/bin/rg",
            "/opt/homebrew/Cellar/ripgrep/14.1.1/bin/rg",
        );

    // When
    let resolved = registry.resolve_closure(&["rg"], &host).unwrap();

    // Then
    assert_eq!(
        resolved,
        vec![ResolvedTool {
            id: "rg".into(),
            command: "rg".into(),
            binary: PathBuf::from("/opt/homebrew/Cellar/ripgrep/14.1.1/bin/rg"),
            reads: vec![],
        }]
    );
}

#[test]
fn the_first_path_directory_holding_the_command_wins() {
    // Given
    let registry = ToolRegistry::builtin().unwrap();
    let host = a_host()
        .with_path(&["/usr/local/bin", "/usr/bin"])
        .with_executable("/usr/local/bin/rg")
        .with_executable("/usr/bin/rg");

    // When
    let resolved = registry.resolve_closure(&["rg"], &host).unwrap();

    // Then
    assert_eq!(resolved[0].binary, PathBuf::from("/usr/local/bin/rg"));
}

#[test]
fn the_closure_includes_deps_transitively_once_each_deps_first() {
    // Given — app → (lib, fmt), lib → fmt
    let tools = a_tools_dir()
        .with_file("app.yaml", &a_tool_yaml("app", "app", &["lib", "fmt"]))
        .with_file("lib.yaml", &a_tool_yaml("lib", "lib", &["fmt"]))
        .with_file("fmt.yaml", &a_tool_yaml("fmt", "fmt", &[]));
    let host = a_host()
        .with_path(&["/bin"])
        .with_executable("/bin/app")
        .with_executable("/bin/lib")
        .with_executable("/bin/fmt");

    // When
    let resolved = a_registry_with(&tools)
        .resolve_closure(&["app"], &host)
        .unwrap();

    // Then
    assert_eq!(
        resolved.iter().map(|t| t.id.as_str()).collect::<Vec<_>>(),
        vec!["fmt", "lib", "app"]
    );
}

#[test]
fn an_unresolvable_binary_names_the_tool_and_everything_searched() {
    // Given — no `rg` anywhere, and the override variable unset
    let registry = ToolRegistry::builtin().unwrap();
    let host = a_host().with_path(&["/usr/bin", "/bin"]);

    // When
    let refused = registry.resolve_closure(&["rg"], &host).unwrap_err();

    // Then
    assert_eq!(
        refused,
        ToolError::Unresolvable {
            tool: "rg".into(),
            searched: "TDDY_RG_BINARY (unset), PATH (/usr/bin:/bin)".into(),
        }
    );
}

#[test]
fn an_env_override_naming_a_missing_file_is_unresolvable_not_a_path_search() {
    // Given — the operator pointed TDDY_RG_BINARY somewhere wrong; `rg` is still on PATH
    let registry = ToolRegistry::builtin().unwrap();
    let host = a_host()
        .with_path(&["/usr/bin"])
        .with_executable("/usr/bin/rg")
        .with_env("TDDY_RG_BINARY", "/nowhere/rg");

    // When
    let refused = registry.resolve_closure(&["rg"], &host).unwrap_err();

    // Then — an explicit override that fails is a refusal, never a quiet fall back to PATH
    assert_eq!(
        refused,
        ToolError::Unresolvable {
            tool: "rg".into(),
            searched: "TDDY_RG_BINARY (/nowhere/rg: not an executable file)".into(),
        }
    );
}

#[test]
fn resolving_an_unknown_id_is_refused() {
    // Given
    let registry = ToolRegistry::builtin().unwrap();

    // When
    let refused = registry.resolve_closure(&["jq"], &a_host()).unwrap_err();

    // Then
    assert_eq!(refused, ToolError::UnknownTool("jq".into()));
}
