//! Transactional groups: consecutive operations sharing a `"group"` id apply as one unit.
//!
//! Some refactors cannot compile step by step — change a type, then adapt every use. A group is
//! judged at its own end: `cargo check --all-targets` over the packages its members touched, and a
//! group that does not compile there is rolled back exactly — contents restored from the pre-images
//! its members journalled, created files removed, renames undone — while everything before it stays
//! applied. Ungrouped operations keep the end-of-run gate and its leave-on-disk contract.
//!
//! **What the fixtures can and cannot show yet.** No operation this engine has today produces a
//! tree that compiles only once a second operation lands: every one of them compiles alone, and a
//! signature change that breaks its callers is `signature-rewrites`' to add. So the group that
//! fails here fails because one member, `move_test_binary_to_crate`, leaves a test binary behind a
//! file it reads with `include_str!` — a tree no later member can repair — and the group that
//! passes is two renames that compile either way, so no test here proves a tree that breaks between
//! members and compiles at the end. What they pin is the unit: the gate at the
//! group's end, the journal's group records, and the exact rollback.
//!
//! Against a live rust-analyzer, one server at a time (the harness enforces it).

mod harness;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use harness::{
    a_rename_in, a_workspace_holding_files, an_extract_module_of, applying_a_plan_of,
    applying_the_plan_with, checking_the_plan, AFixtureWorkspace, ORIGIN_LIB,
    THE_MOVED_TEST_BINARY, THE_TEST_BINARY,
};
use tddy_code_restructuring::console;
use tddy_code_restructuring::journal::{Journal, JournalRecord, OpStatus, PreImage};
use tddy_code_restructuring::runner::{self, RunSummary, StatePaths};
use tddy_code_restructuring::spawn_record::SpawnRecorder;
use tddy_code_restructuring::{
    Anchor, FileEdit, LedgerCheckpoint, OpId, Plan, Position, PositionLedger, Range, RefactorKind,
    RefactorOp, Resolution, TextEdit, WorkspaceEdit,
};

/// `origin`'s library as every test here starts from it: two functions nothing calls.
const TWO_FUNCTIONS: &str =
    "pub fn level() -> u32 {\n    2\n}\n\npub fn depth() -> u32 {\n    3\n}\n";

/// Lines 5–7 of [`TWO_FUNCTIONS`]: `depth`, whole.
const THE_DEPTH_FUNCTION: std::ops::RangeInclusive<u32> = 5..=7;

/// Two crates: `origin`, whose library holds [`TWO_FUNCTIONS`] and whose test binary reads its
/// expected output from a file beside it, and `destination`, where that binary can be moved.
///
/// Moving the binary leaves `golden/expected.txt` behind, so the moved binary no longer compiles —
/// the member that makes a group fail its end gate.
fn two_crates_and_a_test_binary_that_reads_a_file_beside_it() -> AFixtureWorkspace {
    a_workspace_holding_files(&[
        (
            "Cargo.toml",
            "[workspace]\nresolver = \"2\"\nmembers = [\"crates/origin\", \"crates/destination\"]\n",
        ),
        (
            "crates/origin/Cargo.toml",
            "[package]\nname = \"origin\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        ),
        (ORIGIN_LIB, TWO_FUNCTIONS),
        (
            THE_TEST_BINARY,
            "//! Reads its expected output from a file beside it.\n\n#[test]\n\
             fn matches_the_golden_output() {\n    \
             assert_eq!(include_str!(\"golden/expected.txt\"), \"2\\n\");\n}\n",
        ),
        ("crates/origin/tests/golden/expected.txt", "2\n"),
        (
            "crates/destination/Cargo.toml",
            "[package]\nname = \"destination\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        ),
        ("crates/destination/src/lib.rs", "//! Where the test goes.\n"),
    ])
}

/// Renaming `symbol` in `origin`'s library, as a member of `group` when it names one.
fn renaming(symbol: &str, to: &str, group: Option<&str>) -> RefactorOp {
    RefactorOp {
        group: group.map(str::to_string),
        ..a_rename_in(ORIGIN_LIB, symbol, to)
    }
}

/// Moving [`THE_TEST_BINARY`] to `destination`, as a member of `group` when it names one.
fn moving_the_test_binary(group: Option<&str>) -> RefactorOp {
    RefactorOp {
        id: None,
        op: RefactorKind::MoveTestBinaryToCrate,
        anchor: Anchor::Symbol {
            file: THE_TEST_BINARY.to_string(),
            path: "golden".to_string(),
        },
        name: None,
        to: Some("crates/destination".to_string()),
        variant: None,
        with_private_deps: false,
        reexport: None,
        to_file: false,
        also: Vec::new(),
        group: group.map(str::to_string),
        type_: None,
        expr: None,
        order: Vec::new(),
        canonical_paths: false,
    }
}

/// Grouping `depth` into a module of its own, in a file of its own — an operation that creates a
/// file — as a member of `group`.
fn extracting_depth_to_a_file(fixture: &AFixtureWorkspace, group: &str) -> RefactorOp {
    RefactorOp {
        to_file: true,
        group: Some(group.to_string()),
        ..an_extract_module_of(fixture, ORIGIN_LIB, THE_DEPTH_FUNCTION, "depths")
    }
}

/// `op` with a stable id, for a plan whose journal a test writes itself.
fn with_id(op: RefactorOp, id: &str) -> RefactorOp {
    RefactorOp {
        id: Some(OpId(id.to_string())),
        ..op
    }
}

/// Every source file of the workspace with its exact text — what "byte for byte" is measured on.
///
/// The workspace manifest and everything under `crates/`, so a file a group created shows up as
/// one more entry and a file it renamed as one entry moved.
fn the_tree(fixture: &AFixtureWorkspace) -> BTreeMap<String, String> {
    let mut tree = BTreeMap::new();
    tree.insert("Cargo.toml".to_string(), fixture.read("Cargo.toml"));
    collect(fixture.path(), &fixture.path().join("crates"), &mut tree);
    tree
}

fn collect(root: &Path, directory: &Path, tree: &mut BTreeMap<String, String>) {
    let mut entries: Vec<PathBuf> = std::fs::read_dir(directory)
        .expect("the directory reads")
        .map(|entry| entry.expect("an entry").path())
        .collect();
    entries.sort();
    for path in entries {
        if path.is_dir() {
            collect(root, &path, tree);
            continue;
        }
        let relative = path
            .strip_prefix(root)
            .expect("under the root")
            .to_string_lossy()
            .to_string();
        let text = std::fs::read_to_string(&path).expect("a source file reads");
        tree.insert(relative, text);
    }
}

/// Where the run of `plan` keeps its journal.
fn the_journal_of(fixture: &AFixtureWorkspace, plan: &Path) -> PathBuf {
    tddy_code_restructuring::state_directory_for_plan(fixture.path(), plan)
        .expect("the plan has a state directory")
        .join("journal.jsonl")
}

/// The group records in the journal of `plan`'s run, in the order they were written.
fn the_group_records_of(fixture: &AFixtureWorkspace, plan: &Path) -> Vec<(OpStatus, String)> {
    Journal::load(&the_journal_of(fixture, plan))
        .expect("the journal loads")
        .records
        .into_iter()
        .filter(|record| {
            matches!(
                record.status,
                OpStatus::GroupStarted | OpStatus::GroupCompleted | OpStatus::GroupRolledBack
            )
        })
        .map(|record| (record.status, record.group.unwrap_or_default()))
        .collect()
}

/// The edit renaming `level` to `tier` on line 1 of [`TWO_FUNCTIONS`], written out — what a crash
/// inside a group had already committed.
fn the_rename_of_level_to_tier() -> WorkspaceEdit {
    WorkspaceEdit {
        changes: vec![FileEdit::Change {
            path: ORIGIN_LIB.to_string(),
            edits: vec![TextEdit {
                range: Range {
                    start: Position { line: 1, col: 8 },
                    end: Position { line: 1, col: 13 },
                },
                new_text: "tier".to_string(),
            }],
        }],
    }
}

/// The journal a run of `plan` leaves when it crashes inside the group `renames` after committing
/// its first member: `group_started`, that member's pre-image, its write-ahead pair and the
/// plan-sync record a committed operation is followed by — and nothing that closes the group.
fn a_crash_after_the_first_member_of(fixture: &AFixtureWorkspace, plan: &Path) {
    let root = fixture.path();
    let journal_path = the_journal_of(fixture, plan);
    let paths = StatePaths::for_plan(root, plan).expect("the plan has state paths");
    let first = OpId("op-1".to_string());
    let mut journal = Journal::default();
    journal
        .append(
            &journal_path,
            JournalRecord::group_started(0, "renames".to_string(), vec![0, 1]),
        )
        .expect("group_started is journalled");
    journal
        .append(
            &journal_path,
            JournalRecord::pre_imaged(
                0,
                Some(first.clone()),
                "renames".to_string(),
                vec![PreImage {
                    path: ORIGIN_LIB.to_string(),
                    contents: Some(fixture.read(ORIGIN_LIB)),
                }],
            ),
        )
        .expect("the pre-image is journalled");
    runner::commit_operation(
        0,
        Some(&first),
        &Resolution::of(the_rename_of_level_to_tier()),
        root,
        &paths,
        &mut journal,
        &mut PositionLedger::new(),
        &SpawnRecorder::discard(),
    )
    .expect("the first member commits");
    let written = Plan::parse(&std::fs::read_to_string(plan).expect("the plan reads"))
        .expect("the plan parses");
    journal
        .append(
            &journal_path,
            JournalRecord::plan_synced(
                0,
                Some(first),
                tddy_code_restructuring::plan_store::pending_digest(&written, 0),
            ),
        )
        .expect("the plan sync is journalled");
}

/// The one refusal every group that fails its end gate gives: it names the group and says it was
/// rolled back. `group` is the group the plan named.
fn assert_rolled_back_for_not_compiling(refusal: &str, group: &str) {
    let expected = format!("group `{group}` does not compile at its end, so it was rolled back:");
    assert!(
        refusal.starts_with(&expected),
        "the refusal does not name the group that was rolled back:\n{refusal}"
    );
}

/// The checkpoint the run of `plan` left beside its journal, if it left one.
fn the_ledger_checkpoint_of(fixture: &AFixtureWorkspace, plan: &Path) -> Option<LedgerCheckpoint> {
    let beside_the_journal = the_journal_of(fixture, plan).with_file_name("ledger.json");
    LedgerCheckpoint::load(&beside_the_journal).expect("the checkpoint reads")
}

/// Every pre-image the journal of `plan`'s run holds for `group`, in the order written.
fn the_pre_images_of(fixture: &AFixtureWorkspace, plan: &Path, group: &str) -> Vec<PreImage> {
    Journal::load(&the_journal_of(fixture, plan))
        .expect("the journal loads")
        .group_pre_images(group)
}

/// What a group that started and was rolled back leaves in the journal.
fn started_and_rolled_back(group: &str) -> [(OpStatus, String); 2] {
    [
        (OpStatus::GroupStarted, group.to_string()),
        (OpStatus::GroupRolledBack, group.to_string()),
    ]
}

/// Apply the plan at `plan`, collecting every line of the run's account as it is reported.
async fn applying_and_collecting_the_account(
    fixture: &AFixtureWorkspace,
    plan: PathBuf,
) -> (Result<RunSummary, String>, Vec<String>) {
    let lines = Arc::new(Mutex::new(Vec::<String>::new()));
    let sink = Arc::clone(&lines);
    let summary = applying_the_plan_with(fixture, plan, move |options| {
        options.account = Arc::new(move |line: &str| {
            sink.lock()
                .expect("the account lock")
                .push(line.to_string());
        });
    })
    .await;
    let account = lines.lock().expect("the account lock").clone();
    (summary, account)
}

/// The account lines that state an operation as applied, or the group it belongs to.
fn the_applied_and_group_lines(account: &[String]) -> Vec<&str> {
    account
        .iter()
        .map(String::as_str)
        .filter(|line| line.ends_with(" applied") || line.starts_with("   group: "))
        .collect()
}

/// Two renames applied as one group, gated once at the group's end, and journalled as a group that
/// started and completed. It does not show a tree that compiles only once both have landed — see
/// the module doc for why no pair of operations here breaks the tree between them.
#[tokio::test(flavor = "multi_thread")]
async fn two_renames_in_a_group_apply_and_journal_the_group_as_started_then_completed() {
    // Given
    let workspace = two_crates_and_a_test_binary_that_reads_a_file_beside_it();
    let plan = workspace.a_plan_of(&[
        renaming("level", "tier", Some("renames")),
        renaming("depth", "height", Some("renames")),
    ]);

    // When
    let summary = applying_the_plan_with(&workspace, plan.clone(), |_| {}).await;

    // Then
    assert_eq!(
        summary,
        Ok(RunSummary {
            applied: 2,
            total: 2,
            stopped_early: false,
        })
    );
    assert_eq!(
        the_group_records_of(&workspace, &plan),
        [
            (OpStatus::GroupStarted, "renames".to_string()),
            (OpStatus::GroupCompleted, "renames".to_string()),
        ]
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_group_that_does_not_compile_at_its_end_is_rolled_back_byte_for_byte() {
    // Given
    let workspace = two_crates_and_a_test_binary_that_reads_a_file_beside_it();
    let before = the_tree(&workspace);
    let plan = workspace.a_plan_of(&[
        renaming("level", "tier", Some("relocation")),
        moving_the_test_binary(Some("relocation")),
    ]);

    // When
    let refusal = applying_the_plan_with(&workspace, plan.clone(), |_| {})
        .await
        .expect_err("a group whose end does not compile fails the run");

    // Then
    assert_rolled_back_for_not_compiling(&refusal, "relocation");
    // rustc's own wording, quoted from the check; its exact text is the toolchain's, not ours.
    assert!(
        refusal.contains("couldn't read"),
        "the refusal does not carry the compiler's error:\n{refusal}"
    );
    assert_eq!(the_tree(&workspace), before);
    assert_eq!(
        the_group_records_of(&workspace, &plan),
        started_and_rolled_back("relocation")
    );
    assert_eq!(the_ledger_checkpoint_of(&workspace, &plan), None);
}

#[tokio::test(flavor = "multi_thread")]
async fn earlier_operations_stay_applied_when_a_later_group_rolls_back() {
    // Given
    let workspace = two_crates_and_a_test_binary_that_reads_a_file_beside_it();
    let mut expected = the_tree(&workspace);
    expected.insert(
        ORIGIN_LIB.to_string(),
        TWO_FUNCTIONS.replacen("level", "tier", 1),
    );
    let plan = workspace.a_plan_of(&[
        renaming("level", "tier", None),
        renaming("depth", "height", Some("relocation")),
        moving_the_test_binary(Some("relocation")),
    ]);

    // When
    let refusal = applying_the_plan_with(&workspace, plan.clone(), |_| {})
        .await
        .expect_err("a group whose end does not compile fails the run");

    // Then
    assert_rolled_back_for_not_compiling(&refusal, "relocation");
    assert_eq!(the_tree(&workspace), expected);
    assert_eq!(
        the_group_records_of(&workspace, &plan),
        started_and_rolled_back("relocation")
    );
    assert_eq!(
        the_ledger_checkpoint_of(&workspace, &plan).map(|checkpoint| checkpoint.op),
        Some(0)
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_rolled_back_group_restores_created_and_renamed_files() {
    // Given
    let workspace = two_crates_and_a_test_binary_that_reads_a_file_beside_it();
    let before = the_tree(&workspace);
    let plan = workspace.a_plan_of(&[
        extracting_depth_to_a_file(&workspace, "relocation"),
        moving_the_test_binary(Some("relocation")),
    ]);

    // When
    let refusal = applying_the_plan_with(&workspace, plan.clone(), |_| {})
        .await
        .expect_err("a group whose end does not compile fails the run");

    // Then
    assert_rolled_back_for_not_compiling(&refusal, "relocation");
    assert_eq!(the_tree(&workspace), before);
    assert_eq!(
        the_group_records_of(&workspace, &plan),
        started_and_rolled_back("relocation")
    );
    assert_eq!(the_ledger_checkpoint_of(&workspace, &plan), None);
    // And the rollback had something to undo: the extraction really created the file, which the
    // journal knew to be absent beforehand
    assert!(
        the_pre_images_of(&workspace, &plan, "relocation").contains(&PreImage {
            path: "crates/origin/src/depths.rs".to_string(),
            contents: None,
        }),
        "the extraction created no file for the rollback to remove"
    );
}

/// A member that fails *before* the group's end gate — here a rename of a symbol the file does not
/// declare, which only resolving it against the tree discovers — must not leave the members before
/// it applied: the group is a unit, so its first member is undone and the run fails with the
/// member's own error, not the end gate's.
#[tokio::test(flavor = "multi_thread")]
async fn a_group_member_that_cannot_be_resolved_rolls_the_whole_group_back() {
    // Given an ungrouped rename, then a group whose second member renames a symbol nothing declares
    let workspace = two_crates_and_a_test_binary_that_reads_a_file_beside_it();
    let mut expected = the_tree(&workspace);
    expected.insert(
        ORIGIN_LIB.to_string(),
        TWO_FUNCTIONS.replacen("level", "tier", 1),
    );
    let plan = workspace.a_plan_of(&[
        renaming("level", "tier", None),
        renaming("depth", "height", Some("renames")),
        renaming("Circle", "Disc", Some("renames")),
    ]);

    // When
    let refusal = applying_the_plan_with(&workspace, plan.clone(), |_| {})
        .await
        .expect_err("a member that cannot be resolved fails the run");

    // Then the failure is the member's own
    assert!(
        refusal.contains("Circle") && !refusal.contains("does not compile"),
        "the run did not fail with the unresolvable member's own error:\n{refusal}"
    );
    // And the group is undone, the operation before it is not
    assert_eq!(the_tree(&workspace), expected);
    assert_eq!(
        the_group_records_of(&workspace, &plan),
        started_and_rolled_back("renames")
    );
    assert_eq!(
        the_ledger_checkpoint_of(&workspace, &plan).map(|checkpoint| checkpoint.op),
        Some(0)
    );
}

/// The account says what is on disk: a member whose group is then rolled back is not reported as
/// applied, only the operation before the group is.
#[tokio::test(flavor = "multi_thread")]
async fn a_rolled_back_group_reports_none_of_its_members_as_applied() {
    // Given an ungrouped rename, then a group that does not compile at its end
    let workspace = two_crates_and_a_test_binary_that_reads_a_file_beside_it();
    let plan = workspace.a_plan_of(&[
        renaming("level", "tier", None),
        renaming("depth", "height", Some("relocation")),
        moving_the_test_binary(Some("relocation")),
    ]);

    // When
    let (summary, account) = applying_and_collecting_the_account(&workspace, plan).await;

    // Then only the ungrouped operation is reported applied
    assert!(summary.is_err(), "the group did not fail the run");
    assert_eq!(
        the_applied_and_group_lines(&account),
        [console::operation(0, 0, 3, "RenameSymbol", 1, true).as_str()]
    );
}

/// Once its group is kept, every member is reported, each followed by the group it belongs to.
#[tokio::test(flavor = "multi_thread")]
async fn a_kept_group_reports_each_member_applied_with_its_group() {
    // Given a group of two renames that compiles at its end
    let workspace = two_crates_and_a_test_binary_that_reads_a_file_beside_it();
    let plan = workspace.a_plan_of(&[
        renaming("level", "tier", Some("renames")),
        renaming("depth", "height", Some("renames")),
    ]);

    // When
    let (summary, account) = applying_and_collecting_the_account(&workspace, plan).await;

    // Then
    assert!(summary.is_ok(), "the group did not apply: {summary:?}");
    assert_eq!(
        the_applied_and_group_lines(&account),
        [
            console::operation(0, 0, 2, "RenameSymbol", 1, true).as_str(),
            console::group("renames").as_str(),
            console::operation(1, 1, 2, "RenameSymbol", 1, true).as_str(),
            console::group("renames").as_str(),
        ]
    );
}

/// A crash inside a group leaves half of a unit applied. A resume cannot continue from the middle
/// — the group's end gate would judge members applied by two different runs — so it rolls the
/// partial group back from its pre-images and applies the group again, whole.
#[tokio::test(flavor = "multi_thread")]
async fn resume_inside_a_group_rolls_the_partial_group_back_and_reapplies_it() {
    // Given
    let workspace = two_crates_and_a_test_binary_that_reads_a_file_beside_it();
    let plan = workspace.a_plan_of(&[
        with_id(renaming("level", "tier", Some("renames")), "op-1"),
        with_id(renaming("depth", "height", Some("renames")), "op-2"),
    ]);
    a_crash_after_the_first_member_of(&workspace, &plan);

    // When
    applying_the_plan_with(&workspace, plan.clone(), |options| options.resume = true)
        .await
        .expect("the resumed run applies the group");

    // Then
    assert_eq!(
        the_group_records_of(&workspace, &plan),
        [
            (OpStatus::GroupStarted, "renames".to_string()),
            (OpStatus::GroupRolledBack, "renames".to_string()),
            (OpStatus::GroupStarted, "renames".to_string()),
            (OpStatus::GroupCompleted, "renames".to_string()),
        ]
    );
    assert_eq!(
        workspace.read(ORIGIN_LIB),
        "pub fn tier() -> u32 {\n    2\n}\n\npub fn height() -> u32 {\n    3\n}\n"
    );
}

/// A group stands or falls whole, so a refused member refuses the group — one finding naming it,
/// not one per member as if each could be fixed and applied alone.
#[tokio::test(flavor = "multi_thread")]
async fn check_deep_reports_one_finding_for_a_group_with_a_refused_member() {
    // Given a group of two renames, neither of whose symbols the file declares
    let workspace = two_crates_and_a_test_binary_that_reads_a_file_beside_it();
    let plan = workspace.a_plan_of(&[
        renaming("Circle", "Disc", Some("shapes")),
        renaming("Square", "Block", Some("shapes")),
    ]);

    // When
    let findings = checking_the_plan(&workspace, plan, true).await;

    // Then
    let one_naming_the_group = findings.map(|findings| {
        (
            findings.len(),
            findings
                .first()
                .is_some_and(|finding| finding.contains("group `shapes`")),
        )
    });
    assert_eq!(one_naming_the_group, Ok((1, true)));
}

/// Today's contract for ungrouped operations, kept: a run whose result does not compile fails and
/// leaves its edits on disk for inspection. Groups opt in to rollback; nothing else does.
#[tokio::test(flavor = "multi_thread")]
async fn an_ungrouped_failing_run_still_leaves_its_edits_on_disk() {
    // Given
    let workspace = two_crates_and_a_test_binary_that_reads_a_file_beside_it();

    // When
    let refusal = applying_a_plan_of(
        &workspace,
        &[
            renaming("level", "tier", None),
            moving_the_test_binary(None),
        ],
    )
    .await
    .expect_err("an apply whose result does not compile is a failed run");

    // Then
    assert!(
        refusal.starts_with("2 of 2 operation(s) were applied, and the tree no longer compiles"),
        "the apply failed, but not because the tree it left does not compile:\n{refusal}"
    );
    assert_eq!(
        (
            workspace.holds(THE_MOVED_TEST_BINARY),
            workspace.read(ORIGIN_LIB),
        ),
        (true, TWO_FUNCTIONS.replacen("level", "tier", 1)),
    );
}
