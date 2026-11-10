//! Structure: the daemon's coding-agent launcher renders every route shape from the spec, so its
//! production code names neither OpenCode nor the variable OpenCode's spec declares — the route body
//! puts an `inline_config` route in the variable `route.config_env` names, and the launcher applies a
//! route's environment as it applies any (`#agent-tools 29/32`).
//!
//! Changeset: `docs/dev/changesets/2026-11-09-opencode-route-inline.md`.

use std::path::{Path, PathBuf};

/// The launcher's production sources: `coding_agent_launcher.rs` up to its test module, and every
/// module under `coding_agent_launcher/` that is not a test module.
fn the_launchers_production_sources() -> Vec<(PathBuf, String)> {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let root = src.join("coding_agent_launcher.rs");
    let mut sources = vec![(root.clone(), read(&root))];
    let dir = src.join("coding_agent_launcher");
    let mut modules: Vec<PathBuf> = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("{}: {e}", dir.display()))
        .map(|entry| entry.expect("a directory entry").path())
        .filter(|path| {
            path.extension().is_some_and(|ext| ext == "rs")
                && !path
                    .file_stem()
                    .is_some_and(|stem| stem.to_string_lossy().ends_with("_tests"))
        })
        .collect();
    modules.sort();
    sources.extend(modules.into_iter().map(|path| {
        let text = read(&path);
        (path, text)
    }));
    sources
}

/// `path`'s text before its first `#[cfg(test)]` (or `#[cfg(all(test, …))]`) item.
fn read(path: &Path) -> String {
    let text = std::fs::read_to_string(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let end = text.find("#[cfg(test)]").unwrap_or(text.len());
    text[..end].to_string()
}

/// Every line of `sources` that is code, not a comment, and names `word` (case-insensitive), as
/// `file:line: text`.
fn code_lines_naming(sources: &[(PathBuf, String)], word: &str) -> Vec<String> {
    let word = word.to_lowercase();
    sources
        .iter()
        .flat_map(|(path, text)| {
            let file = path.file_name().map(|n| n.to_string_lossy().into_owned());
            text.lines()
                .enumerate()
                .filter(|(_, line)| !line.trim_start().starts_with("//"))
                .filter(|(_, line)| line.to_lowercase().contains(&word))
                .map(move |(n, line)| {
                    format!(
                        "{}:{}: {}",
                        file.clone().unwrap_or_default(),
                        n + 1,
                        line.trim()
                    )
                })
                .collect::<Vec<_>>()
        })
        .collect()
}

#[test]
fn the_daemon_acp_launcher_names_no_agent_and_no_agent_variable() {
    // Given
    let sources = the_launchers_production_sources();
    assert!(
        sources.iter().any(|(path, _)| path.ends_with("acp.rs")),
        "the ACP planner is scanned"
    );

    // When
    let naming_opencode = code_lines_naming(&sources, "opencode");

    // Then — no branch, import or helper names the agent or its `OPENCODE_CONFIG_CONTENT`
    assert_eq!(naming_opencode, Vec::<String>::new());
}

/// The name of every coding agent this workspace ships a spec for: each spec's `id`, split at `-`
/// with the protocol suffix `acp` dropped (`codex-acp` → `codex`), and its `product` with the spaces
/// removed (`Claude Code` → `claudecode`), lower-cased.
fn every_shipped_agent_name() -> Vec<String> {
    let specs = Path::new(env!("CARGO_MANIFEST_DIR")).join("../tddy-coding-agent-specs/specs");
    let mut names: Vec<String> = std::fs::read_dir(&specs)
        .unwrap_or_else(|e| panic!("{}: {e}", specs.display()))
        .map(|entry| entry.expect("a directory entry").path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "yaml"))
        .flat_map(|path| {
            let text = std::fs::read_to_string(&path).unwrap_or_default();
            let value = |key: &str| {
                text.lines()
                    .find_map(|line| line.trim_start().strip_prefix(key))
                    .map(|rest| rest.trim().trim_matches('"').to_lowercase())
            };
            let ids = value("id:")
                .map(|id| {
                    id.split('-')
                        .filter(|part| *part != "acp")
                        .map(str::to_string)
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            ids.into_iter()
                .chain(value("product:").map(|product| product.replace(' ', "")))
        })
        .collect();
    names.sort();
    names.dedup();
    names
}

/// Every code line of `sources` with an identifier segment (split at anything not a letter or digit)
/// equal to one of `names`, as `file:line: text`.
fn code_lines_with_a_segment_in(sources: &[(PathBuf, String)], names: &[String]) -> Vec<String> {
    sources
        .iter()
        .flat_map(|(path, text)| {
            let file = path.file_name().map(|n| n.to_string_lossy().into_owned());
            text.lines()
                .enumerate()
                .filter(|(_, line)| !line.trim_start().starts_with("//"))
                .filter(|(_, line)| {
                    line.to_lowercase()
                        .split(|c: char| !c.is_ascii_alphanumeric())
                        .any(|segment| names.iter().any(|name| name == segment))
                })
                .map(move |(n, line)| {
                    format!(
                        "{}:{}: {}",
                        file.clone().unwrap_or_default(),
                        n + 1,
                        line.trim()
                    )
                })
                .collect::<Vec<_>>()
        })
        .collect()
}

/// `#agent-tools` 33/33 (the sweep, #857's `TODO(sweep)`): the launcher names no shipped agent at
/// all — not only OpenCode. The `toml_args` route renderer is keyed off `RouteShape::TomlArgs`, so it
/// is reached as `toml_route_overrides`, not by a Codex-named helper.
#[test]
fn the_daemon_acp_launcher_names_no_shipped_agent() {
    // Given every agent the specs ship, and the launcher's production code
    let names = every_shipped_agent_name();
    assert!(
        ["claude", "codex", "opencode"]
            .iter()
            .all(|name| names.iter().any(|n| n == name)),
        "the shipped specs are read: {names:?}"
    );
    let sources = the_launchers_production_sources();

    // When
    let naming_an_agent = code_lines_with_a_segment_in(&sources, &names);

    // Then
    assert_eq!(naming_an_agent, Vec::<String>::new());
}
