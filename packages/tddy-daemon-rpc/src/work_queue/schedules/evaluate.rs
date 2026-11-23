//! Arming, disarming and evaluating a queue's schedules against the facts.

use super::*;
use std::collections::HashMap;

use tddy_core::changeset::WorkQueue;
use tddy_github_inbox::InboxError;
use tddy_service::proto::work_queue::ScheduleStatusView;
use tddy_work_queue::commit_flow::InboxEntry;
use tddy_work_queue::inbox_reads::inbox_error_reason;
use tddy_work_queue::lineage::{self, LineEndpoint};
use tddy_work_queue::schedule::{
    self, review_verdict, ChecksVerdict, ReviewVerdict, Schedule, ScheduleError, ScheduleKind,
    ScheduleSnapshot, WorktreeVerdict,
};
use tddy_work_queue::waves;
use tddy_worktree_service::pull_request_view::PullRequestSite;

use super::super::commit_flow::{flow_site, read_flow, FlowInputs};

impl ScheduleCtx<'_> {
    fn queue(&self) -> Result<WorkQueue, ScheduleOpError> {
        self.store
            .read(self.project_id, self.queue_id)
            .map(|record| record.stack)
            .map_err(store_failure)
    }

    fn update(
        &self,
        edit: impl FnOnce(&mut WorkQueue) -> Result<(), ScheduleError>,
    ) -> Result<WorkQueue, ScheduleOpError> {
        let mut refused = None;
        let record = self
            .store
            .update(self.project_id, self.queue_id, |queue| {
                let mut edited = queue.clone();
                match edit(&mut edited) {
                    Ok(()) => *queue = edited,
                    Err(error) => refused = Some(error),
                }
            })
            .map_err(store_failure)?;
        match refused {
            Some(error) => Err(error.into()),
            None => Ok(record.stack),
        }
    }

    fn site(&self) -> Result<PullRequestSite, String> {
        self.pull_requests
            .site_for_session(
                self.os_user,
                self.caller_session_id,
                self.project_id,
                self.session_token,
            )
            .map_err(|status| status.message().to_string())
    }

    /// Arm `kind` on `item_id`, answering the schedule as first evaluated.
    pub(crate) async fn arm_item(
        &self,
        item_id: &str,
        kind: &str,
        skip_review: bool,
    ) -> Result<ScheduleStatusView, ScheduleOpError> {
        let kind = kind_of(kind, skip_review)?;
        let (armed_at, armed_by) = (now_iso(), self.armed_by.to_string());
        let item_id = item_id.trim().to_string();
        let queue = self.update(|queue| {
            schedule::arm(queue, &item_id, kind, &armed_at, &armed_by).map(|_| ())
        })?;
        let (views, _) = self.evaluate(&queue).await;
        views
            .into_iter()
            .find(|v| v.item_id == item_id)
            .ok_or_else(|| ScheduleOpError::Failed("the armed schedule was not stored".to_string()))
    }

    /// Arm a merge on every item of `wave` that has an open PR, bottom-up.
    pub(crate) async fn arm_wave(
        &self,
        wave: u32,
        skip_review: bool,
    ) -> Result<(Vec<ScheduleStatusView>, String), ScheduleOpError> {
        let queue = self.queue()?;
        let items: Vec<String> = queue.nodes.iter().map(|n| n.node_id.clone()).collect();
        // Wave membership is structural: what has already landed does not change it.
        let derived = waves::queue_waves(&items, &lineage::dependencies_of(&queue), &[])
            .map_err(|e| ScheduleOpError::Failed(e.to_string()))?;
        let in_wave = usize::try_from(wave)
            .ok()
            .and_then(|w| w.checked_sub(1))
            .and_then(|index| derived.waves.get(index))
            .ok_or_else(|| {
                ScheduleOpError::NotFound(format!(
                    "wave {wave} does not exist (the queue has {})",
                    derived.waves.len()
                ))
            })?;
        let bottom_up: Vec<String> = derived
            .derived_line
            .iter()
            .filter(|id| in_wave.contains(id))
            .cloned()
            .collect();
        let site = self.site().map_err(|reason| {
            ScheduleOpError::Failed(format!(
                "which items have an open pull request could not be read: {reason}"
            ))
        })?;
        let mut open: HashMap<String, bool> = HashMap::new();
        for id in &bottom_up {
            let item = queue.node(id).expect("a wave holds the queue's own items");
            let has_open = item_pr(&site, item)
                .await
                .map_err(|reason| {
                    ScheduleOpError::Failed(format!(
                        "the pull request of {id} could not be read: {reason}"
                    ))
                })?
                .is_some_and(|pr| is_open(&pr));
            open.insert(id.clone(), has_open);
        }
        let (armed_at, armed_by) = (now_iso(), self.armed_by.to_string());
        let queue = self.update(|queue| {
            schedule::arm_wave(
                queue,
                &bottom_up,
                |id| open.get(id).copied().unwrap_or(false),
                skip_review,
                &armed_at,
                &armed_by,
            )
            .map(|_| ())
        })?;
        let (views, reason) = self.evaluate(&queue).await;
        Ok((views, reason))
    }

    pub(crate) async fn disarm(
        &self,
        item_id: &str,
    ) -> Result<(Vec<ScheduleStatusView>, String), ScheduleOpError> {
        let item_id = item_id.trim().to_string();
        let queue = self.update(|queue| schedule::disarm(queue, &item_id))?;
        Ok(self.evaluate(&queue).await)
    }

    pub(crate) async fn list(&self) -> Result<(Vec<ScheduleStatusView>, String), ScheduleOpError> {
        let queue = self.queue()?;
        Ok(self.evaluate(&queue).await)
    }

    /// Every armed schedule of `queue`, evaluated now, in the order armed; and why GitHub or the
    /// checkout could not be read at all (empty when they could).
    pub(crate) async fn evaluate(&self, queue: &WorkQueue) -> (Vec<ScheduleStatusView>, String) {
        let (views, reason) = self.evaluate_views(queue).await;
        (views.into_iter().map(status_view).collect(), reason)
    }

    /// [`Self::evaluate`] before it is put on the wire: what the schedule runner is fed.
    pub(crate) async fn evaluate_views(
        &self,
        queue: &WorkQueue,
    ) -> (Vec<schedule::ScheduleView>, String) {
        if queue.schedules.is_empty() {
            return (Vec::new(), String::new());
        }
        let site = self.site();
        let line = match &site {
            Ok(site) => {
                lineage::queue_graph(&site.inbox, &site.repo, queue, self.trunk)
                    .await
                    .line
            }
            Err(_) => lineage::planned_line_edges(queue, self.trunk),
        };
        let flow = flow_site(FlowInputs {
            pull_requests: self.pull_requests,
            os_user: self.os_user,
            sessions_base: self.sessions_base,
            queue_id: self.queue_id,
            caller_session_id: self.caller_session_id,
            project_id: self.project_id,
            session_token: self.session_token,
            queue,
            trunk: self.trunk,
        })
        .await;
        let wanted: Vec<String> = queue.schedules.iter().map(|s| s.item_id.clone()).collect();
        let inboxes: Result<HashMap<String, Result<Vec<InboxEntry>, String>>, String> = match &flow
        {
            Ok(flow) => {
                read_flow(flow, move |f, items| {
                    Ok(wanted
                        .into_iter()
                        .map(|id| {
                            let entries = f.inbox(items, &id).map_err(|e| e.to_string());
                            (id, entries)
                        })
                        .collect())
                })
                .await
            }
            Err(reason) => Err(reason.clone()),
        };
        let mut views = Vec::new();
        for armed in &queue.schedules {
            let facts = self
                .snapshot_of(queue, armed, &line, site.as_ref(), inboxes.as_ref())
                .await;
            views.push(match facts {
                Ok(snapshot) => schedule::evaluate(armed, &snapshot),
                Err(reason) => unreadable(armed.clone(), reason),
            });
        }
        let reason = match (&site, &flow) {
            (Err(reason), _) => reason.clone(),
            (_, Err(reason)) => reason.clone(),
            _ => String::new(),
        };
        (views, reason)
    }

    /// What holds each item's merge up, judged as an armed merge would be (`#workqueue` 14/14):
    /// `item -> (condition, detail)` for every item with a branch whose merge conditions are not all
    /// met. Nothing is armed or stored — the schedules are evaluated and dropped.
    pub(crate) async fn merge_holdups(
        &self,
        queue: &WorkQueue,
    ) -> HashMap<String, (String, String)> {
        let mut judged = queue.clone();
        judged.schedules = queue
            .nodes
            .iter()
            .filter(|item| item.branch.is_some())
            .map(|item| Schedule {
                item_id: item.node_id.clone(),
                kind: ScheduleKind::Merge { skip_review: false },
                armed_at: String::new(),
                armed_by: self.armed_by.to_string(),
            })
            .collect();
        let (views, _) = self.evaluate(&judged).await;
        views
            .into_iter()
            .filter(|view| view.status != "met")
            .filter_map(|view| {
                let held = ["blocked", "waiting"]
                    .into_iter()
                    .find_map(|state| view.conditions.iter().find(|c| c.state == state).cloned())?;
                Some((view.item_id, (held.name, held.detail)))
            })
            .collect()
    }

    /// The facts of one armed schedule; the first that cannot be read is the error.
    async fn snapshot_of(
        &self,
        queue: &WorkQueue,
        armed: &Schedule,
        line: &[(String, LineEndpoint)],
        site: Result<&PullRequestSite, &String>,
        inboxes: Result<&HashMap<String, Result<Vec<InboxEntry>, String>>, &String>,
    ) -> Result<ScheduleSnapshot, String> {
        let item = queue
            .node(&armed.item_id)
            .ok_or_else(|| format!("item '{}' no longer exists", armed.item_id))?;
        let base = line
            .iter()
            .find(|(id, _)| *id == armed.item_id)
            .map(|(_, endpoint)| endpoint)
            .ok_or_else(|| "the item has no readable line parent".to_string())?;
        let entries = inboxes
            .map_err(|reason| format!("the inbox could not be read: {reason}"))?
            .get(&armed.item_id)
            .ok_or_else(|| "the inbox was not read for this item".to_string())?
            .as_ref()
            .map_err(|reason| format!("the inbox could not be read: {reason}"))?;
        let (inbox_clean, inbox_conflicting) = inbox_counts(entries);
        let site = site.map_err(|reason| format!("GitHub could not be read: {reason}"))?;

        let merge = matches!(armed.kind, ScheduleKind::Merge { .. });
        let (own_checks, review) = if merge {
            let pr = item_pr(site, item)
                .await?
                .ok_or_else(|| "the item has no pull request".to_string())?;
            if !is_open(&pr) {
                return Err("the item's pull request is no longer open".to_string());
            }
            let checks = checks_of_pr(site, &pr).await?;
            let conversation = site
                .inbox
                .conversation(&site.repo, pr.number)
                .await
                .map_err(|e: InboxError| inbox_error_reason(&e))?;
            let review = review_verdict(
                conversation
                    .value
                    .reviews
                    .iter()
                    .map(|r| (r.author.as_str(), r.state.as_str())),
            );
            (checks, review)
        } else {
            (ChecksVerdict::NotOurs, ReviewVerdict::Pending)
        };
        let base_checks = match (merge, base) {
            (false, LineEndpoint::Item(base_id)) => {
                let base_item = queue
                    .node(base_id)
                    .ok_or_else(|| format!("the base item '{base_id}' no longer exists"))?;
                match item_pr(site, base_item).await? {
                    Some(pr) => checks_of_pr(site, &pr).await?,
                    None => ChecksVerdict::NotOurs,
                }
            }
            _ => ChecksVerdict::NotOurs,
        };
        let worktree = if merge {
            WorktreeVerdict::Clean
        } else {
            worktree_of(item, self.sessions_base)?
        };
        Ok(ScheduleSnapshot {
            base_is_trunk: matches!(base, LineEndpoint::Trunk(_)),
            inbox_clean,
            inbox_conflicting,
            own_checks,
            base_checks,
            review,
            worktree,
        })
    }
}
