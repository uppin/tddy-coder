//! Guard: a test that starts the LiveKit testkit never names its room with a fixed literal.
//!
//! A fixed room is private only while every test owns its own server. The moment one server is
//! shared (`LIVEKIT_TESTKIT_WS_URL`) or the tests run side by side, two tests on one name interfere,
//! and nothing but this scan notices a *new* one. A test's room comes from
//! `LiveKitTestkit::unique_room`.

use std::fs;
use std::path::{Path, PathBuf};

/// Test files that start the testkit but never connect a participant, so their names touch no room.
const TOKEN_ONLY: &[&str] = &["tddy-livekit-testkit/tests/livekit_testkit_integration.rs"];

/// Source shapes that fix a room name. Each is a substring of a single line.
fn fixes_a_room_name(line: &str) -> bool {
    let code = line.trim_start();
    if code.starts_with("//") {
        return false;
    }
    let is_room_const = code.contains("ROOM") && code.contains(": &str = \"");
    let is_room_binding = code.contains("room_name = \"") || code.contains("room_name: &str = \"");
    let is_literal_token_room = code.contains("generate_token(\"");
    let is_literal_harness_room = code.contains("::start(&livekit, \"");
    is_room_const || is_room_binding || is_literal_token_room || is_literal_harness_room
}

fn rust_files_under(dir: &Path, found: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            rust_files_under(&path, found);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            found.push(path);
        }
    }
}

fn livekit_test_files(packages: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    for package in fs::read_dir(packages).expect("read packages/").flatten() {
        rust_files_under(&package.path().join("tests"), &mut files);
    }
    files
        .into_iter()
        .filter(|file| fs::read_to_string(file).is_ok_and(|text| text.contains("LiveKitTestkit")))
        .collect()
}

#[test]
fn no_test_that_starts_the_livekit_testkit_names_its_room_with_a_fixed_literal() {
    // Given every test file in the workspace that starts the LiveKit testkit
    let packages = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let files = livekit_test_files(&packages);
    assert!(
        !files.is_empty(),
        "found no LiveKit test files under {packages:?}"
    );

    // When each line is checked for a fixed room name
    let mut offenders = Vec::new();
    for file in files {
        let relative = file.strip_prefix(&packages).expect("under packages/");
        if TOKEN_ONLY.iter().any(|allowed| relative.ends_with(allowed)) {
            continue;
        }
        let text = fs::read_to_string(&file).expect("read test file");
        for (index, line) in text.lines().enumerate() {
            if fixes_a_room_name(line) {
                offenders.push(format!(
                    "{}:{}: {}",
                    relative.display(),
                    index + 1,
                    line.trim()
                ));
            }
        }
    }

    // Then there is none; each offender is reported with where it is and how to fix it
    assert!(
        offenders.is_empty(),
        "{} fixed LiveKit room name(s); build the room with LiveKitTestkit::unique_room(\"<purpose>\"):\n{}",
        offenders.len(),
        offenders.join("\n")
    );
}
