//! The renderer is reachable from outside this crate, over the vocabulary the library returns.
//!
//! This is the whole point of publishing it. `restructure_cli`'s `report`, `report_findings` and
//! `report_comparison` were private, and that module's only public entry point owns an entire run
//! *including spawning a language server* — the one thing a client of the warm index daemon must
//! not do. So the two other front ends, `tddy_tools::index_console` and
//! `tddy_index_daemon::render`, each re-stated the wording, and the same operation read three ways.
//!
//! A front end reaching these functions with an `Outcome` it got from anywhere — a library call, a
//! stream of RPC events it folded — is what this suite asserts. It returns lines rather than
//! printing them, which is also what keeps `only_the_command_line_front_end_writes_to_standard_output`
//! true and what lets the daemon, which logs, use it at all.

use tddy_code_restructuring::console;
use tddy_code_restructuring::plan::{Anchor, Fingerprint, ItemPath};
use tddy_code_restructuring::runner::{Finding, Outcome, PlanProgress, RunSummary};
use tddy_code_restructuring::verify::{Comparison, Excused};

#[test]
fn renders_an_applied_run_for_a_front_end_that_never_started_a_language_server() {
    // Given what an apply of a whole plan amounted to, as any front end could hold it
    let outcome = Outcome::Applied(RunSummary {
        applied: 3,
        total: 3,
        stopped_early: false,
    });

    // When it is rendered
    let lines = console::outcome(&outcome, false);

    // Then the lines come back to be written wherever this front end writes
    assert_eq!(lines, vec!["applied 3 of 3 operations"]);
}

#[test]
fn renders_a_rehearsed_run_as_resolved_rather_than_applied() {
    // Given a run that rehearsed three operations against an overlay and wrote nothing
    let outcome = Outcome::Applied(RunSummary {
        applied: 3,
        total: 3,
        stopped_early: false,
    });

    // When it is rendered as the rehearsal it was
    let lines = console::outcome(&outcome, true);

    // Then it does not claim to have applied anything
    assert_eq!(lines, vec!["resolved 3 of 3 operations"]);
}

#[test]
fn renders_a_run_that_stopped_where_it_was_told_to_before_what_it_amounted_to() {
    // Given a run that honoured `--stop-after 2` of a five-operation plan
    let outcome = Outcome::Applied(RunSummary {
        applied: 2,
        total: 5,
        stopped_early: true,
    });

    // When it is rendered
    let lines = console::outcome(&outcome, false);

    // Then it says so first, because a partial run that looked complete is the misreading this
    // line exists to prevent
    assert_eq!(
        lines,
        vec![
            "   stopped after 2 operations as requested",
            "applied 2 of 5 operations",
        ]
    );
}

#[test]
fn renders_a_plans_four_journal_counts_in_the_order_a_reader_reads_them() {
    // Given the journal of a two-operation plan that has not run
    let outcome = Outcome::Status(PlanProgress {
        completed: 0,
        in_flight: 0,
        pending: 2,
        failed: 0,
    });

    // When it is rendered
    let lines = console::outcome(&outcome, false);

    // Then all four counts are there, in one order rather than three
    assert_eq!(
        lines,
        vec!["completed 0", "in_flight 0", "failed 0", "pending 2"]
    );
}

#[test]
fn renders_every_finding_attributed_to_the_operation_that_caused_it() {
    // Given a check that found two things wrong
    let outcome = Outcome::Checked(vec![
        Finding {
            operation: 0,
            detail: "the anchor names no such symbol".to_string(),
        },
        Finding {
            operation: 1,
            detail: "the move would introduce a cycle".to_string(),
        },
    ]);

    // When it is rendered
    let lines = console::outcome(&outcome, false);

    // Then each finding names its operation
    assert_eq!(
        lines,
        vec![
            "0: the anchor names no such symbol",
            "1: the move would introduce a cycle",
        ]
    );
}

#[test]
fn renders_a_check_that_found_nothing_as_having_found_nothing() {
    // Given a check of a sound plan
    let outcome = Outcome::Checked(Vec::new());

    // When it is rendered
    let lines = console::outcome(&outcome, false);

    // Then the empty report is stated rather than absent — a silent check is indistinguishable
    // from one that never ran
    assert_eq!(lines, vec!["no findings"]);
}

#[test]
fn renders_an_anchor_as_the_json_document_a_plan_carries_it_as() {
    // Given the anchor over a run of two sibling items, as `restructure anchors --items` found it
    let outcome = Outcome::ItemAnchored(Anchor::Items {
        file: "src/lib.rs".to_string(),
        items: vec![
            ItemPath::parse("stacks::workflow::Alpha").expect("a crate-rooted path"),
            ItemPath::parse("stacks::workflow::Beta").expect("a crate-rooted path"),
        ],
        fingerprints: vec![
            Fingerprint("sha256:aa".into()),
            Fingerprint("sha256:bb".into()),
        ],
    });

    // When it is rendered
    let lines = console::outcome(&outcome, false);

    // Then it is the JSON a caller pastes into a plan, not prose about it
    assert_eq!(
        lines,
        vec![
            r#"{"kind":"items","file":"src/lib.rs","items":["stacks::workflow::Alpha","stacks::workflow::Beta"],"fingerprints":["sha256:aa","sha256:bb"]}"#
        ]
    );
}

#[test]
fn renders_a_comparison_that_holds_as_every_statement_accounted_for() {
    // Given a tree whose statements are exactly those of the ref it was held against
    let outcome = Outcome::Verified(Comparison {
        before: 2,
        after: 2,
        missing: Vec::new(),
        added: Vec::new(),
        ..Comparison::default()
    });

    // When it is rendered
    let lines = console::outcome(&outcome, false);

    // Then both counts and the verdict are stated
    assert_eq!(
        lines,
        vec![
            "2 statements before, 2 after",
            "every statement accounted for"
        ]
    );
}

#[test]
fn renders_the_statements_a_tree_lost_and_gained_without_stating_the_verdict() {
    // Given a tree that traded one statement for another since the ref
    let comparison = Comparison {
        before: 2,
        after: 2,
        missing: vec!["1".to_string()],
        added: vec!["2".to_string()],
        ..Comparison::default()
    };

    // When it is rendered
    let lines = console::outcome(&Outcome::Verified(comparison.clone()), false);

    // Then each side is named, and the refusal is separate — what a comparison that does not hold
    // *means* is the front end's to decide, and each one turns it into a different thing
    assert_eq!(
        lines,
        vec![
            "2 statements before, 2 after",
            "verify: tokens lost: 1 x1; tokens gained: 2 x1",
            "missing: 1",
            "added:   2",
        ]
    );
    assert_eq!(
        console::comparison_refusal(&comparison),
        "1 statement(s) the tree lost and 1 it gained — see above"
    );
}

#[test]
fn renders_one_summary_line_for_the_churn_a_comparison_excused() {
    // Given a tree that held once a re-point, two widenings and a cfg(test) gate were set aside
    let comparison = Comparison {
        before: 5,
        after: 6,
        excused: Excused {
            repointed: 1,
            visibility: 2,
            cfg_test_gates: 1,
        },
        ..Comparison::default()
    };

    // When it is rendered
    let lines = console::outcome(&Outcome::Verified(comparison), false);

    // Then the counts are stated once, between the totals and the verdict
    assert_eq!(
        lines,
        vec![
            "5 statements before, 6 after",
            "verify: 1 statement(s) re-pointed through a module qualifier, 2 visibility-normalised, 1 cfg(test) gate line(s) excused",
            "every statement accounted for"
        ]
    );
}

#[test]
fn omits_the_zero_parts_of_the_excused_summary() {
    // Given a tree where only widenings were excused
    let comparison = Comparison {
        excused: Excused {
            visibility: 3,
            ..Excused::default()
        },
        ..Comparison::default()
    };

    // When it is rendered
    let lines = console::outcome(&Outcome::Verified(comparison), false);

    // Then only that part is named
    assert_eq!(lines[1], "verify: 3 visibility-normalised");
}

#[test]
fn states_why_a_check_that_found_something_is_a_failed_run() {
    // Given a check that made two findings
    // When the refusal is rendered
    // Then it says how many, and that the tree was left alone
    assert_eq!(
        console::findings_refusal(2),
        "2 finding(s) — see above. Nothing was written."
    );
}
