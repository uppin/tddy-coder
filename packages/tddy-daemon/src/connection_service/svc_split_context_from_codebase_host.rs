use std::time::Duration;

use tddy_service::proto::connection::ResumeSessionResponse;
use tddy_service::proto::session_files::{
    ContextFileBatchChunk, ContextManifestEntry, ContextManifestRequest,
    ReadContextFileBatchRequest,
};

use tddy_rpc::Response;

use std::path::PathBuf;

use tddy_rpc::Status;

use super::ConnectionServiceImpl;

/// The coordinate the two context reads below are served at — this daemon's own when the codebase
/// lives here, a peer's otherwise. Named because a forward has to be addressed at the service that
/// *declares* the method, and `connection.ConnectionService` has not since `#unbundle` node 6.
const SESSION_FILES_SERVICE: &str = "session_files.SessionFilesService";

impl ConnectionServiceImpl {
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
    /// `verb` is what the caller is doing — `"start"` or `"resume"`. It is a parameter rather than a
    /// word in the message because both paths reach this: a resume that cannot read its project's
    /// guidance re-fetches for the same reason a start does (the repository moved on while the
    /// session was stopped), and an operator reading "cannot start" about a session that was already
    /// running is being told to look in the wrong place.
    /// Every allow-listed path in the addressed host's checkout, with the hash that says whether
    /// it moved.
    ///
    /// Routed before anything is read, exactly as the served coordinate routes it: this daemon is
    /// usually the *agent* host asking the one that holds the codebase, and answered locally it
    /// would report an empty session directory as the project's guidance. When the codebase is
    /// here, the read is gated by [`ConnectionServiceImpl::session_context_scope`] — the same
    /// resolution `session_files.SessionFilesService` serves a wire caller through.
    ///
    /// Collected rather than streamed because the caller bounds the whole set against this host's
    /// attachment cap before spending a read on any of it.
    async fn context_manifest_of(
        &self,
        req: ContextManifestRequest,
    ) -> Result<Vec<ContextManifestEntry>, Status> {
        if let Some(mut frames) = self
            .stream_served_by_peer::<_, ContextManifestEntry>(
                SESSION_FILES_SERVICE,
                "StreamContextManifest",
                &req.daemon_instance_id,
                &req,
            )
            .await?
        {
            let mut entries = Vec::new();
            while let Some(frame) = frames.recv().await {
                entries.push(frame?);
            }
            return Ok(entries);
        }

        let scope = self.session_context_scope(&req.session_token, &req.session_id, &req.agent)?;
        let max_bytes = self.config.max_attachment_bytes;
        tokio::task::spawn_blocking(move || {
            crate::context_files::context_manifest(&scope.worktree_root, scope.globs, max_bytes)
        })
        .await
        .map_err(|e| Status::internal(e.to_string()))?
    }

    /// The bytes of every path in one request, from the same host and under the same gate as
    /// [`Self::context_manifest_of`].
    async fn context_file_batch_of(
        &self,
        req: ReadContextFileBatchRequest,
    ) -> Result<Vec<ContextFileBatchChunk>, Status> {
        if let Some(mut frames) = self
            .stream_served_by_peer::<_, ContextFileBatchChunk>(
                SESSION_FILES_SERVICE,
                "StreamReadContextFileBatch",
                &req.daemon_instance_id,
                &req,
            )
            .await?
        {
            let mut chunks = Vec::new();
            while let Some(frame) = frames.recv().await {
                chunks.push(frame?);
            }
            return Ok(chunks);
        }

        let scope = self.session_context_scope(&req.session_token, &req.session_id, &req.agent)?;
        let max_bytes = self.config.max_attachment_bytes;
        let rel_paths = req.rel_paths;
        let files = tokio::task::spawn_blocking(move || {
            crate::context_files::read_context_files_bytes(
                &scope.worktree_root,
                &rel_paths,
                scope.globs,
                max_bytes,
            )
        })
        .await
        .map_err(|e| Status::internal(e.to_string()))??;
        Ok(crate::context_files::context_file_batch_frames(&files))
    }

    pub(crate) async fn split_context_from_codebase_host(
        &self,
        session_token: &str,
        codebase_session: &str,
        codebase_daemon: &str,
        agent: &str,
        verb: &str,
    ) -> Result<crate::context_sync::PrefetchedContext, Status> {
        let refusal = |what: &str, status: Status| Status {
            code: status.code(),
            message: format!(
                "cannot {verb} a split session without the guidance held beside its codebase: \
                 {what} from session {codebase_session} on daemon {codebase_daemon} failed: {}",
                status.message()
            ),
        };

        let manifest = self
            .context_manifest_of(ContextManifestRequest {
                session_token: session_token.to_string(),
                session_id: codebase_session.to_string(),
                daemon_instance_id: codebase_daemon.to_string(),
                agent: agent.to_string(),
            })
            .await
            .map_err(|status| refusal("reading the context manifest", status))?;

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
            return Err(refusal(
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
            return Err(refusal(
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
            .map_err(|status| refusal("reading the context files", status))?;

        let advertised: std::collections::BTreeMap<&str, u64> = entries
            .iter()
            .map(|entry| (entry.rel_path.as_str(), entry.size_bytes))
            .collect();
        let mut files: std::collections::BTreeMap<String, Vec<u8>> =
            std::collections::BTreeMap::new();
        let mut complete: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
        let mut received_bytes: u64 = 0;
        for frame in frames {
            // The peer names the file each frame belongs to, and a name that was never asked for is
            // a path this host is about to create in the agent's working directory. Refused here
            // rather than filtered, for the reason `context_sync::refuse_unlisted_paths` gives: a
            // peer serving something outside the set is a defect on one side or the other, and
            // dropping it silently leaves the two halves disagreeing about what the agent reads.
            let Some(&size_bytes) = advertised.get(frame.rel_path.as_str()) else {
                return Err(refusal(
                    &format!(
                        "the batch carried {:?}, which its manifest never advertised; reading it",
                        frame.rel_path
                    ),
                    Status::permission_denied("context file outside the manifest"),
                ));
            };
            if complete.contains(&frame.rel_path) {
                return Err(refusal(
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
                return Err(refusal(
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
            return Err(refusal(
                &format!(
                    "the batch never finished {:?}; reading it",
                    missing.rel_path
                ),
                Status::internal("context file stream ended before the file did"),
            ));
        }

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
                return Err(refusal(
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
                return Err(refusal(
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
