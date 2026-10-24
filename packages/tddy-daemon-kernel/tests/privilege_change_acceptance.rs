//! Acceptance: a child is started as another OS user only by a process that can become that user;
//! anything else is refused **before** a fork, with a message naming both users.
//!
//! The decision is a pure function of the target account and the current effective identity
//! (`plan_privilege_change_from`), so every case runs on a non-root developer machine or CI runner.
//!
//! Feature docs: docs/ft/daemon/project-concept.md (Cloning at session start)
#![cfg(unix)]

use tddy_daemon_kernel::privilege_drop::{
    plan_privilege_change_from, resolve_target_account, EffectiveIdentity, PrivilegeChange,
    TargetAccount,
};

fn the_current_os_user() -> String {
    let output = std::process::Command::new("id")
        .arg("-un")
        .output()
        .unwrap();
    String::from_utf8(output.stdout).unwrap().trim().to_string()
}

fn the_current_uid() -> u32 {
    let output = std::process::Command::new("id").arg("-u").output().unwrap();
    String::from_utf8(output.stdout)
        .unwrap()
        .trim()
        .parse()
        .unwrap()
}

fn an_unprivileged_process_running_as(name: &str, uid: u32, gid: u32) -> EffectiveIdentity {
    EffectiveIdentity {
        uid,
        gid,
        name: name.to_string(),
    }
}

fn root() -> EffectiveIdentity {
    EffectiveIdentity {
        uid: 0,
        gid: 0,
        name: "root".to_string(),
    }
}

fn an_account(name: &str, uid: u32, gid: u32) -> TargetAccount {
    TargetAccount {
        name: name.to_string(),
        uid,
        gid,
        groups: vec![gid],
        home: format!("/home/{name}"),
    }
}

#[test]
fn a_non_root_process_asked_for_another_user_refuses_before_forking() {
    // Given — the incident's shape: the daemon runs as `tddy`, the session belongs to `mantas`
    let tddy = an_unprivileged_process_running_as("tddy", 998, 998);
    let mantas = an_account("mantas", 1000, 1000);

    // When
    let refusal = plan_privilege_change_from(mantas, &tddy)
        .expect_err("an unprivileged process cannot become another user");

    // Then — the message says who, who for, and where such spawns belong
    let message = refusal.to_string();
    assert_eq!(
        (refusal.running_as.as_str(), refusal.requested.as_str()),
        ("tddy", "mantas"),
        "the refusal names both users"
    );
    assert!(
        message.contains("'tddy'")
            && message.contains("'mantas'")
            && message.contains("tddy-supervisor"),
        "the message names both users and the supervisor that brokers such spawns: {message}"
    );
}

#[test]
fn a_child_for_the_process_own_identity_changes_nothing() {
    // Given
    let me = an_unprivileged_process_running_as("alice", 1000, 1000);

    // When
    let change = plan_privilege_change_from(an_account("alice", 1000, 1000), &me).unwrap();

    // Then
    assert!(
        matches!(change, PrivilegeChange::None),
        "a child running as its parent's own identity drops nothing, got {change:?}"
    );
}

#[test]
fn root_may_start_a_child_as_any_account() {
    // When
    let change = plan_privilege_change_from(an_account("mantas", 1000, 1000), &root()).unwrap();

    // Then
    assert!(
        matches!(change, PrivilegeChange::Drop(ref account) if account.uid == 1000 && account.gid == 1000),
        "root drops to the target's uid and gid, got {change:?}"
    );
}

#[test]
fn a_target_account_carries_its_supplementary_groups_resolved_before_any_fork() {
    // When
    let me = resolve_target_account(&the_current_os_user()).unwrap();

    // Then
    assert_eq!(me.uid, the_current_uid(), "the passwd entry's uid");
    assert!(
        me.groups.contains(&(me.gid as libc::gid_t)),
        "the group list setgroups(2) will set includes the primary group: {:?} vs gid {}",
        me.groups,
        me.gid
    );
    assert!(!me.home.is_empty(), "the account's home directory");
}

#[test]
fn resolving_an_account_that_does_not_exist_names_it() {
    // When
    let error = resolve_target_account("tddy-no-such-user-0x5db67307").unwrap_err();

    // Then
    assert!(
        error.contains("tddy-no-such-user-0x5db67307"),
        "the error names the missing account: {error}"
    );
}

#[test]
fn the_effective_identity_is_the_one_id_reports() {
    // When
    let me = EffectiveIdentity::current();

    // Then
    assert_eq!(
        (me.uid, me.name.as_str()),
        (the_current_uid(), the_current_os_user().as_str()),
        "effective uid and its login name"
    );
}
