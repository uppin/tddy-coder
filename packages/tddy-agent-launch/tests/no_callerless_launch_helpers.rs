//! Source scan: the launch helpers `#agent-tools` left callerless are gone (`#agent-tools 24/28`).
//!
//! Changeset: `docs/dev/1-WIP/2026-11-06-dead-launch-code-removal.md`. Every agent now starts from
//! its YAML spec, so the bespoke jailed claude start's steps, the cursor notify env builders and the
//! sandbox-runner relaunch trio have no production caller. A deletion claim is only durable if it is
//! pinned, so this test reads the crate's sources and refuses any **declaration** of those names:
//! a `fn <name>` followed by `(` or `<`, or a `struct <Name>` followed by `<`, `{`, `(` or `;`.
//! Comments are skipped, so a doc line recalling a deleted helper is not a declaration.
//!
//! `relaunch_jail_dirs` and `relaunch_managed_workflow` are live (a jailed CLI agent's resume reads
//! them) and stay; the guard below proves the scan reads the files that hold them.

use std::path::{Path, PathBuf};

use pretty_assertions::assert_eq;

/// The callerless functions (changeset § Scope), by name.
const CALLERLESS_FUNCTIONS: &[&str] = &[
    "warm_up_jail_agents",
    "jail_semantic_index_env",
    "project_default_branch_ref",
    "create_jail_project_worktree",
    "link_jail_branch_to_stack_node",
    "create_routed_session_dir",
    "cursor_notify_env",
    "resumed_cursor_notify_env",
    "cursor_hook_daemon_url",
    "relaunch_jail_env",
    "spawn_relaunched_runner",
    "bridge_relaunched_jail",
];

/// The structs only those functions read.
const CALLERLESS_STRUCTS: &[&str] = &[
    "JailBranch",
    "RelaunchJailEnv",
    "RelaunchedRunnerSpawn",
    "RelaunchedJailBridge",
];

/// One production line that declares a callerless helper.
#[derive(Debug, PartialEq, Eq)]
struct Declaration {
    file: PathBuf,
    line: usize,
    name: &'static str,
}

fn the_crates_sources() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("src")
}

/// Every `.rs` file under `dir`, recursively, sorted so a failure lists them stably.
fn rust_sources_in(dir: &Path) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("{} is readable: {e}", dir.display()))
        .map(|entry| entry.expect("a readable directory entry").path())
        .flat_map(|path| {
            if path.is_dir() {
                rust_sources_in(&path)
            } else if path.extension().is_some_and(|ext| ext == "rs") {
                vec![path]
            } else {
                Vec::new()
            }
        })
        .collect();
    files.sort();
    files
}

/// Whether `line` declares `keyword name` followed by one of `openers`.
fn declares(line: &str, keyword: &str, name: &str, openers: &[char]) -> bool {
    let needle = format!("{keyword} {name}");
    line.match_indices(&needle).any(|(at, _)| {
        line[at + needle.len()..]
            .chars()
            .next()
            .is_some_and(|next| openers.contains(&next))
    })
}

/// Every declaration of a callerless helper in `text`, attributed to `file`.
fn callerless_declarations_in(file: &Path, text: &str) -> Vec<Declaration> {
    text.lines()
        .enumerate()
        .filter(|(_, line)| !line.trim_start().starts_with("//"))
        .flat_map(|(index, line)| {
            let functions = CALLERLESS_FUNCTIONS
                .iter()
                .filter(|name| declares(line, "fn", name, &['(', '<']));
            let structs = CALLERLESS_STRUCTS
                .iter()
                .filter(|name| declares(line, "struct", name, &['<', ' ', '{', '(', ';']));
            functions
                .chain(structs)
                .map(|name| Declaration {
                    file: file.to_path_buf(),
                    line: index + 1,
                    name,
                })
                .collect::<Vec<_>>()
        })
        .collect()
}

fn callerless_declarations_under(dir: &Path) -> Vec<Declaration> {
    rust_sources_in(dir)
        .into_iter()
        .flat_map(|file| {
            let text = std::fs::read_to_string(&file)
                .unwrap_or_else(|e| panic!("{} is readable: {e}", file.display()));
            callerless_declarations_in(&file, &text)
        })
        .collect()
}

#[test]
fn the_scan_finds_a_declaration_in_each_shape_and_skips_comments_calls_and_longer_names() {
    // Given — the shapes the helpers were declared in, beside lines that are not declarations
    let file = Path::new("helpers.rs");
    let text = concat!(
        "    pub(super) async fn warm_up_jail_agents(\n",
        "pub fn cursor_notify_env<'a>(config: &'a DaemonConfig) {}\n",
        "pub struct RelaunchJailEnv<'a> {\n",
        "// fn relaunch_jail_env( recalled in a comment\n",
        "    let env = self.relaunch_jail_env(launch)?;\n",
        "pub fn cursor_notify_env_pairs() {}\n",
        "pub struct JailBranchLink {}\n",
    );

    // When
    let found = callerless_declarations_in(file, text);

    // Then
    assert_eq!(
        found,
        vec![
            Declaration {
                file: file.to_path_buf(),
                line: 1,
                name: "warm_up_jail_agents",
            },
            Declaration {
                file: file.to_path_buf(),
                line: 2,
                name: "cursor_notify_env",
            },
            Declaration {
                file: file.to_path_buf(),
                line: 3,
                name: "RelaunchJailEnv",
            },
        ]
    );
}

#[test]
fn the_scan_reads_the_live_relaunch_steps_it_must_leave_in_place() {
    // Given
    let sources = rust_sources_in(&the_crates_sources());

    // When
    let relaunch_steps = sources
        .iter()
        .find(|file| file.ends_with("relaunch_jail_steps.rs"))
        .map(|file| std::fs::read_to_string(file).expect("relaunch_jail_steps.rs is readable"));

    // Then
    let relaunch_steps = relaunch_steps.expect("the scan must reach relaunch_jail_steps.rs");
    assert!(
        declares(&relaunch_steps, "fn", "relaunch_managed_workflow", &['(']),
        "a jailed CLI agent's resume still reads relaunch_managed_workflow"
    );
    assert!(
        sources
            .iter()
            .any(|file| file.ends_with("relaunch_jail_dirs.rs")),
        "relaunch_jail_dirs is live and must stay"
    );
}

#[test]
fn no_callerless_launch_helper_survives() {
    // Given
    let src = the_crates_sources();

    // When
    let survivors = callerless_declarations_under(&src);

    // Then
    assert_eq!(
        survivors,
        Vec::new(),
        "these launch helpers have no production caller and are deleted (#agent-tools 24/28)"
    );
}
