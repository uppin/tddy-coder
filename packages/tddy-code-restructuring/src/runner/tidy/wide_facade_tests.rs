//! The tidy over the facade groups a real split left: the pre-tidy `rust.rs` statements and the
//! `unused_imports` diagnostics rustc gave for them, trimmed from the failing run of #539.

use super::*;

const ROUND_ONE: &str = include_str!("../../../tests/fixtures/tidy_wide_facade/round1.jsonl");
const FACADE_GROUPS: &str =
    include_str!("../../../tests/fixtures/tidy_wide_facade/facade_groups.rs");
const SERVER_GROUP: &str = include_str!("../../../tests/fixtures/tidy_wide_facade/server_group.rs");
const FILE: &str = "packages/tddy-code-restructuring/src/backends/rust.rs";

/// Where the statements sit in the failing run's `rust.rs`; the diagnostics carry byte offsets.
const SERVER_GROUP_AT: usize = 21471;
const FACADE_GROUPS_AT: usize = 108270;

/// The tree the split left, with the compiler's findings on it.
struct TheSplitTree {
    directory: tempfile::TempDir,
    before: BTreeMap<String, Vec<u8>>,
    unused: UnusedImports,
}

fn the_tree_the_split_left() -> TheSplitTree {
    let mut text = vec![b'\n'; SERVER_GROUP_AT];
    text.extend_from_slice(SERVER_GROUP.as_bytes());
    text.resize(FACADE_GROUPS_AT, b'\n');
    text.extend_from_slice(FACADE_GROUPS.as_bytes());
    let directory = tempfile::tempdir().expect("a temporary directory");
    let path = directory.path().join(FILE);
    std::fs::create_dir_all(path.parent().expect("a parent")).expect("directories");
    std::fs::write(&path, &text).expect("the tree is written");
    let touched: BTreeSet<String> = [FILE.to_string()].into();
    TheSplitTree {
        directory,
        before: [(FILE.to_string(), text)].into(),
        unused: unused_imports(&parse(ROUND_ONE), &touched),
    }
}

impl TheSplitTree {
    fn read(&self) -> String {
        std::fs::read_to_string(self.directory.path().join(FILE)).expect("the file is read")
    }

    /// The tree after the first round's removals, as the tidy applies them.
    fn after_removing_what_the_compiler_says(&self) -> Result<String> {
        let reconciled = gating::reconcile(&self.unused, &self.before);
        apply_fixes(self.directory.path(), &reconciled.fixes)?;
        Ok(self.read())
    }

    /// The tree after a repair whose failed check quoted `names`.
    fn after_gating_what_errors_naming(&self, names: &[&str]) -> String {
        let quoted: BTreeSet<String> = names.iter().map(|name| name.to_string()).collect();
        let matched = named_by_errors(&self.unused.primaries, &self.before, &quoted);
        let placement = place(&self.unused, &self.before, &matched);
        apply_fixes(self.directory.path(), &placement.fixes).expect("the placement applies");
        self.read()
    }
}

/// The lines of `source` from `use <module>::` to the next blank line.
fn the_group_of(module: &str, source: &str) -> String {
    let from = source
        .find(&format!("mod {module};"))
        .expect("the module is declared");
    let rest = &source[from..];
    rest[..rest.find("\n\n").unwrap_or(rest.len())].to_string()
}

#[test]
fn a_group_whose_library_unused_set_contains_the_test_sets_is_rewritten_from_the_sets() {
    // Given the group the library unit calls five names unused of, and the test unit only one
    let tree = the_tree_the_split_left();

    // When a repair gates the three names the failed check could name
    let tidied = tree.after_gating_what_errors_naming(&[
        "refuse_inferred_placeholder",
        "carries_placeholder_type",
        "refuse_residual_placeholder",
    ]);

    // Then the kept names stay, the four only the tests read are gated one by one, the one
    // nothing reads is gone
    assert_eq!(
        the_group_of("placeholder_checks", &tidied),
        "mod placeholder_checks;\n\
         use placeholder_checks::{Block, declares};\n\
         #[cfg(test)]\nuse placeholder_checks::refuse_inferred_placeholder;\n\
         #[cfg(test)]\nuse placeholder_checks::carries_placeholder_type;\n\
         #[cfg(test)]\nuse placeholder_checks::refuse_residual_placeholder;\n\
         #[cfg(test)]\nuse placeholder_checks::placeholder_sites;"
    );
}

#[test]
fn the_names_only_the_library_unit_reports_are_the_ones_the_tests_read() {
    // Given the group the library unit reports five names of and the test unit one
    let tree = the_tree_the_split_left();

    // When the findings are read
    let read_by_the_tests: BTreeSet<&str> = tree
        .unused
        .read_by_a_unit
        .iter()
        .map(|span| gating::text_of(&tree.before, span))
        .filter(|name| name.contains("placeholder"))
        .collect();

    // Then the tests' names are those, and `is_identifier_byte` — reported by both — is not
    assert_eq!(
        read_by_the_tests,
        BTreeSet::from([
            "carries_placeholder_type",
            "placeholder_sites",
            "refuse_inferred_placeholder",
            "refuse_residual_placeholder",
        ])
    );
}

#[test]
fn never_leaves_a_name_in_a_group_whose_two_units_disagree_on_what_to_remove() {
    // Given the compiler's member edits for one group, which the two units word differently
    let tree = the_tree_the_split_left();

    // When the first round's edits are applied
    let tidied = tree
        .after_removing_what_the_compiler_says()
        .expect("the round applies");

    // Then the group is either whole or rewritten from the names, never half of each
    assert_eq!(
        the_group_of("placeholder_checks", &tidied),
        "mod placeholder_checks;\nuse placeholder_checks::{Block, declares};"
    );
}

#[test]
fn refuses_two_different_edits_over_the_same_text_instead_of_skipping_one() {
    // Given two edits that overlap and are not the same edit
    let directory = tempfile::tempdir().expect("a temporary directory");
    std::fs::write(directory.path().join("a.rs"), "use a::{b, c};\n").expect("a file");
    let edit = |start, end, replacement: &str| Fix {
        file: "a.rs".to_string(),
        start,
        end,
        replacement: replacement.to_string(),
    };
    let fixes = BTreeMap::from([(
        "a.rs".to_string(),
        BTreeSet::from([edit(7, 11, ""), edit(9, 13, "")]),
    )]);

    // When they are applied
    let applied = apply_fixes(directory.path(), &fixes);

    // Then the application fails, and the file is as it was
    assert!(
        applied.is_err(),
        "an edit was dropped silently: {applied:?}"
    );
    let text = std::fs::read_to_string(directory.path().join("a.rs")).expect("the file");
    assert_eq!(text, "use a::{b, c};\n");
}

/// A round that has been redone with `gated` names gated, whose check then quoted `failing` names.
fn a_round_redone(attempts: usize, failing: usize, gated: usize) -> Round {
    Round {
        before: BTreeMap::new(),
        unused: UnusedImports {
            fixes: BTreeMap::new(),
            primaries: BTreeSet::new(),
            read_by_a_unit: BTreeSet::new(),
        },
        applied: Vec::new(),
        declined: Vec::new(),
        placement: Some(Placement {
            fixes: BTreeMap::new(),
            gated: Vec::new(),
            declined: Vec::new(),
        }),
        repairs: Repairs {
            attempts,
            failing,
            matched: names_gated(gated),
        },
    }
}

fn names_gated(count: usize) -> BTreeSet<Span> {
    (0..count)
        .map(|at| Span {
            file: "a.rs".to_string(),
            start: at,
            end: at + 1,
        })
        .collect()
}

/// What the next repair would have learned: it is attempt `attempts` and the check that failed
/// quoted `failing` names, of which `gated` are now to be gated.
fn the_next_repair(attempts: usize, failing: usize, gated: usize) -> Repairs {
    Repairs {
        attempts,
        failing,
        matched: names_gated(gated),
    }
}

#[test]
fn redoes_a_round_again_while_each_redo_leaves_fewer_names_failing() {
    // Given a round redone once, whose check quoted five names
    let round = a_round_redone(1, 5, 3);

    // When the redo's check quotes three
    let again = round.keeps_improving(&the_next_repair(2, 3, 5));

    // Then it is worth redoing again
    assert!(again);
}

#[test]
fn gives_up_when_a_redo_leaves_as_many_names_failing_as_before() {
    // Given a round redone once, whose check quoted five names
    let round = a_round_redone(1, 5, 3);

    // When the redo's check quotes five again
    let again = round.keeps_improving(&the_next_repair(2, 5, 5));

    // Then the round is undone, not redone a third time
    assert!(!again);
}

#[test]
fn gives_up_when_a_redo_has_nothing_new_to_gate() {
    // Given a round redone once, whose check quoted five names
    let round = a_round_redone(1, 5, 3);

    // When the redo's check quotes fewer, but every name it quotes is already gated
    let again = round.keeps_improving(&the_next_repair(2, 4, 3));

    // Then redoing it would write the same text again
    assert!(!again);
}

#[test]
fn gives_up_after_five_redos_however_well_they_go() {
    // Given a round redone five times, each time with fewer names failing
    let round = a_round_redone(5, 6, 9);

    // When the sixth redo's check still quotes fewer
    let again = round.keeps_improving(&the_next_repair(6, 2, 12));

    // Then the round is undone
    assert!(!again);
}
