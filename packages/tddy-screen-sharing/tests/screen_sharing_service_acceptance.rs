//! Acceptance tests: the screen-sharing control plane over the credential store.
//!
//! PRD: docs/ft/web/screen-sharing-sessions.md (AC-SS-2 through AC-SS-9).
//!
//! **There is no unlock here, and that is the point.** The service used to make the operator type a
//! second secret before it would touch a target, because the daemon had no other way to know a
//! person was present. The session-gated credential store (`#keyring` 3/9) is that other way, so the
//! passphrase, the RPC that carried it and the vault it opened are gone — and every test below
//! reaches its targets with nothing but the session token the caller already holds.
//!
//! # What is faked, and what is not
//!
//! [`AnInMemoryTargetStore`] fakes **storage only**: it keeps records in a `Vec` instead of a sealed
//! file. Everything a target's record *is* — which provider it belongs to, which account, what
//! travels in the metadata — goes through the real [`record_for`] and [`target_from`], because that
//! mapping is the whole of this node's claim and a fake that reimplemented it would test nothing.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex as StdMutex};

use tokio::sync::Mutex;

use tddy_credentials::CredentialRecord;
use tddy_rpc::{Code, Request};
use tddy_screen_sharing::screen_sharing_records::{
    account_for, record_for, screen_sharing_provider, target_from, ScreenSharingTargetStore,
    TargetError,
};
use tddy_screen_sharing::screen_sharing_service::{
    ScreenSharingKeyCache, ScreenSharingServiceImpl,
};
use tddy_service::proto::screen_sharing::{
    AddTargetRequest, ListTargetsRequest, Protocol, RemoveTargetRequest, ScreenSharingService,
    ScreenSharingTarget, StartStreamRequest,
};

const VALID_TOKEN: &str = "valid-token";
const SESSION_ID: &str = "ss-test-session-aabbccdd";
const A_DESKTOP_PASSWORD: &str = "the-desktop-password";
const WRITTEN_AT: u64 = 1_758_240_000;

type SessionsBaseFn = Arc<dyn Fn(&str) -> Option<PathBuf> + Send + Sync>;
type UserResolverFn = Arc<dyn Fn(&str) -> Option<String> + Send + Sync>;

// ---------------------------------------------------------------------------
// The store the service talks to
// ---------------------------------------------------------------------------

/// A credential store that holds its records in memory.
///
/// `locked` stands in for the one thing a real store can be that an empty one is not: present, and
/// sealed under a login key this session does not have.
#[derive(Default)]
struct AnInMemoryTargetStore {
    records: StdMutex<Vec<CredentialRecord>>,
    locked: bool,
}

impl AnInMemoryTargetStore {
    fn open() -> Arc<Self> {
        Arc::new(Self::default())
    }

    fn sealed_under_another_login() -> Arc<Self> {
        Arc::new(Self {
            records: StdMutex::new(Vec::new()),
            locked: true,
        })
    }

    fn the_records_it_holds(&self) -> Vec<CredentialRecord> {
        self.records.lock().unwrap().clone()
    }

    fn admits(&self, session_token: &str) -> Result<(), TargetError> {
        match (session_token, self.locked) {
            (VALID_TOKEN, false) => Ok(()),
            (VALID_TOKEN, true) => Err(TargetError::Locked),
            _ => Err(TargetError::NoSuchSession),
        }
    }
}

impl ScreenSharingTargetStore for AnInMemoryTargetStore {
    fn list(&self, session_token: &str) -> Result<Vec<ScreenSharingTarget>, TargetError> {
        self.admits(session_token)?;
        self.the_records_it_holds()
            .iter()
            .map(target_from)
            .collect()
    }

    fn add(
        &self,
        session_token: &str,
        target: &ScreenSharingTarget,
        password: &str,
    ) -> Result<ScreenSharingTarget, TargetError> {
        self.admits(session_token)?;

        let mut stored = target.clone();
        stored.id = format!("target-{}", self.records.lock().unwrap().len());

        let record = record_for(&stored, password, WRITTEN_AT);
        self.records.lock().unwrap().push(record);

        Ok(stored)
    }

    fn remove(&self, session_token: &str, target_id: &str) -> Result<(), TargetError> {
        self.admits(session_token)?;
        self.records
            .lock()
            .unwrap()
            .retain(|record| record.account != account_for(target_id));
        Ok(())
    }

    fn password_for(&self, session_token: &str, target_id: &str) -> Result<String, TargetError> {
        self.admits(session_token)?;
        self.the_records_it_holds()
            .into_iter()
            .find(|record| record.account == account_for(target_id))
            .map(|record| record.secret)
            .ok_or_else(|| TargetError::Malformed(format!("no target {target_id}")))
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn a_service_over(
    store: &Arc<AnInMemoryTargetStore>,
    sessions_dir: &std::path::Path,
) -> ScreenSharingServiceImpl {
    a_service_with_no_store(sessions_dir).with_target_store(Arc::clone(store) as Arc<_>)
}

fn a_service_with_no_store(sessions_dir: &std::path::Path) -> ScreenSharingServiceImpl {
    let sessions_path = sessions_dir.to_path_buf();
    let sessions_base: SessionsBaseFn = Arc::new(move |_user| Some(sessions_path.clone()));
    let user_resolver: UserResolverFn =
        Arc::new(|token| (token == VALID_TOKEN).then(|| "testuser".to_string()));
    let key_cache: ScreenSharingKeyCache = Arc::new(Mutex::new(HashMap::new()));

    ScreenSharingServiceImpl::new(user_resolver, sessions_base, key_cache)
}

fn session_dir(base: &std::path::Path) -> PathBuf {
    let dir = base.join("sessions").join(SESSION_ID);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn a_vnc_desktop() -> AddTargetRequest {
    AddTargetRequest {
        session_token: VALID_TOKEN.to_string(),
        session_id: SESSION_ID.to_string(),
        label: "VNC Dev Box".to_string(),
        host: "10.0.0.5".to_string(),
        port: 5900,
        password: A_DESKTOP_PASSWORD.to_string(),
        protocol: Protocol::Vnc as i32,
        username: String::new(),
    }
}

fn an_rdp_desktop() -> AddTargetRequest {
    AddTargetRequest {
        session_token: VALID_TOKEN.to_string(),
        session_id: SESSION_ID.to_string(),
        label: "Windows Dev Box".to_string(),
        host: "10.0.0.10".to_string(),
        port: 3389,
        password: A_DESKTOP_PASSWORD.to_string(),
        protocol: Protocol::Rdp as i32,
        username: "tester".to_string(),
    }
}

fn a_listing_for(token: &str) -> ListTargetsRequest {
    ListTargetsRequest {
        session_token: token.to_string(),
        session_id: SESSION_ID.to_string(),
    }
}

// ---------------------------------------------------------------------------
// AC-SS-2: a token that names no session reaches no targets
// ---------------------------------------------------------------------------

#[tokio::test]
async fn a_token_naming_no_session_reaches_no_targets() {
    // Given a store holding a desktop
    let tmp = tempfile::tempdir().unwrap();
    let store = AnInMemoryTargetStore::open();
    let svc = a_service_over(&store, tmp.path());

    // When a stranger lists
    let refusal = svc
        .list_targets(Request::new(a_listing_for("bad-token")))
        .await
        .expect_err("a token naming no session must be refused");

    // Then
    assert_eq!(refusal.code, Code::Unauthenticated);
}

// ---------------------------------------------------------------------------
// AC-SS-3: a target's password is a credential record
// ---------------------------------------------------------------------------

#[tokio::test]
async fn a_desktops_password_is_retained_as_a_screen_sharing_credential() {
    // Given a session with a credential store
    let tmp = tempfile::tempdir().unwrap();
    let _session = session_dir(tmp.path());
    let store = AnInMemoryTargetStore::open();
    let svc = a_service_over(&store, tmp.path());

    // When a desktop is added, with no unlock and no second secret
    svc.add_target(Request::new(a_vnc_desktop()))
        .await
        .expect("adding a desktop must succeed");

    // Then the password is a record of the `screen-sharing` provider — the same store a GitHub
    // token lives in, reached through the same session key
    let records = store.the_records_it_holds();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].provider, screen_sharing_provider());
    assert_eq!(records[0].secret, A_DESKTOP_PASSWORD);
}

// ---------------------------------------------------------------------------
// AC-SS-4: a desktop survives the session that added it
// ---------------------------------------------------------------------------

#[tokio::test]
async fn a_desktop_added_in_one_session_is_listed_by_the_next() {
    // Given a desktop added through one service instance
    let tmp = tempfile::tempdir().unwrap();
    let _session = session_dir(tmp.path());
    let store = AnInMemoryTargetStore::open();
    a_service_over(&store, tmp.path())
        .add_target(Request::new(an_rdp_desktop()))
        .await
        .expect("adding a desktop must succeed");

    // When a later session lists — no unlock in between, because the store is opened by the login
    let listing = a_service_over(&store, tmp.path())
        .list_targets(Request::new(a_listing_for(VALID_TOKEN)))
        .await
        .expect("listing must succeed")
        .into_inner();

    // Then the desktop comes back whole, protocol included
    assert_eq!(listing.targets.len(), 1);
    assert_eq!(listing.targets[0].label, "Windows Dev Box");
    assert_eq!(listing.targets[0].host, "10.0.0.10");
    assert_eq!(listing.targets[0].port, 3389);
    assert_eq!(listing.targets[0].protocol, Protocol::Rdp as i32);
    assert_eq!(listing.targets[0].username, "tester");
}

#[tokio::test]
async fn a_vnc_desktop_is_listed_as_vnc() {
    // Given a VNC desktop
    let tmp = tempfile::tempdir().unwrap();
    let _session = session_dir(tmp.path());
    let store = AnInMemoryTargetStore::open();
    let svc = a_service_over(&store, tmp.path());
    svc.add_target(Request::new(a_vnc_desktop()))
        .await
        .expect("adding a desktop must succeed");

    // When it is listed
    let listing = svc
        .list_targets(Request::new(a_listing_for(VALID_TOKEN)))
        .await
        .expect("listing must succeed")
        .into_inner();

    // Then the protocol survives the round trip through the record
    assert_eq!(listing.targets.len(), 1);
    assert_eq!(listing.targets[0].protocol, Protocol::Vnc as i32);
}

#[tokio::test]
async fn a_removed_desktop_is_no_longer_a_credential() {
    // Given a stored desktop
    let tmp = tempfile::tempdir().unwrap();
    let _session = session_dir(tmp.path());
    let store = AnInMemoryTargetStore::open();
    let svc = a_service_over(&store, tmp.path());
    let added = svc
        .add_target(Request::new(a_vnc_desktop()))
        .await
        .expect("adding a desktop must succeed")
        .into_inner()
        .target
        .expect("an added desktop comes back");

    // When it is removed
    svc.remove_target(Request::new(RemoveTargetRequest {
        session_token: VALID_TOKEN.to_string(),
        session_id: SESSION_ID.to_string(),
        target_id: added.id,
    }))
    .await
    .expect("removing a desktop must succeed");

    // Then its password is gone with it
    assert_eq!(store.the_records_it_holds(), vec![]);
}

// ---------------------------------------------------------------------------
// AC-SS-5: a sealed store is reported as sealed, never as "no desktops"
// ---------------------------------------------------------------------------

#[tokio::test]
async fn a_store_sealed_under_another_login_is_reported_as_locked() {
    // Given a store this session's key does not open
    let tmp = tempfile::tempdir().unwrap();
    let store = AnInMemoryTargetStore::sealed_under_another_login();
    let svc = a_service_over(&store, tmp.path());

    // When the desktops are listed
    let listing = svc
        .list_targets(Request::new(a_listing_for(VALID_TOKEN)))
        .await
        .expect("a sealed store is a state to render, not a refusal")
        .into_inner();

    // Then the screen is told the store is locked rather than empty — the second reading would tell
    // the operator to re-add desktops they already have
    assert!(listing.vault_locked);
    assert_eq!(listing.targets, vec![]);
}

// ---------------------------------------------------------------------------
// AC-SS-6: with no store wired there is nowhere to put a password
// ---------------------------------------------------------------------------

#[tokio::test]
async fn a_desktop_cannot_be_added_with_no_credential_store_wired() {
    // Given a service with no store
    let tmp = tempfile::tempdir().unwrap();
    let _session = session_dir(tmp.path());
    let svc = a_service_with_no_store(tmp.path());

    // When a desktop is added
    let refusal = svc
        .add_target(Request::new(a_vnc_desktop()))
        .await
        .expect_err("a password with nowhere to go must not be accepted");

    // Then it is refused rather than kept somewhere else — there is no second place for a secret
    assert_eq!(refusal.code, Code::FailedPrecondition);
}

// ---------------------------------------------------------------------------
// AC-SS-9: opening a desktop is unchanged, and needs no unlock
// ---------------------------------------------------------------------------

#[tokio::test]
async fn opening_a_desktop_returns_the_coordinates_a_viewer_needs() {
    use tddy_core::session_metadata::{
        write_initial_tool_session_metadata, InitialToolSessionMetadataOpts,
    };

    // Given a session holding a desktop
    let tmp = tempfile::tempdir().unwrap();
    let session_dir_path = session_dir(tmp.path());
    write_initial_tool_session_metadata(
        &session_dir_path,
        InitialToolSessionMetadataOpts {
            project_id: "proj-ss-1".to_string(),
            livekit_room: Some("room-ss-test".to_string()),
            ..Default::default()
        },
    )
    .expect("session metadata must write");

    let store = AnInMemoryTargetStore::open();
    let svc = a_service_over(&store, tmp.path());
    let added = svc
        .add_target(Request::new(a_vnc_desktop()))
        .await
        .expect("adding a desktop must succeed")
        .into_inner()
        .target
        .expect("an added desktop comes back");

    // When it is opened — with no unlock, because the store needs none
    let opened = svc
        .start_stream(Request::new(StartStreamRequest {
            session_token: VALID_TOKEN.to_string(),
            session_id: SESSION_ID.to_string(),
            target_id: added.id,
        }))
        .await
        .expect("opening a desktop must succeed")
        .into_inner();

    // Then the viewer gets the same coordinates it always did
    assert_eq!(opened.livekit_room, "room-ss-test");
    assert!(opened.track_name.starts_with("screenshare:"));
    assert!(opened.bridge_identity.starts_with("screenshare-"));
}
