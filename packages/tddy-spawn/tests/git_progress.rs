//! Git's clone progress lines parse into stage, percent, counts, bytes and throughput, and a throttle
//! keeps only the redraws worth forwarding.
//!
//! The lines are git's own (2.4x), including the `remote: ` prefix and the `, done.` tail.
//!
//! Feature docs: docs/ft/daemon/project-concept.md

use std::time::{Duration, Instant};

use tddy_spawn::git_progress::{parse, GitProgress, ProgressThrottle, PROGRESS_INTERVAL};

fn progress(
    stage: &str,
    percent: u32,
    done: u64,
    total: u64,
    bytes: u64,
    rate: u64,
    line: &str,
) -> GitProgress {
    GitProgress {
        stage: stage.to_string(),
        percent,
        objects_done: done,
        objects_total: total,
        bytes_received: bytes,
        bytes_per_second: rate,
        line: line.to_string(),
    }
}

#[test]
fn a_receiving_line_carries_bytes_and_throughput() {
    let line = "Receiving objects:  45% (36555/81234), 38.50 MiB | 4.25 MiB/s";
    assert_eq!(
        parse(line),
        Some(progress(
            "Receiving objects",
            45,
            36555,
            81234,
            38 * 1024 * 1024 + 512 * 1024,
            4 * 1024 * 1024 + 256 * 1024,
            line
        ))
    );
}

#[test]
fn kibibyte_figures_are_bytes_too() {
    let line = "Receiving objects:  12% (12/100), 512.00 KiB | 256.00 KiB/s";
    assert_eq!(
        parse(line),
        Some(progress(
            "Receiving objects",
            12,
            12,
            100,
            512 * 1024,
            256 * 1024,
            line
        ))
    );
}

#[test]
fn a_remote_stage_loses_its_prefix_and_a_done_tail_is_not_a_figure() {
    let line = "remote: Counting objects: 100% (1200/1200), done.";
    assert_eq!(
        parse(line),
        Some(progress("Counting objects", 100, 1200, 1200, 0, 0, line))
    );
}

#[test]
fn a_resolving_line_has_counts_but_no_bytes() {
    let line = "Resolving deltas:  60% (30740/51234)";
    assert_eq!(
        parse(line),
        Some(progress("Resolving deltas", 60, 30740, 51234, 0, 0, line))
    );
}

#[test]
fn a_line_without_progress_is_not_progress() {
    assert_eq!(
        parse("Cloning into '/home/mantas/repos/tddy-coder'..."),
        None
    );
    assert_eq!(parse("remote: Enumerating objects: 81234, done."), None);
    assert_eq!(parse(""), None);
}

#[test]
fn a_throttle_passes_stage_changes_and_completion_but_not_every_tick() {
    // Given
    let mut throttle = ProgressThrottle::new();
    let t0 = Instant::now();
    let counting = progress("Counting objects", 10, 10, 100, 0, 0, "");
    let counting_more = progress("Counting objects", 11, 11, 100, 0, 0, "");
    let counting_done = progress("Counting objects", 100, 100, 100, 0, 0, "");
    let receiving = progress("Receiving objects", 1, 1, 100, 0, 0, "");

    // Then
    assert!(throttle.admit(&counting, t0), "the first line goes through");
    assert!(
        !throttle.admit(&counting_more, t0 + Duration::from_millis(10)),
        "a redraw 10 ms later does not"
    );
    assert!(
        throttle.admit(&counting_done, t0 + Duration::from_millis(20)),
        "100% always goes through"
    );
    assert!(
        throttle.admit(&receiving, t0 + Duration::from_millis(30)),
        "a new stage always goes through"
    );
    assert!(
        throttle.admit(
            &progress("Receiving objects", 2, 2, 100, 0, 0, ""),
            t0 + Duration::from_millis(30) + PROGRESS_INTERVAL
        ),
        "a redraw an interval later goes through"
    );
}
