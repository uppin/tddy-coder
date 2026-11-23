//! Bodies of the `WorkQueueHandler` methods that `ports.rs` delegates here, as inherent methods
//! of [`WorkQueueRpcHandler`].
//!
//! TODO: these are a verbatim hand move out of the trait impl in `ports.rs` — the restructure
//! engine cannot cut a trait impl (no extract-into-function after a `forward_if_addressed` guard).

use super::project_stacks::{require_a_stack, work_queue_target, WorkQueueTarget};
use super::WorkQueueRpcHandler;
use tddy_core::session_lifecycle::{unified_session_dir_path, validate_session_id_segment};
use tddy_rpc::{Request, Response, Status};
use tddy_service::proto::work_queue::{
    AddWorkItemRequest, AddWorkItemResponse, LinkWorkItemRequest, LinkWorkItemResponse,
};
use tddy_session_lifecycle::connection_service::resolve_os_user;
use tddy_session_lifecycle::session_list_enrichment;
use tddy_session_lifecycle::user_sessions_path::sessions_base_for_user;

impl WorkQueueRpcHandler {
    pub(super) async fn add_work_item_rpc(
        &self,
        request: Request<AddWorkItemRequest>,
    ) -> Result<Response<AddWorkItemResponse>, Status> {
        let req = request.into_inner();
        log::debug!(
            "AddWorkItem: session_id={} title={:?}",
            req.session_id.trim(),
            req.title
        );
        let (github_user, os_user_owned) = self.authenticated_user(&req.session_token)?;
        let target = work_queue_target(req.stack.as_ref(), &req.session_id)?;
        if let Some(answered) = self
            .forward_if_addressed::<_, AddWorkItemResponse>("AddWorkItem", &target, &req)
            .await?
        {
            return Ok(Response::new(answered));
        }
        let _ = &github_user;
        if req.title.trim().is_empty() {
            return Err(Status::invalid_argument("title is required"));
        }
        let os_user = &os_user_owned;
        let sessions_base = sessions_base_for_user(os_user, Some(&self.tddy_data_dir))
            .ok_or_else(|| Status::internal("could not resolve sessions path"))?;
        if let WorkQueueTarget::Address(address) = &target {
            let added = self.add_work_item_to_store(&sessions_base, address, &req)?;
            log::info!(
                "AddWorkItem: success stack={} node_id={} title={:?}",
                address.queue_id,
                added.item_id,
                req.title
            );
            return Ok(Response::new(added));
        }
        let WorkQueueTarget::Session(session_id) = &target else {
            unreachable!("work_queue_target yields an address or a session")
        };
        validate_session_id_segment(session_id)
            .map_err(|e| Status::invalid_argument(e.message()))?;
        let session_dir = unified_session_dir_path(&sessions_base, session_id);
        require_a_stack(&session_dir)?;

        let branch_suggestion =
            (!req.branch_suggestion.trim().is_empty()).then(|| req.branch_suggestion.clone());
        let child_recipe = (!req.child_recipe.trim().is_empty()).then(|| req.child_recipe.clone());

        // The appended node, so the response can *name* what this call created. The caller must not
        // infer it by diffing the returned plan: the orchestrator agent appends nodes to the same
        // stack, so a plan can come back holding several nodes the caller has never seen.
        let added = tddy_work_queue::item_ops::add_work_item(
            &session_dir,
            tddy_work_queue::item_ops::AddWorkItemInput {
                title: req.title.clone(),
                description: req.description.clone(),
                branch_suggestion,
                parents: req.parents.clone(),
                child_recipe,
            },
        )
        .map_err(Status::invalid_argument)?;

        // Re-read the just-updated changeset and reuse the same serializer as `ListSessions`
        // enrichment so the response's `work_queue_json` is byte-for-byte the same wire shape
        // `PrStackScreen`'s `parseWorkQueue` already knows how to read.
        let work_queue_json = session_list_enrichment::work_queue_json_of(&session_dir);

        log::info!(
            "AddWorkItem: success session_id={} node_id={} title={:?}",
            req.session_id.trim(),
            added.node_id,
            req.title
        );
        Ok(Response::new(AddWorkItemResponse {
            work_queue_json,
            item_id: added.node_id,
        }))
    }

    pub(super) async fn link_work_item_rpc(
        &self,
        request: Request<LinkWorkItemRequest>,
    ) -> Result<Response<LinkWorkItemResponse>, Status> {
        self.rpc_activity.record();
        let req = request.into_inner();

        if let Some(answered) = self
            .peer_routing
            .rpc_served_by_peer(
                tddy_work_queue::WORK_QUEUE_SERVICE,
                "LinkWorkItem",
                &req.daemon_instance_id,
                &req,
            )
            .await?
        {
            return Ok(Response::new(answered));
        }

        let node_id = req.item_id.trim();
        if node_id.is_empty() {
            return Err(Status::invalid_argument(
                "node_id is required: a planned node is never derived from the branch a spawn creates, which the operator can rename before confirming",
            ));
        }
        let child_session_id = req.child_session_id.trim();
        if child_session_id.is_empty() {
            return Err(Status::invalid_argument(
                "child_session_id is required: a link names the session that materialized the node",
            ));
        }
        // Refused for the same reason `node_id` is. A blank branch leaves `node.branch` untouched —
        // `link_work_item_to_session` takes `None` and writes nothing there — while the RPC
        // answers `Ok`, so the node still refuses every descendant and nothing said so. That is the
        // "reports success, changes nothing" shape D35 exists to remove, and it is reachable
        // whenever `effective_spawn_branch` yields an empty name.
        let branch = req.branch.trim();
        if branch.is_empty() {
            return Err(Status::invalid_argument(
                "branch is required: the branch is what makes the node's descendants spawnable, so a link that records none changes nothing",
            ));
        }

        let os_user = resolve_os_user(&self.config, &self.user_resolver, &req.session_token)?;
        let sessions_base = sessions_base_for_user(&os_user, Some(&self.tddy_data_dir))
            .ok_or_else(|| Status::internal("could not resolve sessions path"))?;
        validate_session_id_segment(&req.orchestrator_session_id)
            .map_err(|e| Status::invalid_argument(e.message()))?;
        let session_dir = unified_session_dir_path(&sessions_base, &req.orchestrator_session_id);
        require_a_stack(&session_dir)?;

        // Checked before the write, because `link_work_item_to_session` leaves an unknown
        // node alone and reports success — which reads to the caller exactly like a link that
        // landed, and is the failure mode this whole path exists to make visible.
        let stack = tddy_core::changeset::read_session_stack(&session_dir)
            .map_err(|e| Status::internal(e.to_string()))?;
        if stack
            .as_ref()
            .is_none_or(|stack| stack.node(node_id).is_none())
        {
            return Err(Status::not_found(format!(
                "planned node '{node_id}' is not in the stack of orchestrator {}",
                req.orchestrator_session_id.trim()
            )));
        }

        tddy_core::changeset::link_work_item_to_session(
            &session_dir,
            node_id,
            child_session_id,
            Some(branch.to_string()),
        )
        .map_err(|e| {
            Status::internal(format!(
                "failed to link stack node '{node_id}' to child session {child_session_id}: {e}"
            ))
        })?;
        log::info!(
            "LinkWorkItem: orchestrator {} node '{node_id}' now records child session {child_session_id} on branch '{branch}'",
            req.orchestrator_session_id.trim()
        );

        // Re-read and serialize through the same path `AddWorkItem` / `RepointWorkItem` /
        // `ReorderWorkItem` answer with, so a caller that renders the plan reuses `parseWorkQueue`
        // and does not re-read what this call already knows.
        Ok(Response::new(LinkWorkItemResponse {
            work_queue_json: session_list_enrichment::work_queue_json_of(&session_dir),
        }))
    }
}
