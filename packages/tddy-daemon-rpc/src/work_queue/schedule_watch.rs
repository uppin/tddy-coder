//! The daemon drives the schedule runner (`#workqueue` 14/14): an armed schedule fires by itself,
//! once, when its last condition turns met — without anyone watching the queue.
//!
//! One watcher per queue with schedules, subscribed to the GitHub inbox's events for the queue's
//! repository (PR, checks, comments, pushes) — a subscription, never a poll, so the inbox's own quota
//! discipline holds. Each event re-evaluates the queue's schedules and hands the met ones to a
//! [`ScheduleRunner`](tddy_work_queue::schedule_runner::ScheduleRunner); a change to the queue itself
//! (a schedule armed or disarmed) calls [`after_arming`], which evaluates at once and makes sure a
//! watcher is running. A schedule is taken out of the store in the write that fires it, so however many
//! events or watchers see it met, it fires once. The watcher ends when nothing is armed.
//!
//! The watcher acts as the person who armed (the inbox registers the repository to be read as them).
//! It holds their [`Armer`] identity, not a token: on a daemon that signs tokens it signs a fresh
//! short-lived one at every use, so a watcher outlives the arming call's token. At startup
//! [`reattach_armed`] starts a watcher on every queue that still has schedules, as the GitHub user
//! `users[]` maps the queue's OS user to — never an identity read from the queue file.

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};

use tddy_core::work_queue_store::WorkQueueStore;
use tddy_github_inbox::EventKind;
use tddy_service::proto::work_queue::WorkQueueAddress;
use tddy_work_queue::lineage::trunk_branch_of;
use tddy_worktree_service::pull_request_view::SessionPullRequests;

use super::land::{fire_met_in_background, DaemonFireSchedule, GitEnvSource};
use super::schedules::ScheduleCtx;

/// Whose credentials a watcher reads GitHub and fires with.
#[derive(Clone)]
pub(crate) enum Armer {
    /// The arming call's own token: good for as long as it lives.
    Caller(String),
    /// A GitHub user this daemon signs a fresh short-lived token for at every use.
    Signed {
        signer: tddy_github::SessionTokenSigner,
        user: tddy_github::GitHubUser,
    },
}

impl Armer {
    /// `login` as this daemon signs for it.
    pub(crate) fn signed(signer: tddy_github::SessionTokenSigner, login: &str) -> Self {
        Self::Signed {
            signer,
            user: tddy_github::GitHubUser {
                id: 0,
                login: login.to_string(),
                avatar_url: String::new(),
                name: login.to_string(),
            },
        }
    }

    /// Vault access as this armer: through the caller's token, or — signed — straight through their
    /// vault, which needs no token at all.
    pub(crate) fn identity(
        &self,
        vaults: Option<Arc<tddy_credentials::SessionVaults>>,
        resolver: tddy_daemon_kernel::SessionUserResolver,
    ) -> tddy_session_lifecycle::connection_service::session_acting_identity::SessionAccountAccess
    {
        use tddy_session_lifecycle::connection_service::session_acting_identity::SessionAccountAccess;
        match self {
            Self::Caller(token) => SessionAccountAccess::new(vaults, resolver, token),
            Self::Signed { user, .. } => SessionAccountAccess::for_subject(vaults, &user.login),
        }
    }

    /// A token to present now.
    pub(crate) fn token(&self) -> String {
        match self {
            Self::Caller(token) => token.clone(),
            Self::Signed { signer, user } => signer.mint_access(user),
        }
    }
}

/// Everything a watcher needs, owned, for one queue on this host.
pub(crate) struct WatchInputs {
    pub source: GitEnvSource,
    pub pull_requests: Arc<dyn SessionPullRequests>,
    pub store: WorkQueueStore,
    pub sessions_base: PathBuf,
    pub os_user: String,
    pub address: WorkQueueAddress,
    pub armer: Armer,
}

fn running() -> &'static Mutex<HashSet<String>> {
    static RUNNING: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();
    RUNNING.get_or_init(Default::default)
}

impl WatchInputs {
    fn key(&self) -> String {
        format!(
            "{}|{}|{}",
            self.sessions_base.display(),
            self.address.project_id,
            self.address.queue_id
        )
    }

    fn trunk(&self) -> Result<String, String> {
        tddy_projects::project_storage::effective_integration_base_ref_for_project(
            &self.store.projects_dir(),
            &self.address.project_id,
        )
        .map(|base| trunk_branch_of(&base).to_string())
        .map_err(|e| format!("the project's trunk could not be resolved: {e:#}"))
    }

    fn firing(&self) -> DaemonFireSchedule {
        DaemonFireSchedule::through_engines(
            self.source.clone(),
            self.store.clone(),
            self.sessions_base.clone(),
            self.os_user.clone(),
            self.address.clone(),
        )
    }

    /// Evaluate the queue's schedules now and fire the met ones. `false` when none is armed.
    async fn evaluate_and_fire(&self) -> bool {
        let Ok(record) = self
            .store
            .read(&self.address.project_id, &self.address.queue_id)
        else {
            return false;
        };
        if record.stack.schedules.is_empty() {
            return false;
        }
        let trunk = match self.trunk() {
            Ok(trunk) => trunk,
            Err(why) => {
                log::warn!(
                    "schedules of {} not evaluated: {why}",
                    self.address.queue_id
                );
                return true;
            }
        };
        let session_token = self.armer.token();
        let (views, _) = ScheduleCtx {
            store: &self.store,
            pull_requests: self.pull_requests.as_ref(),
            os_user: &self.os_user,
            sessions_base: &self.sessions_base,
            project_id: &self.address.project_id,
            queue_id: &self.address.queue_id,
            caller_session_id: &self.address.queue_id,
            session_token: &session_token,
            trunk: &trunk,
            armed_by: &self.os_user,
        }
        .evaluate_views(&record.stack)
        .await;
        fire_met_in_background(self.firing(), views);
        true
    }
}

/// A schedule was armed or removed: evaluate at once, and keep a watcher on the queue's repository
/// while anything is armed.
pub(crate) async fn after_arming(inputs: WatchInputs) {
    if inputs.evaluate_and_fire().await {
        ensure_watcher(inputs);
    }
}

fn ensure_watcher(inputs: WatchInputs) {
    let key = inputs.key();
    if !running()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .insert(key.clone())
    {
        return;
    }
    tokio::spawn(async move {
        watch(&inputs, &key).await;
        running()
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&key);
    });
}

async fn watch(inputs: &WatchInputs, key: &str) {
    let site = match inputs.pull_requests.site_for_session(
        &inputs.os_user,
        &inputs.address.queue_id,
        &inputs.address.project_id,
        &inputs.armer.token(),
    ) {
        Ok(site) => site,
        Err(status) => {
            log::warn!(
                "schedules of {} are not watched: {}",
                inputs.address.queue_id,
                status.message()
            );
            return;
        }
    };
    // Subscribed before the first evaluation, so a change landing during it is not missed.
    let mut events = site.inbox.subscribe(
        &site.repo,
        &[
            EventKind::Pr,
            EventKind::Checks,
            EventKind::Comments,
            EventKind::Push,
        ],
    );
    loop {
        if !inputs.evaluate_and_fire().await {
            // Nothing armed: stop, unless a schedule was armed while this decided (its arming found
            // this watcher running and started none).
            running()
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .remove(key);
            if !inputs.evaluate_and_fire().await {
                return;
            }
            running()
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .insert(key.to_string());
        }
        if events.next().await.is_none() {
            return;
        }
    }
}

impl super::WorkQueueRpcHandler {
    /// At daemon startup: start a watcher on every queue of this host that still has armed schedules,
    /// so an arming survives a restart. Each acts as the GitHub user `users[]` maps the schedule's
    /// `armed_by` OS user to; a schedule whose arming user is no longer mapped is left unwatched and
    /// logged. A daemon that signs no tokens watches nothing. Answers how many queues are watched.
    pub async fn reattach_armed_schedules(&self) -> usize {
        let Some(signer) = self.session_token_signer.clone() else {
            log::info!("armed schedules are not re-attached: this daemon signs no session tokens");
            return 0;
        };
        let users = self.config.users.snapshot();
        let mut watched = 0;
        for (os_user, sessions_base) in self.sessions_bases_of(&users) {
            let store = self.stack_store(&sessions_base);
            let projects = tddy_projects::project_storage::read_projects(&store.projects_dir())
                .unwrap_or_default();
            for project in projects {
                let Ok(records) = store.list(&project.project_id) else {
                    continue;
                };
                for record in records {
                    let Some(armed_by) = record.stack.schedules.first().map(|s| s.armed_by.clone())
                    else {
                        continue;
                    };
                    if armed_by != os_user {
                        continue;
                    }
                    let Some(mapping) = users.iter().find(|m| m.os_user == armed_by) else {
                        continue;
                    };
                    let armer = Armer::signed(signer.clone(), &mapping.github_user);
                    after_arming(WatchInputs {
                        source: self.git_env_source_for(&armer),
                        pull_requests: Arc::clone(&self.pull_requests),
                        store: store.clone(),
                        sessions_base: sessions_base.clone(),
                        os_user: os_user.clone(),
                        address: WorkQueueAddress {
                            project_id: project.project_id.clone(),
                            queue_id: record.queue_id.clone(),
                            daemon_instance_id: self.local_instance_id(),
                        },
                        armer,
                    })
                    .await;
                    watched += 1;
                }
            }
        }
        if watched > 0 {
            log::info!("re-attached the armed schedules of {watched} work queue(s)");
        }
        watched
    }

    /// Each mapped OS user once, with the sessions base its queues are under.
    fn sessions_bases_of(
        &self,
        users: &[tddy_daemon_kernel::config::UserMapping],
    ) -> Vec<(String, PathBuf)> {
        let mut seen = HashSet::new();
        users
            .iter()
            .filter(|mapping| seen.insert(mapping.os_user.clone()))
            .filter_map(|mapping| {
                let base = self.sessions_base(&mapping.os_user).ok()?;
                Some((mapping.os_user.clone(), base))
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;
    use tddy_changeset::work_queue_fixtures::{a_stack_host, A_PROJECT};
    use tddy_core::changeset::{WorkItem, WorkQueue};
    use tddy_session_lifecycle::test_util::{signing_test_host_as, TEST_USER};
    use tddy_work_queue::schedule::{Schedule, ScheduleKind};

    use super::*;

    fn a_queue_armed_by(os_user: &str) -> WorkQueue {
        WorkQueue {
            version: 1,
            nodes: vec![WorkItem {
                node_id: "n1".to_string(),
                ..WorkItem::default()
            }],
            schedules: vec![Schedule {
                item_id: "n1".to_string(),
                kind: ScheduleKind::Merge { skip_review: false },
                armed_at: "2026-11-23T10:00:00Z".to_string(),
                armed_by: os_user.to_string(),
            }],
        }
    }

    fn a_registered_project(projects_dir: &std::path::Path) {
        tddy_projects::project_storage::add_project(
            projects_dir,
            tddy_projects::project_storage::ProjectData {
                project_id: A_PROJECT.to_string(),
                name: "alpha".to_string(),
                git_url: "https://github.com/acme/alpha.git".to_string(),
                main_repo_path: "/nonexistent/alpha".to_string(),
                main_branch_ref: None,
                remote_name: None,
                host_repo_paths: std::collections::HashMap::new(),
                accounts: Vec::new(),
                local_gh: false,
                home_session_id: None,
                orchestrator_defaults: None,
                lifecycle: Default::default(),
            },
        )
        .expect("register the project");
    }

    #[tokio::test]
    async fn after_a_restart_queues_armed_by_a_mapped_user_are_watched_again_as_that_user() {
        // Given a host whose users[] maps the test user to `dev`, with one queue armed by `dev`
        // and one armed by an OS user nobody maps
        let stacks = a_stack_host();
        a_registered_project(&stacks.projects_dir());
        stacks.orchestrator("owner", a_queue_armed_by("dev"));
        stacks.orchestrator("stranger", a_queue_armed_by("nobody-maps-me"));
        let (host, _) = signing_test_host_as(stacks.base().to_path_buf(), "dev");
        let restarted = super::super::WorkQueueRpcHandler::from_host(&host);

        // When the restarted daemon re-attaches the armed schedules
        let watched = restarted.reattach_armed_schedules().await;

        // Then only the mapped user's queue is watched, and the watcher's identity is the mapped
        // GitHub user, signed afresh: each token it presents resolves to them on this daemon
        assert_eq!(watched, 1);
        let armer = Armer::signed(
            host.session_token_signer().expect("the host signs tokens"),
            TEST_USER,
        );
        let resolve = host.user_resolver();
        assert_eq!(
            (resolve(&armer.token()), resolve(&armer.token())),
            (Some(TEST_USER.to_string()), Some(TEST_USER.to_string()))
        );
    }
}
