//! Results leave this library as values, not as lines on stdout.
//!
//! `backends::rust` already states the reason for progress: "anything that speaks a protocol on
//! stdout, a persistent server most obviously, would have its stream corrupted by an engine writing
//! progress into it." The same is true of results, and a server serving `Check` over its own
//! stdin/stdout is exactly that caller. These suites pin the return values, and the last one pins
//! the invariant itself — that the only module allowed to print is the command-line front end.

use std::path::Path;

use tddy_code_restructuring::apply::hash_file;
use tddy_code_restructuring::runner::{self, Command, Finding, Options, PlanProgress};
use tokio_util::sync::CancellationToken;

/// A git worktree holding one source file, at a path that is not the process directory.
fn a_workspace_holding(source: &str) -> tempfile::TempDir {
    let workspace = tempfile::tempdir().expect("a temporary workspace");
    std::process::Command::new("git")
        .args(["init", "--quiet"])
        .current_dir(workspace.path())
        .status()
        .expect("git init");
    std::fs::create_dir_all(workspace.path().join("src")).expect("src");
    std::fs::write(workspace.path().join("src/lib.rs"), source).expect("a source file");
    workspace
}

/// A plan under `root` whose snapshot names `src/lib.rs`, carrying `ops` verbatim.
fn a_plan_under(root: &Path, ops: &[&str]) -> std::path::PathBuf {
    let digest = hash_file(&root.join("src/lib.rs")).expect("hash the source file");
    let plan = root.join("plan.jsonl");
    let mut lines = vec![format!(
        "{{\"v\":1,\"snapshot\":{{\"src/lib.rs\":\"{digest}\"}}}}"
    )];
    lines.extend(ops.iter().map(|op| (*op).to_string()));
    std::fs::write(&plan, format!("{}\n", lines.join("\n"))).expect("write the plan");
    plan
}

fn a_check_of(plan: &Path) -> Options {
    Options {
        command: Command::Check,
        target: Some(plan.to_path_buf()),
        ..Options::default()
    }
}

/// An extraction into a module name the file already binds.
///
/// A **range** anchor, because the static tier judges only those: `RustBackend::check` returns
/// nothing for a symbol anchor, since both of its rules need the seam's extent. A name collision is
/// a lexical fact, so this is a finding no language server has to be consulted about — which is the
/// whole point of having a static tier.
const AN_EXTRACTION_INTO_A_NAME_ALREADY_TAKEN: &str = r#"{"op":"extract_module","anchor":{"kind":"range","file":"src/lib.rs","start":{"line":2,"col":1},"end":{"line":4,"col":2}},"name":"grouped","reexport":"glob"}"#;

/// A source file that already declares `grouped`, so an extraction claiming that name collides.
const A_SOURCE_THAT_ALREADY_BINDS_GROUPED: &str = "mod grouped;\npub fn foo() -> u32 {\n    1\n}\n";

#[test]
fn a_check_returns_the_findings_it_made() {
    // Given a plan whose one operation names a symbol the source does not declare
    let workspace = a_workspace_holding(A_SOURCE_THAT_ALREADY_BINDS_GROUPED);
    let plan = a_plan_under(workspace.path(), &[AN_EXTRACTION_INTO_A_NAME_ALREADY_TAKEN]);

    // When it is checked
    let findings = runner::check(
        workspace.path(),
        a_check_of(&plan),
        None,
        CancellationToken::new(),
    )
    .expect("a check with findings is not itself a failure");

    // Then the findings come back as values, attributed to the operation that caused them
    assert_eq!(findings.len(), 1, "expected one finding, got {findings:?}");
    assert_eq!(findings[0].operation, 0);
    assert!(
        !findings[0].detail.is_empty(),
        "a finding must say what is wrong"
    );
}

/// An extract-method over lines 3–5 of [`A_FUNCTION_THAT_RETURNS_EARLY`], which hold its `return`.
const AN_EXTRACTION_OVER_AN_EARLY_RETURN: &str = r#"{"op":"extract_method","anchor":{"kind":"range","file":"src/lib.rs","start":{"line":3,"col":5},"end":{"line":5,"col":6}},"name":"base_or_early"}"#;

/// A function whose body exits early from inside the statements an extraction would take.
const A_FUNCTION_THAT_RETURNS_EARLY: &str = "pub fn level(x: bool) -> Result<u32, String> {\n    let base = 2;\n    if x {\n        return Ok(1);\n    }\n    Ok(base)\n}\n";

#[test]
fn a_check_finds_an_extraction_that_would_carry_an_early_return() {
    // Given a plan extracting statements that return early from the function around them
    let workspace = a_workspace_holding(A_FUNCTION_THAT_RETURNS_EARLY);
    let plan = a_plan_under(workspace.path(), &[AN_EXTRACTION_OVER_AN_EARLY_RETURN]);

    // When it is checked, without a language server
    let findings = runner::check(
        workspace.path(),
        a_check_of(&plan),
        None,
        CancellationToken::new(),
    )
    .expect("a check with findings is not itself a failure");

    // Then the early return is the one finding, naming the line that holds it
    assert_eq!(findings.len(), 1, "expected one finding, got {findings:?}");
    assert_eq!(findings[0].operation, 0);
    assert!(
        findings[0].detail.starts_with(
            "this seam cannot be cut here: the range returns early from the function around it, \
             on line 4 (`return Ok(1);`)."
        ),
        "{}",
        findings[0].detail
    );
}

#[test]
fn a_check_of_a_sound_plan_returns_no_findings() {
    // Given a plan with nothing wrong in it
    let workspace = a_workspace_holding("pub fn foo() -> u32 {\n    1\n}\n");
    let plan = a_plan_under(workspace.path(), &[]);

    // When it is checked
    let findings = runner::check(
        workspace.path(),
        a_check_of(&plan),
        None,
        CancellationToken::new(),
    )
    .expect("a sound plan checks clean");

    // Then the report is empty rather than absent
    assert_eq!(findings, Vec::<Finding>::new());
}

#[test]
fn a_status_returns_the_plan_progress_it_read() {
    // Given a plan of two operations whose journal holds nothing yet
    let workspace = a_workspace_holding(A_SOURCE_THAT_ALREADY_BINDS_GROUPED);
    let plan = a_plan_under(
        workspace.path(),
        &[
            AN_EXTRACTION_INTO_A_NAME_ALREADY_TAKEN,
            AN_EXTRACTION_INTO_A_NAME_ALREADY_TAKEN,
        ],
    );

    // When its status is asked for
    let progress = runner::status(
        workspace.path(),
        Options {
            command: Command::Status,
            target: Some(plan),
            ..Options::default()
        },
    )
    .expect("a status of an unstarted plan");

    // Then the counts come back as values
    assert_eq!(
        progress,
        PlanProgress {
            completed: 0,
            in_flight: 0,
            pending: 2,
            failed: 0,
        }
    );
}

#[test]
fn a_verify_returns_the_comparison_it_made() {
    // Given a committed tree that has not changed since
    let workspace = a_workspace_holding("pub fn foo() -> u32 {\n    1\n}\n");
    for args in [
        vec!["add", "-A"],
        vec![
            "-c",
            "user.email=t@t",
            "-c",
            "user.name=t",
            "commit",
            "-qm",
            "base",
        ],
    ] {
        std::process::Command::new("git")
            .args(&args)
            .current_dir(workspace.path())
            .status()
            .expect("git");
    }

    // When it is verified against that commit
    let comparison = runner::verify(
        workspace.path(),
        Options {
            command: Command::Verify,
            against: Some("HEAD".to_string()),
            ..Options::default()
        },
    )
    .expect("a verify against HEAD");

    // Then the comparison itself comes back, not just the fact that it held
    assert!(comparison.holds());
    assert_eq!(comparison.missing, Vec::<String>::new());
    assert_eq!(comparison.added, Vec::<String>::new());
    assert_eq!(comparison.before, comparison.after);
}

#[test]
fn only_the_command_line_front_end_writes_to_standard_output() {
    // Given this crate's own sources
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");

    // When every module is read
    let printing: Vec<String> = modules_under(&src)
        .into_iter()
        .filter(|module| {
            std::fs::read_to_string(module)
                .expect("a source file")
                .lines()
                .any(|line| {
                    let code = line.trim();
                    !code.starts_with("//")
                        && (code.contains("println!") || code.contains("print!"))
                })
        })
        .map(|module| {
            module
                .strip_prefix(&src)
                .expect("a path under src")
                .to_string_lossy()
                .to_string()
        })
        .collect();

    // Then only the front end that owns a console does so — every other module hands its results
    // back, so a caller speaking a protocol on stdout keeps its stream
    assert_eq!(printing, vec!["restructure_cli.rs".to_string()]);
}

/// Every `.rs` file under `dir`, sorted, so the assertion above is deterministic.
fn modules_under(dir: &Path) -> Vec<std::path::PathBuf> {
    let mut found = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(next) = stack.pop() {
        for entry in std::fs::read_dir(&next).expect("a readable directory") {
            let path = entry.expect("a directory entry").path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|ext| ext == "rs") {
                found.push(path);
            }
        }
    }
    found.sort();
    found
}
