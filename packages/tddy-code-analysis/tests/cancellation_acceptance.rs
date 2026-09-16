//! What the two long analyses do when nobody is waiting for them any more.
//!
//! A capture is 55 minutes and duplicate detection ~22, so both have to be stoppable: a client
//! that hangs up one minute in must not leave the other 54 running. The surface is a predicate the
//! caller supplies — `&dyn Fn() -> bool`, checked between units of work — because this crate is
//! synchronous, carries no tokio and needs no dependency to read a closure.
//!
//! **Nothing here runs a real capture or a real detection.** A predicate that is already true is
//! checked at the same places a predicate that turns true half way through is, so it proves the
//! same plumbing in milliseconds: the work refuses to start, and it says so as a refusal rather
//! than as a success over partial artefacts.

use pretty_assertions::assert_eq;
use tddy_code_analysis::coverage::{capture_coverage, CaptureProgress};
use tddy_code_analysis::duplicate_tests::analyze_coverage_dir;
use tddy_code_analysis::AnalysisError;

/// A caller that has already gone away.
fn already_cancelled() -> bool {
    true
}

/// A crate directory with a manifest and one source file.
///
/// Enough for a capture to resolve the crate it was asked about, and deliberately not paired with
/// anything else: if a cancelled capture got as far as cargo, this test would take minutes rather
/// than milliseconds, which is itself the assertion.
fn a_crate_directory() -> tempfile::TempDir {
    let crate_dir = tempfile::tempdir().expect("a temporary crate directory");
    std::fs::write(
        crate_dir.path().join("Cargo.toml"),
        "[package]\nname = \"measured\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    )
    .expect("a manifest");
    std::fs::create_dir_all(crate_dir.path().join("src")).expect("a source directory");
    std::fs::write(crate_dir.path().join("src/lib.rs"), "pub fn f() {}\n").expect("a source file");
    crate_dir
}

/// A capture of two tests, as the per-test artefacts duplicate detection reads.
fn a_capture_of_two_tests() -> tempfile::TempDir {
    let coverage = tempfile::tempdir().expect("a temporary coverage directory");
    let per_test = coverage.path().join("per-test");
    std::fs::create_dir_all(&per_test).expect("a per-test directory");
    for (id, name) in [("aaaa", "covers_one"), ("bbbb", "covers_the_same")] {
        std::fs::write(
            per_test.join(format!("{id}.meta.json")),
            serde_json::json!({
                "id": id,
                "name": name,
                "full_name": name,
                "spec": "src/lib.rs",
                "line": null,
                "status": "ok",
                "durationMs": 1,
                "lang": "rust",
            })
            .to_string(),
        )
        .expect("a test's metadata");
        std::fs::write(
            per_test.join(format!("{id}.rust.json")),
            serde_json::json!({
                "/src/lib.rs": {
                    "regions": [{
                        "startLine": 1, "startCol": 1, "endLine": 1, "endCol": 10,
                        "count": 1, "kind": "code",
                    }],
                },
            })
            .to_string(),
        )
        .expect("a test's regions");
    }
    coverage
}

/// The work a cancellation names and how far it got, or a panic saying what arrived instead.
fn cancellation(refusal: AnalysisError) -> (String, String) {
    match refusal {
        AnalysisError::Cancelled { work, reached } => (work, reached),
        other => panic!("expected a cancellation, got {other:?}"),
    }
}

/// Every phase a capture reported, so a test can say the build never began.
fn phase(progress: &CaptureProgress<'_>) -> String {
    format!("{progress:?}")
}

#[test]
fn refuses_a_capture_whose_caller_went_away_before_the_build_began() {
    // Given a crate to capture and a caller that is already gone
    let crate_dir = a_crate_directory();
    let coverage_dir = crate_dir.path().join("coverage");
    let mut reported: Vec<String> = Vec::new();

    // When a capture is asked for
    let outcome = capture_coverage(
        crate_dir.path(),
        &coverage_dir,
        &already_cancelled,
        &mut |progress| reported.push(phase(&progress)),
    );

    // Then it is refused as cancelled, naming how far it got — and it reported no phase at all,
    // which is what says the instrumented build, the one stretch of a capture nothing can
    // interrupt, was never started
    assert_eq!(
        cancellation(outcome.expect_err("a cancelled capture is not a success")),
        (
            "coverage capture".to_string(),
            "0 test(s) captured, and no denominator was written".to_string()
        )
    );
    assert_eq!(reported, Vec::<String>::new());
}

#[test]
fn writes_nothing_into_the_coverage_directory_of_a_cancelled_capture() {
    // Given a crate to capture and a caller that is already gone
    let crate_dir = a_crate_directory();
    let coverage_dir = crate_dir.path().join("coverage");

    // When a capture is asked for
    let outcome = capture_coverage(
        crate_dir.path(),
        &coverage_dir,
        &already_cancelled,
        &mut |_| {},
    );

    // Then the directory a later report would read was never even created, so nothing can mistake
    // a stopped capture for a thin one
    outcome.expect_err("a cancelled capture is not a success");
    assert!(
        !coverage_dir.exists(),
        "a cancelled capture created {}",
        coverage_dir.display()
    );
}

#[test]
fn refuses_duplicate_detection_whose_caller_went_away_before_a_signature_was_read() {
    // Given a capture on disk and a caller that is already gone
    let coverage = a_capture_of_two_tests();

    // When the detection is asked for
    let outcome = analyze_coverage_dir(coverage.path(), 5, 0.5, false, &already_cancelled);

    // Then it is refused as cancelled, counting the signatures it had been asked to read
    assert_eq!(
        cancellation(outcome.expect_err("a cancelled detection is not a success")),
        (
            "duplicate-test detection".to_string(),
            "0 of 2 per-test signatures loaded".to_string()
        )
    );
}
