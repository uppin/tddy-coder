use std::time::Duration;

use tddy_service::proto::connection::ResumeSessionResponse;
use tddy_service::proto::session_files::{
    ContextFileBatchChunk, ContextManifestEntry, ContextManifestRequest,
    ReadContextFileBatchRequest, SessionFilesService,
};

use futures_util::StreamExt;
use tddy_rpc::{Request, Response};
use tddy_worktree_service::stream::MpscResultStream;

use std::path::PathBuf;

use tddy_rpc::Status;

use super::{ConnectionServiceImpl, PeerRoutedSessionFiles};

/// Every frame of one served context read, as the single value the split path below needs.
///
/// Drained rather than sampled: the handler answers a refusal as the call's own error *before* the
/// stream exists, so an error item here can only be one the serving host raised mid-stream — and a
/// manifest that stopped half way must fail this read rather than reach the caller as a project
/// with fewer rules than it has.
///
/// For the *manifest* only. A batch's frames carry the bytes themselves, and collecting those
/// before applying the cap would buffer a dishonest peer's whole overage on this host first —
/// [`ContextRead::files_bounded_as_they_arrive`] is what drains those.
async fn every_frame_of<Frame>(mut frames: MpscResultStream<Frame>) -> Result<Vec<Frame>, Status> {
    let mut collected = Vec::new();
    while let Some(frame) = frames.next().await {
        collected.push(frame?);
    }
    Ok(collected)
}

/// One split session's context read, as its refusals name it: what this host is doing, and which
/// session on which daemon it asked.
///
/// A value rather than a closure so the bounded drain below builds the same message the handler
/// does. The three fields are the whole of it — every refusal on this path says "cannot {verb} a
/// split session without the guidance held beside its codebase", because that is the consequence
/// whichever step failed.
struct ContextRead<'a> {
    /// What the caller is doing — `"start"` or `"resume"`.
    ///
    /// A parameter rather than a word in the message because both paths reach this: a resume that
    /// cannot read its project's guidance re-fetches for the same reason a start does (the
    /// repository moved on while the session was stopped), and an operator reading "cannot start"
    /// about a session that was already running is being told to look in the wrong place.
    verb: &'a str,
    /// The workspace session the guidance is read from, and the daemon holding its checkout.
    codebase_session: &'a str,
    codebase_daemon: &'a str,
}

impl ContextRead<'_> {
    /// Why the split session cannot proceed, preserving the served refusal's own `code`.
    fn refusal(&self, what: &str, status: Status) -> Status {
        let ContextRead {
            verb,
            codebase_session,
            codebase_daemon,
        } = self;
        Status {
            code: status.code(),
            message: format!(
                "cannot {verb} a split session without the guidance held beside its codebase: \
                 {what} from session {codebase_session} on daemon {codebase_daemon} failed: {}",
                status.message()
            ),
        }
    }

    /// Every byte of one batch read, keyed by path — bounded as it arrives, and complete.
    ///
    /// Streamed rather than collected first: the frames carry the bytes, and `size_bytes` on the
    /// manifest is the *peer's* number. A peer that serves more than it advertised is refused on the
    /// frame that crosses the cap, and dropping `frames` there tears its forward down — so the
    /// overage is never held on this host. Collecting first would make the cap a report on an
    /// allocation already made.
    async fn files_bounded_as_they_arrive(
        &self,
        mut frames: MpscResultStream<ContextFileBatchChunk>,
        entries: &[tddy_sandbox::ContextEntry],
        max_bytes: u64,
    ) -> Result<std::collections::BTreeMap<String, Vec<u8>>, Status> {
        let advertised: std::collections::BTreeMap<&str, u64> = entries
            .iter()
            .map(|entry| (entry.rel_path.as_str(), entry.size_bytes))
            .collect();
        let mut files: std::collections::BTreeMap<String, Vec<u8>> =
            std::collections::BTreeMap::new();
        let mut complete: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
        let mut received_bytes: u64 = 0;
        while let Some(frame) = frames.next().await {
            let frame =
                frame.map_err(|status| self.refusal("reading the context files", status))?;
            // The peer names the file each frame belongs to, and a name that was never asked for is
            // a path this host is about to create in the agent's working directory. Refused here
            // rather than filtered, for the reason `context_sync::refuse_unlisted_paths` gives: a
            // peer serving something outside the set is a defect on one side or the other, and
            // dropping it silently leaves the two halves disagreeing about what the agent reads.
            let Some(&size_bytes) = advertised.get(frame.rel_path.as_str()) else {
                return Err(self.refusal(
                    &format!(
                        "the batch carried {:?}, which its manifest never advertised; reading it",
                        frame.rel_path
                    ),
                    Status::permission_denied("context file outside the manifest"),
                ));
            };
            if complete.contains(&frame.rel_path) {
                return Err(self.refusal(
                    &format!(
                        "the batch carried more frames for {:?} after declaring it finished; \
                         reading it",
                        frame.rel_path
                    ),
                    Status::invalid_argument("context file framed twice"),
                ));
            }
            // Bounded as it arrives, because the advertised size is not a promise about how many
            // frames follow it — per file *and* in aggregate, since a batch's whole set is held in
            // memory at once.
            received_bytes = received_bytes.saturating_add(frame.data.len() as u64);
            let bytes = files
                .entry(frame.rel_path.clone())
                // Clamped, because the reservation is a peer-supplied number and an honest one is
                // already under the cap; a dishonest one must not name this host's allocation size.
                .or_insert_with(|| Vec::with_capacity(size_bytes.min(max_bytes) as usize));
            if bytes.len() as u64 + frame.data.len() as u64 > max_bytes
                || received_bytes > max_bytes
            {
                return Err(self.refusal(
                    &format!(
                        "{} streamed more than this host's {max_bytes} byte cap; reading it",
                        frame.rel_path
                    ),
                    Status::invalid_argument("context file over the attachment cap"),
                ));
            }
            bytes.extend_from_slice(&frame.data);
            if frame.end_of_file {
                complete.insert(frame.rel_path);
            }
        }

        // A stream that ended without saying a file was finished is a truncated file, and a
        // truncated `CLAUDE.md` is a wrong `CLAUDE.md`: it drops the project's last rule with
        // nothing downstream able to tell. `end_of_file` is what makes that detectable at all —
        // a byte count alone cannot separate "empty" from "never arrived".
        if let Some(missing) = entries
            .iter()
            .find(|entry| !complete.contains(&entry.rel_path))
        {
            return Err(self.refusal(
                &format!(
                    "the batch never finished {:?}; reading it",
                    missing.rel_path
                ),
                Status::internal("context file stream ended before the file did"),
            ));
        }

        Ok(files)
    }
}

impl ConnectionServiceImpl {
    /// This daemon's `session_files.SessionFilesService` surface — the one coordinate the two
    /// context reads below are served at, whichever host holds the codebase.
    ///
    /// Reached through an `Arc` of a clone, as [`Self::session_room_roster`] is reached from the
    /// split start (`svc_spawn_split_agent`): `Clone` here is the documented shallow, shared clone,
    /// so the surface talks to this daemon's own config, roster and room slot. Deliberately not
    /// [`Self::self_arc`], which panics unless `runtime.rs` recorded the self handle — a split
    /// session's context read must not depend on wiring only the daemon binary performs.
    fn session_files_of_this_daemon(&self) -> PeerRoutedSessionFiles {
        std::sync::Arc::new(self.clone()).session_files_service()
    }

    /// The project's own guidance, fetched from the daemon that holds the codebase.
    ///
    /// Routed through this daemon's own handlers, exactly as
    /// [`Self::split_withdrawals_from_codebase_host`] routes the roster read, and subject to the
    /// same rule: **a failure is a refusal, never an empty result**. "The codebase host is
    /// unreachable" and "the project has no `CLAUDE.md`" would otherwise produce the same context
    /// dir, and the agent would work an entire session against rules it was never shown, with
    /// nothing anywhere saying so.
    ///
    /// Everything is fetched up front rather than lazily because populating an empty directory
    /// reads every allow-listed path anyway; what it buys is that the synchronous builder never has
    /// to reach a peer (`context_sync::PrefetchedContext`).
    ///
    /// `verb` is what the caller is doing — `"start"` or `"resume"`; [`ContextRead`] documents why
    /// it is a parameter rather than a word in the message.
    /// Every allow-listed path in the addressed host's checkout, with the hash that says whether
    /// it moved.
    ///
    /// Asked of the *service* rather than read here, even when the codebase is on this host: that
    /// surface is what classifies the route, gates the read by
    /// [`ConnectionServiceImpl::session_context_scope`] and bounds it by this host's
    /// `spawn_worker_request_timeout`. A second local read beside it would be a second answer to
    /// all three — and an unbounded one, which is what a stalled checkout turns into a split start
    /// that never finishes and never says why.
    ///
    /// Collected rather than streamed because the caller bounds the whole set against this host's
    /// attachment cap before spending a read on any of it.
    async fn context_manifest_of(
        &self,
        req: ContextManifestRequest,
    ) -> Result<Vec<ContextManifestEntry>, Status> {
        let frames = self
            .session_files_of_this_daemon()
            .stream_context_manifest(Request::new(req))
            .await?
            .into_inner();
        every_frame_of(frames).await
    }

    /// The bytes of every path in one request, from the same host and under the same gate and
    /// deadline as [`Self::context_manifest_of`].
    ///
    /// The *stream*, not a `Vec`: these frames carry the bytes, and how many of them follow is the
    /// peer's choice rather than anything its manifest promised. They are bounded as they arrive
    /// (`ContextRead::files_bounded_as_they_arrive`) so a peer serving past the cap is refused —
    /// and its receiver dropped, tearing the forward down — before the overage is held here.
    async fn context_file_batch_of(
        &self,
        req: ReadContextFileBatchRequest,
    ) -> Result<MpscResultStream<ContextFileBatchChunk>, Status> {
        Ok(self
            .session_files_of_this_daemon()
            .stream_read_context_file_batch(Request::new(req))
            .await?
            .into_inner())
    }

    pub(crate) async fn split_context_from_codebase_host(
        &self,
        session_token: &str,
        codebase_session: &str,
        codebase_daemon: &str,
        agent: &str,
        verb: &str,
    ) -> Result<crate::context_sync::PrefetchedContext, Status> {
        let read = ContextRead {
            verb,
            codebase_session,
            codebase_daemon,
        };

        let manifest = self
            .context_manifest_of(ContextManifestRequest {
                session_token: session_token.to_string(),
                session_id: codebase_session.to_string(),
                daemon_instance_id: codebase_daemon.to_string(),
                agent: agent.to_string(),
            })
            .await
            .map_err(|status| read.refusal("reading the context manifest", status))?;

        let entries: Vec<tddy_sandbox::ContextEntry> = manifest
            .into_iter()
            .map(|entry| tddy_sandbox::ContextEntry {
                rel_path: entry.rel_path,
                sha256: entry.sha256,
                size_bytes: entry.size_bytes,
            })
            .collect();

        // Every number below is the *peer's*, and the whole set is about to be held in memory at
        // once. `size_bytes` rides the manifest precisely so a client can refuse before spending a
        // read (PRD § Design), so it is spent here rather than trusted: this host's own
        // `max_attachment_bytes` bounds both one file and the tree, and the refusal happens before
        // the first byte is requested. Without it a peer's manifest is an instruction to allocate.
        let max_bytes = self.config.max_attachment_bytes;
        if let Some(oversized) = entries.iter().find(|entry| entry.size_bytes > max_bytes) {
            return Err(read.refusal(
                &format!(
                    "the manifest offered {} at {} bytes, over this host's {max_bytes} byte cap; \
                     reading it",
                    oversized.rel_path, oversized.size_bytes
                ),
                Status::invalid_argument("context file over the attachment cap"),
            ));
        }
        let total_bytes: u64 = entries.iter().map(|entry| entry.size_bytes).sum();
        if total_bytes > max_bytes {
            return Err(read.refusal(
                &format!(
                    "the manifest offered {} path(s) totalling {total_bytes} bytes, over this \
                     host's {max_bytes} byte cap; reading them",
                    entries.len()
                ),
                Status::invalid_argument("context manifest over the attachment cap"),
            ));
        }

        // A repository that ships no agent configuration at all is served an empty manifest, and
        // there is nothing to ask for: the batch read refuses an empty path list rather than let a
        // caller spend a round trip on nothing, so the call is not made.
        if entries.is_empty() {
            log::info!(
                "split context: session {codebase_session} on daemon {codebase_daemon} serves no \
                 allow-listed path; the context dir will hold the managed-codebase preamble alone"
            );
            return crate::context_sync::PrefetchedContext::new(entries, Default::default());
        }

        // **One** call for the whole set, not one per path. Every byte here is fetched before the
        // agent process exists, so the round trips are dead time the operator waits through: a
        // 120-file `.claude/skills/` tree read one file at a time is 121 sequential peer calls,
        // ~18s on a 150 ms link, and 121 separate chances to trip `PEER_FORWARD_TIMEOUT`. Batched,
        // a split start costs two peer calls whatever the tree's size.
        let frames = self
            .context_file_batch_of(ReadContextFileBatchRequest {
                session_token: session_token.to_string(),
                session_id: codebase_session.to_string(),
                daemon_instance_id: codebase_daemon.to_string(),
                agent: agent.to_string(),
                rel_paths: entries.iter().map(|e| e.rel_path.clone()).collect(),
            })
            .await
            .map_err(|status| read.refusal("reading the context files", status))?;

        let files = read
            .files_bounded_as_they_arrive(frames, &entries, max_bytes)
            .await?;

        // `end_of_file` is the peer's word for it, and on its own that is all truncation detection
        // rests on — a peer that sets the flag early yields a short `CLAUDE.md` this host accepts,
        // writes, and then records in `SyncState` under the *manifest's* hash. The record would
        // then describe bytes that are not on disk, so every later tick sees held == served and
        // never repairs it: one wrong flag, and the agent reads a `CLAUDE.md` missing its last rule
        // for the life of the session.
        //
        // The manifest this host already holds says what the bytes must be, and the bytes are
        // already in memory, so both halves of that claim are checked rather than taken on trust.
        // The size first, because it names the defect precisely (a truncated stream) where a hash
        // mismatch could be either that or corruption in flight.
        for entry in &entries {
            let bytes = files.get(&entry.rel_path).map(Vec::as_slice).unwrap_or(&[]);
            if bytes.len() as u64 != entry.size_bytes {
                return Err(read.refusal(
                    &format!(
                        "the batch served {} byte(s) of {:?}, which its manifest advertised at {} \
                         byte(s); reading it",
                        bytes.len(),
                        entry.rel_path,
                        entry.size_bytes
                    ),
                    Status::internal("context file stream ended before the file did"),
                ));
            }
            let received = tddy_sandbox::sha256_hex(bytes);
            if received != entry.sha256 {
                return Err(read.refusal(
                    &format!(
                        "the batch served {:?} hashing {received}, which its manifest advertised \
                         as {}; reading it",
                        entry.rel_path, entry.sha256
                    ),
                    Status::internal("context file content does not match its manifest entry"),
                ));
            }
        }

        crate::context_sync::PrefetchedContext::new(entries, files)
    }

    /// Re-spawn and re-dial a sandboxed claude-cli session.
    pub(crate) async fn resume_sandboxed_claude_cli_session(
        &self,
        _os_user: &str,
        session_id: &str,
        session_dir: PathBuf,
        meta: tddy_core::SessionMetadata,
    ) -> Result<Response<ResumeSessionResponse>, Status> {
        if let Some(state) = self.sandbox_manager.remove(session_id).await {
            state.stop();
        } else if let Some(pid) = meta.pid {
            tddy_daemon_sandbox::sandbox_session::terminate_sandbox_process(pid);
        }
        tokio::time::sleep(Duration::from_millis(300)).await;

        let model = meta.model.clone().unwrap_or_default();
        let worktree_path = meta
            .repo_path
            .as_ref()
            .map(PathBuf::from)
            .ok_or_else(|| Status::internal("sandbox session missing repo_path in metadata"))?;

        // A recipe in metadata marks a managed session; re-wire its workflow on resume.
        let managed_recipe = match meta.recipe.as_deref().filter(|s| !s.trim().is_empty()) {
            Some(name) => Some(
                tddy_workflow_recipes::resolve_workflow_recipe_from_cli_name(name)
                    .map_err(Status::invalid_argument)?,
            ),
            None => None,
        };

        let pid = self
            .relaunch_sandboxed_runner(
                session_id,
                &session_dir,
                &worktree_path,
                &model,
                "auto",
                &meta.agents,
                managed_recipe,
                // Resume path: the transcript already exists under the persistent sandbox claude
                // HOME, so the runner must launch `claude --resume <id>`, not `--session-id <id>`.
                true,
            )
            .await?;

        let now = chrono::Utc::now().to_rfc3339();
        let updated = tddy_core::SessionMetadata {
            updated_at: now,
            status: "active".to_string(),
            pid: Some(pid),
            ..meta
        };
        tddy_core::write_session_metadata(&session_dir, &updated)
            .map_err(|e| Status::internal(format!("failed to update session metadata: {e}")))?;

        Ok(Response::new(ResumeSessionResponse {
            session_id: session_id.to_string(),
            livekit_room: String::new(),
            livekit_url: String::new(),
            livekit_server_identity: String::new(),
        }))
    }
}

/// The deadline a split session's own context read is bounded by.
///
/// In-crate rather than under `tests/` because [`ConnectionServiceImpl::split_context_from_codebase_host`]
/// is `pub(crate)`: the behaviour worth pinning is what *this* path does with a checkout that
/// stalls, and an integration test could only reach it by starting a whole split session against a
/// peer.
///
/// Co-located deliberately: when the codebase is on this host, the read is this daemon's own
/// filesystem work, and a filesystem can genuinely stall (a network mount, a device that stopped
/// answering). Unbounded, the split start waits for exactly as long as the stall lasts, with
/// nothing for the caller to retry and nothing for an operator to raise.
///
/// **How a read is made to stall here.** The read runs on the runtime's blocking pool, so this
/// gives the runtime exactly one blocking thread and hands it a read that does not return until the
/// assertion has been made — the same technique as
/// `tddy-session-files/tests/context_read_deadline_acceptance.rs`, and for the same reason: a slow
/// filesystem or a `sleep` would be a wall-clock race that passes or fails with the load on the
/// machine.
///
/// PRD: docs/ft/daemon/agent-context-sync.md.
#[cfg(test)]
mod the_deadline_a_split_sessions_context_read_is_bounded_by {
    use std::future::Future;
    use std::path::Path;
    use std::sync::Arc;

    use pretty_assertions::assert_eq;
    use tddy_rpc::Code;

    use super::*;
    use crate::cli_session_manager::CliSessionManager;
    use crate::test_util::{test_config, TEST_TOKEN, TEST_USER};

    /// The instance id this daemon answers to — and the one the split session records as holding
    /// its codebase, which is what makes the read below local rather than a peer forward.
    const THIS_HOST: &str = "the-codebase-host";

    /// The workspace session a split session's agent fetches its guidance from.
    const CODEBASE_SESSION: &str = "aaaaaaaa-aaaa-7aaa-8aaa-aaaaaaaaaaaa";

    /// The agent half, recorded on the codebase session as the pairing that makes it a split one.
    const AGENT_SESSION: &str = "bbbbbbbb-bbbb-7bbb-8bbb-bbbbbbbbbbbb";

    /// The budget this host is configured to allow one context read.
    ///
    /// One second because `spawn_worker_request_timeout_secs` is whole seconds and the refusal
    /// quotes them: the shortest budget an operator can configure, so the message asserted below is
    /// one production can really produce.
    const A_ONE_SECOND_READ_BUDGET_SECS: u64 = 1;

    /// How long this test waits for the fetch itself, which is *not* the behaviour under test: ten
    /// times the budget above, so a context read bounded by nothing at all fails saying what never
    /// happened instead of hanging the suite.
    const A_CALLS_OWN_PATIENCE: std::time::Duration = std::time::Duration::from_secs(10);

    /// A split session whose codebase half lives on this very daemon, with a checkout to read.
    struct ASplitSession {
        _data_dir: tempfile::TempDir,
        _checkout: tempfile::TempDir,
        service: ConnectionServiceImpl,
    }

    /// This daemon holding the codebase of one split session, allowing a context read `budget_secs`.
    fn a_split_session_whose_codebase_read_may_take(budget_secs: u64) -> ASplitSession {
        let data_dir = tempfile::tempdir().expect("a data dir");
        let checkout = tempfile::tempdir().expect("a checkout");
        std::fs::write(checkout.path().join("CLAUDE.md"), b"# the project's rules")
            .expect("the guidance file");
        a_codebase_session_in(data_dir.path(), checkout.path());

        let mut config = test_config();
        config.daemon_instance_id = Some(THIS_HOST.to_string());
        config.spawn_worker_request_timeout_secs = budget_secs;
        let base = data_dir.path().to_path_buf();
        let sessions_base: tddy_daemon_kernel::SessionsBaseResolver =
            Arc::new(move |_| Some(base.clone()));
        let users: tddy_daemon_kernel::SessionUserResolver =
            Arc::new(|token| (token == TEST_TOKEN).then(|| TEST_USER.to_string()));
        let service = ConnectionServiceImpl::new(
            config,
            sessions_base,
            data_dir.path().to_path_buf(),
            users,
            None,
            None,
            None,
            Arc::new(CliSessionManager::new()),
        );
        ASplitSession {
            _data_dir: data_dir,
            _checkout: checkout,
            service,
        }
    }

    /// The `workspace` session a split start records on the codebase host: the checkout it holds,
    /// and the agent half it is paired with — the pairing is what makes the `claude` allow-list the
    /// row this session is served.
    fn a_codebase_session_in(data_dir: &Path, checkout: &Path) {
        let session_dir =
            tddy_core::session_lifecycle::unified_session_dir_path(data_dir, CODEBASE_SESSION);
        std::fs::create_dir_all(&session_dir).expect("the session dir");
        std::fs::write(
            session_dir.join(tddy_core::SESSION_METADATA_FILENAME),
            format!(
                "session_id: {CODEBASE_SESSION}\n\
                 project_id: 019d105b-ac0f-78d3-9a89-409731145a40\n\
                 created_at: 2026-09-11T09:00:00Z\n\
                 updated_at: 2026-09-11T09:00:00Z\n\
                 status: active\n\
                 session_type: workspace\n\
                 repo_path: {checkout}\n\
                 agent_daemon_instance_id: the-agent-host\n\
                 agent_session_id: {AGENT_SESSION}\n",
                checkout = checkout.display()
            ),
        )
        .expect("the session metadata");
    }

    /// Drive one fetch on a runtime whose only blocking thread is held by a read that never
    /// returns, and give back the refusal it answered with.
    ///
    /// The held read is released once the fetch has answered, so the queued read drains and the
    /// runtime shuts down rather than the test leaking a parked thread.
    fn the_refusal_when_the_read_cannot_start<Call, Answer, Fetched>(call: Call) -> Status
    where
        Call: FnOnce() -> Answer,
        Answer: Future<Output = Result<Fetched, Status>>,
    {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .max_blocking_threads(1)
            .build()
            .expect("a runtime with exactly one blocking thread");
        let (release, held) = std::sync::mpsc::channel::<()>();
        runtime.spawn_blocking(move || {
            let _ = held.recv();
        });

        let answer = runtime.block_on(async {
            tokio::time::timeout(A_CALLS_OWN_PATIENCE, call())
                .await
                .expect("the fetch never answered: the context read was bounded by no deadline")
        });

        release.send(()).expect("the held read to be releasable");
        answer
            .err()
            .expect("a read that cannot start inside the budget must refuse the start")
    }

    #[test]
    fn refuses_a_split_start_whose_codebase_read_does_not_return_inside_the_hosts_budget() {
        // Given this host holding the codebase, allowing one context read a second
        let split = a_split_session_whose_codebase_read_may_take(A_ONE_SECOND_READ_BUDGET_SECS);

        // When the start fetches the project's guidance while that read cannot start
        let refusal = the_refusal_when_the_read_cannot_start(|| {
            split.service.split_context_from_codebase_host(
                TEST_TOKEN,
                CODEBASE_SESSION,
                THIS_HOST,
                "claude",
                "start",
            )
        });

        // Then the start is refused, naming the read that stalled and the key an operator raises
        assert_eq!(refusal.code(), Code::DeadlineExceeded);
        assert_eq!(
            refusal.message,
            format!(
                "cannot start a split session without the guidance held beside its codebase: \
                 reading the context manifest from session {CODEBASE_SESSION} on daemon \
                 {THIS_HOST} failed: StreamContextManifest: timed out after 1s \
                 (spawn_worker_request_timeout_secs)"
            )
        );
    }
}

/// The bound one batch read is held to **as it arrives**.
///
/// In-crate for the same reason the module above is: the drain is private to this path, and an
/// integration test could only reach it by standing up a peer that lies about its own manifest —
/// which is the one peer a real serving host never is. Fed a stream directly, the test can be that
/// peer.
///
/// What is being pinned is not only the refusal but *when* it happens. A batch collected first and
/// checked afterwards produces the same `Status` while having already held the whole overage on
/// this host, so both tests below serve their frames over a stream that is never closed: a drain
/// that waits for the end never answers at all.
///
/// PRD: docs/ft/daemon/agent-context-sync.md.
#[cfg(test)]
mod the_bound_a_batch_read_is_held_to_as_it_arrives {
    use std::collections::BTreeMap;

    use pretty_assertions::assert_eq;
    use tddy_rpc::Code;
    use tokio::sync::mpsc::{unbounded_channel, UnboundedSender};

    use super::*;

    /// The daemon holding the codebase, and the workspace session whose guidance is being read —
    /// the two coordinates every refusal below names.
    const THIS_HOST: &str = "the-codebase-host";
    const CODEBASE_SESSION: &str = "aaaaaaaa-aaaa-7aaa-8aaa-aaaaaaaaaaaa";

    /// The one path the manifest advertises, and the size it advertises it at — under the cap, so
    /// nothing refuses this read before the bytes start arriving.
    const THE_ADVERTISED_PATH: &str = "CLAUDE.md";
    const THE_ADVERTISED_SIZE: u64 = 4;

    /// What this host allows one context tree. Eight bytes so the frames it takes to cross the cap
    /// are countable in the test rather than generated.
    const A_HOSTS_EIGHT_BYTE_CAP: u64 = 8;

    /// How long the drain is given to answer. Not the behaviour under test: a drain that collects
    /// the batch before applying the cap never answers while the peer holds its stream open, and
    /// this is what turns that into a failure that says so.
    const A_CALLERS_OWN_PATIENCE: Duration = Duration::from_secs(5);

    /// A peer serving one batch of context files, frame by frame, over a stream it never closes.
    ///
    /// Frames are queued *before* the drain runs, so nothing about the test's outcome depends on
    /// the two racing; and the sender stays open afterwards, so "has this host stopped taking
    /// frames" is a question [`Self::is_still_being_drained`] can answer.
    struct APeerServingOneBatch(UnboundedSender<Result<ContextFileBatchChunk, Status>>);

    /// The peer, and the stream this host drains it through.
    fn a_peer_serving_one_batch() -> (
        APeerServingOneBatch,
        MpscResultStream<ContextFileBatchChunk>,
    ) {
        let (frames, drained) = unbounded_channel();
        (
            APeerServingOneBatch(frames),
            MpscResultStream::from(drained),
        )
    }

    impl APeerServingOneBatch {
        /// One more frame of the advertised file, with more to follow.
        fn serves(&self, data: &[u8]) {
            self.0
                .send(Ok(a_frame_of(data, false)))
                .expect("this host is still draining the batch");
        }

        /// The file's last frame, after which the peer's stream closes — as a served batch's does.
        fn serves_the_end_of_the_file(self, data: &[u8]) {
            self.0
                .send(Ok(a_frame_of(data, true)))
                .expect("this host is still draining the batch");
        }

        /// Whether this host is still taking frames. `false` once it has dropped the receiver,
        /// which is what tears the peer's forward down rather than letting it serve into memory.
        fn is_still_being_drained(&self) -> bool {
            self.0.send(Ok(a_frame_of(b"more", false))).is_ok()
        }
    }

    /// One frame of the advertised file, carrying the size its manifest entry claims.
    fn a_frame_of(data: &[u8], end_of_file: bool) -> ContextFileBatchChunk {
        ContextFileBatchChunk {
            rel_path: THE_ADVERTISED_PATH.to_string(),
            data: data.to_vec(),
            total_byte_size: THE_ADVERTISED_SIZE,
            end_of_file,
        }
    }

    /// The manifest this host holds: one path, at the size the peer advertised it.
    ///
    /// `sha256` is what the *handler* checks the assembled bytes against; the drain never reads it,
    /// which is why it is the hash of nothing in particular here.
    fn the_manifest_it_advertised() -> Vec<tddy_sandbox::ContextEntry> {
        vec![tddy_sandbox::ContextEntry {
            rel_path: THE_ADVERTISED_PATH.to_string(),
            sha256: tddy_sandbox::sha256_hex(b"0123"),
            size_bytes: THE_ADVERTISED_SIZE,
        }]
    }

    /// The split start doing the reading, as its refusals name it.
    fn a_split_start() -> ContextRead<'static> {
        ContextRead {
            verb: "start",
            codebase_session: CODEBASE_SESSION,
            codebase_daemon: THIS_HOST,
        }
    }

    #[tokio::test]
    async fn refuses_a_peer_serving_past_the_cap_without_taking_the_rest_of_its_batch() {
        // Given a peer that has queued twelve bytes of a file it advertised at four, over a stream
        // it never closes
        let (peer, batch) = a_peer_serving_one_batch();
        peer.serves(b"0123");
        peer.serves(b"4567");
        peer.serves(b"89ab");

        // When this host drains that batch under its own eight-byte cap
        let refusal = tokio::time::timeout(
            A_CALLERS_OWN_PATIENCE,
            a_split_start().files_bounded_as_they_arrive(
                batch,
                &the_manifest_it_advertised(),
                A_HOSTS_EIGHT_BYTE_CAP,
            ),
        )
        .await
        .expect("the drain never answered: the batch was collected before the cap was applied")
        .expect_err("a peer serving past this host's cap must be refused");

        // Then the start is refused, naming the file and the cap it crossed
        assert_eq!(refusal.code(), Code::InvalidArgument);
        assert_eq!(
            refusal.message,
            format!(
                "cannot start a split session without the guidance held beside its codebase: \
                 CLAUDE.md streamed more than this host's 8 byte cap; reading it from session \
                 {CODEBASE_SESSION} on daemon {THIS_HOST} failed: context file over the \
                 attachment cap"
            )
        );

        // And the peer's forward is torn down rather than left serving into this host's memory:
        // the advertised size is the peer's number, which is why the cap cannot wait for the end
        assert!(
            !peer.is_still_being_drained(),
            "this host was still taking frames after refusing the batch"
        );
    }

    #[tokio::test]
    async fn assembles_the_bytes_of_a_peer_that_serves_exactly_what_it_advertised() {
        // Given a peer serving its four advertised bytes in two frames, the second declaring the
        // file finished
        let (peer, batch) = a_peer_serving_one_batch();
        peer.serves(b"01");
        peer.serves_the_end_of_the_file(b"23");

        // When this host drains that batch under its own cap
        let files = tokio::time::timeout(
            A_CALLERS_OWN_PATIENCE,
            a_split_start().files_bounded_as_they_arrive(
                batch,
                &the_manifest_it_advertised(),
                A_HOSTS_EIGHT_BYTE_CAP,
            ),
        )
        .await
        .expect("the drain answered an honest peer")
        .expect("a batch inside the cap is served");

        // Then the frames are assembled in order under the path its manifest named
        assert_eq!(
            files,
            BTreeMap::from([(THE_ADVERTISED_PATH.to_string(), b"0123".to_vec())])
        );
    }
}
