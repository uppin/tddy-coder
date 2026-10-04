//! `LiveKitTestkit::unique_room` names a room no other test will use.
//!
//! These tests need no Docker: the helper only builds a name.

use std::collections::HashSet;
use std::thread;
use tddy_livekit_testkit::LiveKitTestkit;

#[test]
fn a_unique_room_keeps_the_prefix_it_was_given() {
    // Given a prefix naming what the room is for
    let prefix = "roster-metadata";

    // When a unique room is made from it
    let room = LiveKitTestkit::unique_room(prefix);

    // Then the name starts with that prefix, so a leaked room can be attributed
    assert!(
        room.starts_with(&format!("{prefix}-")),
        "room {room:?} should start with {prefix:?}-"
    );
}

#[test]
fn two_unique_rooms_from_one_prefix_differ() {
    // Given two rooms made from the same prefix in the same process
    let first = LiveKitTestkit::unique_room("same-purpose");
    let second = LiveKitTestkit::unique_room("same-purpose");

    // Then they are different rooms
    assert_ne!(first, second);
}

#[test]
fn unique_rooms_made_on_many_threads_never_repeat() {
    // Given sixteen threads each making fifty rooms from one prefix
    let workers: Vec<_> = (0..16)
        .map(|_| {
            thread::spawn(|| {
                (0..50)
                    .map(|_| LiveKitTestkit::unique_room("contended"))
                    .collect::<Vec<_>>()
            })
        })
        .collect();

    // When every name is collected
    let names: Vec<String> = workers
        .into_iter()
        .flat_map(|worker| worker.join().expect("a worker thread panicked"))
        .collect();

    // Then no name was handed out twice
    let distinct: HashSet<&String> = names.iter().collect();
    assert_eq!(distinct.len(), names.len(), "a room name was repeated");
}

#[test]
fn a_unique_room_is_a_valid_livekit_room_name() {
    // Given a room made from a plain prefix
    let room = LiveKitTestkit::unique_room("attach-cross-host");

    // Then it holds only lowercase letters, digits and dashes, and is not empty or oversized
    assert!(
        room.chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-'),
        "room {room:?} holds a character outside [a-z0-9-]"
    );
    assert!(room.len() <= 128, "room {room:?} is longer than 128 bytes");
}
