//! Unit: the common-room advertisement carries the host's attachment size cap.
//!
//! PRD: `docs/ft/web/1-WIP/PRD-2026-08-01-session-attach-ui.md`
//! Changeset: `docs/dev/1-WIP/2026-08-01-session-attach-ui.md`
//!
//! `max_attachment_bytes` is a per-host policy, so the Start-Session form has to learn it from the
//! host it is about to stage to. It travels on the existing `DaemonAdvertisement` rather than a new
//! RPC — the web already parses that metadata into a `DaemonHost`. Mirrors how `repos_base_path` is
//! advertised, including staying optional so an older daemon's advertisement still parses.

use tddy_daemon::livekit_peer_discovery::{parse_daemon_advertisement_json, DaemonAdvertisement};

const SIXTY_FOUR_MIB: u64 = 64 * 1024 * 1024;

#[test]
fn an_advertisement_round_trips_its_attachment_cap() {
    // Given — an advertisement carrying a 64 MiB cap
    let advertisement = DaemonAdvertisement {
        instance_id: "udoo".to_string(),
        label: "udoo (this daemon)".to_string(),
        repos_base_path: "repos".to_string(),
        max_attachment_bytes: SIXTY_FOUR_MIB,
    };

    // When — it is serialized and parsed back the way the discovery transport does
    let json = serde_json::to_string(&advertisement).expect("advertisement must serialize");
    let parsed = parse_daemon_advertisement_json(&json).expect("advertisement must parse");

    // Then
    assert_eq!(parsed, advertisement);
}

#[test]
fn an_advertisement_serializes_the_cap_under_its_snake_case_wire_name() {
    // Given
    let advertisement = DaemonAdvertisement {
        instance_id: "udoo".to_string(),
        label: "udoo (this daemon)".to_string(),
        repos_base_path: String::new(),
        max_attachment_bytes: SIXTY_FOUR_MIB,
    };

    // When
    let json = serde_json::to_string(&advertisement).expect("advertisement must serialize");

    // Then — the web parses this exact key (`participantRole.ts`)
    assert!(
        json.contains("\"max_attachment_bytes\":67108864"),
        "advertisement must carry the cap under its wire name, was {json}"
    );
}

#[test]
fn an_advertisement_from_a_daemon_predating_the_cap_still_parses() {
    // Given — metadata published before the field existed
    let json = r#"{"instance_id":"udoo","label":"udoo (this daemon)"}"#;

    // When
    let parsed = parse_daemon_advertisement_json(json).expect("older advertisement must parse");

    // Then — zero stands for "unadvertised"; the web treats it as no cap rather than a cap of none
    assert_eq!(parsed.max_attachment_bytes, 0);
    assert_eq!(parsed.instance_id, "udoo");
}

#[test]
fn an_unadvertised_cap_is_left_out_of_the_wire_form() {
    // Given — a daemon with no cap to advertise
    let advertisement = DaemonAdvertisement {
        instance_id: "udoo".to_string(),
        label: "udoo (this daemon)".to_string(),
        repos_base_path: String::new(),
        max_attachment_bytes: 0,
    };

    // When
    let json = serde_json::to_string(&advertisement).expect("advertisement must serialize");

    // Then — omitted rather than sent as 0, so a reader cannot mistake it for a real cap of zero
    assert!(
        !json.contains("max_attachment_bytes"),
        "an unadvertised cap must be omitted, was {json}"
    );
}
