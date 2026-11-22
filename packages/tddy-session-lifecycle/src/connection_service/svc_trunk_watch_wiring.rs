//! The daemon's watch over each project's remote trunk (`#orchestra` 5/9): the host's side of
//! [`tddy_session_activity::trunk_watch::TrunkWatcher`].
//!
//! An orchestrator-role session is subscribed when it starts or resumes
//! ([`DaemonSessionHost::subscribe_orchestrator_to_trunk`]) and falls away when the host stops
//! holding a live agent for it — the watcher's liveness sweep drops it. One task
//! ([`DaemonSessionHost::spawn_trunk_watch`]) drives the watcher on the deadline it returns.
//!
//! A project's trunk is read through the daemon's GitHub inbox, by the host's `gh` as the session's OS user when the project is
//! **Local gh**, and through the REST API with the project's GitHub account token otherwise.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, Instant};

use async_trait::async_trait;
use tddy_daemon_kernel::live_sessions::LiveSessions;
use tddy_github::conditional::{
    ConditionalRead, GhCliConditionalRead, RestConditionalRead, GITHUB_API_BASE,
};
use tddy_github::pr_api::owner_repo_from_remote_url;
use tddy_github::trunk_feed::{
    GhApi, GhApiOutput, RepoSlug, TrunkFeed, TrunkPoller, TrunkReadError,
};
use tddy_github_inbox::{GithubInbox, InboxError, InboxTrunkFeed, RepoReaders};
use tddy_session_activity::trunk_watch::{
    SubscriberLiveness, TrunkSource, TrunkSubscription, TrunkWatcher,
};

use super::session_acting_identity::SessionAccountAccess;
use super::DaemonSessionHost;

/// The longest the driver sleeps between steps: how soon a new subscriber's first poll is made,
/// and how soon a stopped one is dropped.
const LONGEST_STEP_WAIT: Duration = Duration::from_secs(5);
/// The shortest, so a poller due now is not spun on.
const SHORTEST_STEP_WAIT: Duration = Duration::from_secs(1);
/// How long a just-started session counts as live before the host's own snapshot must show it.
const START_GRACE: Duration = Duration::from_secs(60);

/// What the host keeps to watch trunks: the watcher, the sessions' liveness, and the inbox the
/// trunk is read through.
pub(crate) struct TrunkWatching {
    watcher: Arc<TrunkWatcher>,
    liveness: Arc<HostLiveness>,
    inbox: Arc<GithubInbox>,
}

/// The transport that reads each repository, registered as projects are subscribed. A repository
/// is read with the credential of the project that most recently subscribed to it; the token
/// itself is resolved on every read, so a rotated one is picked up without a re-subscribe.
#[derive(Default)]
struct HostRepoReaders {
    readers: Mutex<HashMap<RepoSlug, Arc<dyn ConditionalRead>>>,
}

impl HostRepoReaders {
    fn register(&self, repo: &RepoSlug, reader: Arc<dyn ConditionalRead>) {
        self.readers
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(repo.clone(), reader);
    }
}

impl RepoReaders for HostRepoReaders {
    fn reader_for(&self, repo: &RepoSlug) -> Result<Arc<dyn ConditionalRead>, InboxError> {
        self.readers
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(repo)
            .cloned()
            .ok_or_else(|| {
                InboxError::Unavailable(format!(
                    "no project has registered a credential for {}/{}",
                    repo.owner, repo.name
                ))
            })
    }
}

/// Which subscribed sessions are live: the host's snapshot, taken before each step, and the
/// sessions started since, which the snapshot may not hold yet.
#[derive(Default)]
struct HostLiveness {
    snapshot: RwLock<LiveSessions>,
    just_started: Mutex<HashMap<String, Instant>>,
}

impl HostLiveness {
    /// Forget the starts whose grace has passed: from then on the snapshot alone decides.
    fn forget_expired_starts(&self) {
        self.just_started
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .retain(|_, at| at.elapsed() < START_GRACE);
    }
}

impl SubscriberLiveness for HostLiveness {
    fn is_live(&self, session_id: &str) -> bool {
        let in_snapshot = self
            .snapshot
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .contains(session_id);
        in_snapshot
            || self
                .just_started
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .get(session_id)
                .is_some_and(|at| at.elapsed() < START_GRACE)
    }
}

/// `gh api -i` as an OS user, for [`GhCliConditionalRead`].
struct HostGh {
    cli: tddy_host_service::GithubCli,
    os_user: String,
    hostname: String,
}

#[async_trait]
impl GhApi for HostGh {
    async fn api_with_headers(
        &self,
        path: &str,
        if_none_match: Option<&str>,
    ) -> Result<GhApiOutput, TrunkReadError> {
        let (cli, os_user, hostname) = (
            self.cli.clone(),
            self.os_user.clone(),
            self.hostname.clone(),
        );
        let (path, etag) = (path.to_string(), if_none_match.map(str::to_string));
        let (stdout, exit_code) = tokio::task::spawn_blocking(move || {
            cli.api_with_headers(&os_user, &hostname, &path, etag.as_deref())
        })
        .await
        .map_err(|e| TrunkReadError(e.to_string()))?
        .map_err(|refusal| TrunkReadError(format!("{refusal:?}")))?;
        Ok(GhApiOutput { stdout, exit_code })
    }
}

/// Builds a project's poller from its row on this host.
struct HostTrunkSource {
    host: DaemonSessionHost,
    inbox: Arc<GithubInbox>,
    readers: Arc<HostRepoReaders>,
}

/// The GitHub token `subject`'s vault holds for the project's account, read from the project's
/// row as it is now.
fn project_account_token(
    host: &DaemonSessionHost,
    subscription: &TrunkSubscription,
    subject: &str,
) -> Result<String, String> {
    let row = host
        .project_row(
            &subscription.os_user,
            &subscription.session_id,
            &subscription.project_id,
        )
        .ok_or_else(|| "the project's row cannot be read".to_string())?;
    SessionAccountAccess::for_subject(host.credential_vaults(), subject)
        .acting_identity(&row.accounts)
        .map(|identity| identity.token)
        .map_err(|refusal| refusal.to_string())
}

impl HostTrunkSource {
    /// Register how `repo` is read for this project, and answer the feed that reads it through the
    /// inbox.
    fn feed_for(
        &self,
        row: &tddy_projects::project_storage::ProjectData,
        subscription: &TrunkSubscription,
        repo: &RepoSlug,
    ) -> Result<Arc<dyn TrunkFeed>, String> {
        let reader: Arc<dyn ConditionalRead> = if row.local_gh {
            let hostname = tddy_accounts::local_gh::github_hostname_of(&row.git_url)
                .map_err(|e| e.to_string())?;
            Arc::new(GhCliConditionalRead::new(Arc::new(HostGh {
                cli: self.host.github_cli.clone(),
                os_user: subscription.os_user.clone(),
                hostname,
            })))
        } else {
            let subject = subscription
                .github_subject
                .clone()
                .ok_or_else(|| "the session's GitHub user is unknown".to_string())?;
            // Resolved now, so a project with no usable account is left unwatched at once, and again
            // on every read, so a token rotated in the vault is picked up without a re-subscribe.
            project_account_token(&self.host, subscription, &subject)?;
            let (host, subscription) = (self.host.clone(), subscription.clone());
            Arc::new(RestConditionalRead::new(
                GITHUB_API_BASE,
                Arc::new(move || {
                    project_account_token(&host, &subscription, &subject).map_err(TrunkReadError)
                }),
            ))
        };
        self.readers.register(repo, reader);
        Ok(Arc::new(InboxTrunkFeed::new(Arc::clone(&self.inbox))))
    }
}

impl TrunkSource for HostTrunkSource {
    fn poller_for(
        &self,
        subscription: &TrunkSubscription,
    ) -> Result<(TrunkPoller, String), String> {
        let row = self
            .host
            .project_row(
                &subscription.os_user,
                &subscription.session_id,
                &subscription.project_id,
            )
            .ok_or_else(|| "the project's row cannot be read".to_string())?;
        let repo = owner_repo_from_remote_url(&row.git_url)
            .and_then(|slug| {
                slug.split_once('/')
                    .map(|(owner, name)| (owner.to_string(), name.to_string()))
            })
            .ok_or_else(|| format!("{} names no github.com repository", row.git_url))?;
        let projects_dir = tddy_daemon_kernel::user_paths::projects_path_for_user(
            &subscription.os_user,
            Some(self.host.tddy_data_dir()),
        )
        .ok_or_else(|| "could not resolve the projects path".to_string())?;
        let trunk_ref = tddy_projects::project_storage::effective_integration_base_ref_for_project(
            &projects_dir,
            &subscription.project_id,
        )
        .map_err(|e| e.to_string())?;
        let branch = trunk_ref
            .split_once('/')
            .map_or(trunk_ref.as_str(), |(_, branch)| branch)
            .to_string();
        let repo = RepoSlug {
            owner: repo.0,
            name: repo.1,
        };
        let feed = self.feed_for(&row, subscription, &repo)?;
        Ok((TrunkPoller::new(feed, repo, branch), trunk_ref))
    }
}

impl DaemonSessionHost {
    /// Watch the trunks of the projects whose orchestrators run here, publishing each new commit
    /// on `bus` (builder pattern). Nothing is polled until an orchestrator subscribes and
    /// [`Self::spawn_trunk_watch`] runs.
    pub fn with_trunk_watch(
        mut self,
        bus: Arc<crate::session_notifications::SessionNotificationBus>,
    ) -> Self {
        let liveness = Arc::new(HostLiveness::default());
        let readers = Arc::new(HostRepoReaders::default());
        let inbox = Arc::new(GithubInbox::new(
            Arc::clone(&readers) as Arc<dyn RepoReaders>
        ));
        let source = Arc::new(HostTrunkSource {
            host: self.clone(),
            inbox: Arc::clone(&inbox),
            readers,
        });
        let watcher = Arc::new(TrunkWatcher::new(
            source,
            Arc::clone(&liveness) as Arc<dyn SubscriberLiveness>,
            bus,
        ));
        self.trunk_watching = Some(Arc::new(TrunkWatching {
            watcher,
            liveness,
            inbox,
        }));
        self
    }

    /// The trunk watcher installed with [`Self::with_trunk_watch`], or `None` on a host with none.
    pub fn trunk_watcher(&self) -> Option<Arc<TrunkWatcher>> {
        self.trunk_watching
            .as_ref()
            .map(|watching| Arc::clone(&watching.watcher))
    }

    /// Subscribe a running orchestrator-role session to its project's trunk. A trunk that cannot be
    /// watched (no GitHub remote, no identity) is logged and the session runs on without it.
    pub(crate) fn subscribe_orchestrator_to_trunk(
        &self,
        os_user: &str,
        project_id: &str,
        session_id: &str,
        session_token: &str,
    ) {
        let Some(watching) = &self.trunk_watching else {
            return;
        };
        watching
            .liveness
            .just_started
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(session_id.to_string(), Instant::now());
        if let Err(reason) = watching.watcher.subscribe(TrunkSubscription {
            project_id: project_id.to_string(),
            os_user: os_user.to_string(),
            session_id: session_id.to_string(),
            github_subject: (self.user_resolver())(session_token),
        }) {
            log::warn!(
                target: "tddy_daemon::trunk_watch",
                "session {session_id} does not watch its project's trunk: {reason}"
            );
        }
    }

    /// The task that drives the trunk watcher on the deadline it returns, or `None` on a host
    /// with no watcher.
    pub fn spawn_trunk_watch(&self) -> Option<tokio::task::JoinHandle<()>> {
        let watching = Arc::clone(self.trunk_watching.as_ref()?);
        let host = self.clone();
        Some(tokio::spawn(async move {
            loop {
                *watching
                    .liveness
                    .snapshot
                    .write()
                    .unwrap_or_else(|e| e.into_inner()) = host.live_sessions().await;
                watching.liveness.forget_expired_starts();
                let now = tddy_daemon_kernel::now_unix_ms();
                watching.inbox.step(now).await;
                let wait = watching
                    .watcher
                    .step(now)
                    .await
                    .map_or(LONGEST_STEP_WAIT, |due| {
                        Duration::from_millis(due.saturating_sub(tddy_daemon_kernel::now_unix_ms()))
                    })
                    .clamp(SHORTEST_STEP_WAIT, LONGEST_STEP_WAIT);
                tokio::time::sleep(wait).await;
            }
        }))
    }
}
