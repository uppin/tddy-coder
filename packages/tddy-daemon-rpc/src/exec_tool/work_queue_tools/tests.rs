use pretty_assertions::assert_eq;
use tddy_changeset::work_queue_fixtures::{a_stack_host, A_PROJECT};
use tddy_core::changeset::{Changeset, WorkItem, WorkQueue};

use super::*;

fn a_queue_linking(child: Option<&str>) -> WorkQueue {
    WorkQueue {
        version: 1,
        nodes: vec![WorkItem {
            node_id: "n1".to_string(),
            title: "Store".to_string(),
            session_id: child.map(str::to_string),
            ..WorkItem::default()
        }],
        ..Default::default()
    }
}

#[test]
fn a_session_owning_a_queue_is_its_owner_whatever_its_role() {
    // Given a session with no orchestrator role that owns a queue
    let host = a_stack_host();
    host.orchestrator("owner", a_queue_linking(None));

    // When it is read
    let caller = work_queue_caller_of(host.base(), "owner").unwrap();

    // Then it is the owner of its project's queue
    assert_eq!(
        caller,
        Some((
            WorkQueueCaller {
                session_id: "owner".to_string(),
                project_id: A_PROJECT.to_string(),
            },
            WorkQueueAccess::Owner
        ))
    );
}

#[test]
fn an_orchestrator_role_session_owning_no_queue_has_no_access() {
    // Given a project-home session that owns no queue (access never reads the role)
    let host = a_stack_host();
    let dir = host.session("main", Changeset::default());
    let mut meta = tddy_core::read_session_metadata(&dir).unwrap();
    meta.session_role = Some("project-home".to_string());
    tddy_core::session_metadata::write_session_metadata(&dir, &meta).unwrap();

    // When it is read
    let caller = work_queue_caller_of(host.base(), "main").unwrap();

    // Then it has none
    assert_eq!(caller, None);
}

#[test]
fn a_session_with_no_record_is_refused_rather_than_read_as_ordinary() {
    // Given a session id with nothing on disk
    let host = a_stack_host();

    // When it is read
    let refusal =
        work_queue_caller_of(host.base(), "ghost").expect_err("its access cannot be known");

    // Then it is refused as permission denied, naming the session
    assert_eq!(refusal.code(), tddy_rpc::Code::PermissionDenied);
    assert!(refusal.message().contains("ghost"), "{}", refusal.message());
}
