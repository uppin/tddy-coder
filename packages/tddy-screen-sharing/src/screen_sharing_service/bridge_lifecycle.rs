// TODO(restructure): hand-added -- extract_module does not carry a trait import used only for its
// methods (`AsyncWriteExt::write_all` below), so the compile gate failed without it.
use super::ScreenSharingServiceImpl;

use std::process::Stdio;

use log::{error, info};
use tokio::io::AsyncWriteExt;
use tokio::process::Command;

use super::PreparedBridge;

use super::prepare_bridge;

use super::BridgeTarget;

use tddy_daemon_kernel::config::DaemonConfig;

use super::decrypt_desktop_password;

use tddy_host_service::host_prompts::answer_before_expiry;

use super::desktop_prompt_subject;

use tddy_host_service::host_prompts::PromptKind;

use tddy_host_service::host_desktop_targets::HostDesktopTarget;

use super::HostScope;

use tddy_rpc::Status;

use crate::screen_sharing_records::ScreenSharingTargetStore;

use std::sync::Arc;

impl ScreenSharingServiceImpl {
    /// Signal a running bridge to stop and forget its PID.
    ///
    /// Shared by both scopes: the process, the signal and the PID map are the same, and only the
    /// key that addresses it differs.
    pub(crate) async fn terminate_bridge(&self, bridge_key: &str) {
        let pid_opt = self.active_bridges.lock().await.remove(bridge_key);

        if let Some(pid) = pid_opt {
            info!(
                "stop stream: sending SIGTERM to bridge pid={} key={}",
                pid, bridge_key
            );
            #[cfg(unix)]
            // SAFETY: pid > 0 is enforced by the `if pid > 0` guard in `try_spawn_bridge`
            // that filters out the zero sentinel before inserting into `active_bridges`.
            // Sending SIGTERM to a pid is safe — the signal is delivered to the process
            // group and the call does not affect memory safety.
            unsafe {
                libc::kill(pid as libc::pid_t, libc::SIGTERM);
            }
        } else {
            info!(
                "stop stream: no active bridge for key={} (already stopped?)",
                bridge_key
            );
        }
    }

    /// The wired credential store, or a refusal naming the fact that none is wired.
    ///
    /// `failed_precondition` rather than an empty listing: a daemon assembled without a store is a
    /// deployment mistake, and answering "no targets" would hide it behind a plausible screen.
    pub(crate) fn require_target_store(
        &self,
    ) -> Result<&Arc<dyn ScreenSharingTargetStore>, Status> {
        self.target_store
            .as_ref()
            .ok_or_else(|| Status::failed_precondition("no credential store is wired"))
    }

    /// Ask this host's operator for `target`'s password, and return what they typed.
    ///
    /// The question is raised **here**, by the daemon, rather than answered by the caller. It has to
    /// be: `HostPromptEvent` is the only place a host publishes the public key an answer travels
    /// under, so a browser that has not been asked anything has nothing to encrypt with — and a
    /// desktop password is stored nowhere on this host, which is the whole point of prompting for
    /// it.
    ///
    /// Every start asks. Nothing records whether a desktop wants a password, and a daemon that
    /// guessed would either skip the question for a desktop that needs one or refuse a desktop that
    /// does not; an operator answering with nothing is how a password-less desktop is opened.
    ///
    /// The wait is bounded by the prompt's own expiry, so an operator who walks away releases the
    /// call at the moment the question stops being answerable — never later, and never never.
    pub(crate) async fn prompt_for_desktop_password(
        &self,
        scope: &HostScope,
        operator: &str,
        target: &HostDesktopTarget,
    ) -> Result<String, Status> {
        // Stamped with the GitHub user that raised it, which is what makes it *this* operator's
        // question: `StreamHostPrompts` shows a prompt to nobody else, and nobody else can spend
        // its one answer. Anything else here and the question reaches no browser at all.
        let prompt = scope.prompts.issue(
            operator,
            PromptKind::DesktopPassword,
            &desktop_prompt_subject(target),
            tddy_host_service::host_registry::now_unix_ms(),
        );
        // Claimed immediately after issuing, because issuing is what puts the prompt on the feed: an
        // operator whose browser answers at once must find a handoff already waiting for them.
        let answer = match scope.prompts.awaited_answer(&prompt.prompt_id) {
            Some(handoff) => answer_before_expiry(handoff, &prompt).await,
            // The registry forgot the prompt between issuing it and being asked for its handoff,
            // which for the operator is indistinguishable from one that ran out of time.
            None => None,
        };
        let Some(encrypted_password) = answer else {
            // Refused rather than started without one: a bridge spawned against a desktop whose
            // password nobody supplied authenticates to nothing, and would leave a process running
            // for a stream that can never carry a frame.
            return Err(Status::deadline_exceeded(format!(
                "nobody answered the password prompt for {} before it expired",
                prompt.subject
            )));
        };
        decrypt_desktop_password(scope.keypair.as_ref(), &encrypted_password)
    }

    /// Spawn a bridge for the given target, logging every failure and reporting none.
    ///
    /// The **session-scoped** spawn, and deliberately silent: `start_stream` hands back its
    /// pre-computed LiveKit coordinates whether or not a bridge came up, which is the behaviour
    /// AC-9 pins and its callers depend on. The host path does not reuse this — it reports —
    /// see [`Self::spawn_bridge`] and `start_host_stream`.
    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn try_spawn_bridge(
        &self,
        config: &DaemonConfig,
        target: &BridgeTarget,
        username: String,
        password: String,
        bridge_identity: &str,
        track_name: &str,
        livekit_room: &str,
        // Built by `session_bridge_key` or `host_bridge_key`: the scope decides how a running
        // bridge is addressed, the spawn itself does not care which.
        bridge_key: String,
    ) {
        let prepared = match prepare_bridge(
            config,
            target,
            username,
            bridge_identity,
            track_name,
            livekit_room,
            &*self.room_tokens,
        ) {
            Ok(prepared) => prepared.for_password(password),
            Err(e) => {
                info!("bridge spawn skipped: {e}");
                return;
            }
        };

        if let Err(e) = self.spawn_bridge(prepared, bridge_key).await {
            error!("bridge spawn failed: {e}");
        }
    }

    /// Start a bridge process, and say so when it does not start.
    ///
    /// Takes a [`PreparedBridge`], so everything knowable without the desktop's password has
    /// already been settled: what is left here is the spawn itself, and the only failures it can
    /// report are real ones.
    pub(crate) async fn spawn_bridge(
        &self,
        prepared: PreparedBridge,
        // Built by `session_bridge_key` or `host_bridge_key`: the scope decides how a running
        // bridge is addressed, the spawn itself does not care which.
        bridge_key: String,
    ) -> Result<(), String> {
        let PreparedBridge {
            binary,
            spawn_config,
        } = prepared;

        let config_json = serde_json::to_vec(&spawn_config)
            .map_err(|e| format!("encoding the bridge's config: {e}"))?;

        // A second start for the same desktop must not strand the first process. `active_bridges`
        // holds one pid per key, so a spawn that overwrote one would leave a bridge nothing can
        // ever signal — running against a desktop nobody is watching.
        self.terminate_bridge(&bridge_key).await;

        let mut child = Command::new(&binary)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::inherit())
            .spawn()
            .map_err(|e| format!("spawning the bridge binary '{binary}': {e}"))?;

        // Write the JSON config to the bridge's stdin, then close it.
        if let Some(mut stdin) = child.stdin.take() {
            if let Err(e) = stdin.write_all(&config_json).await {
                let _ = child.kill().await;
                return Err(format!("writing the bridge's config to its stdin: {e}"));
            }
            // stdin dropped here → pipe closed → bridge reads EOF and proceeds
        }

        // Store the PID so the matching stop call can send SIGTERM.
        let pid = child.id().unwrap_or(0);
        if pid > 0 {
            self.active_bridges
                .lock()
                .await
                .insert(bridge_key.clone(), pid);
        }

        info!(
            "bridge spawned: binary={} pid={} key={}",
            binary, pid, bridge_key
        );

        // Background task: wait for the process to exit (prevents zombie processes) and
        // forget the PID when it does.
        let active_bridges = Arc::clone(&self.active_bridges);
        tokio::spawn(async move {
            match child.wait().await {
                Ok(status) => {
                    info!("bridge exited: key={} status={}", bridge_key, status)
                }
                Err(e) => error!("bridge wait error: key={} err={}", bridge_key, e),
            }
            // Only while the key still names *this* bridge. A restart records a newer pid under
            // the same key, and forgetting that one would leave the running process unstoppable.
            let mut bridges = active_bridges.lock().await;
            if bridges.get(&bridge_key) == Some(&pid) {
                bridges.remove(&bridge_key);
            }
        });

        Ok(())
    }
}
