// TODO(restructure): hand-added -- extract_module does not carry a trait import used only for its
// methods (`AsyncWriteExt::write_all` below), so the compile gate failed without it.
use super::ScreenSharingServiceImpl;

use std::process::Stdio;

use log::{error, info, warn};
use tokio::io::AsyncWriteExt;
use tokio::process::{Child, ChildStdout, Command};
use tokio::sync::Mutex;

use std::collections::HashMap;

use crate::bridge_config::PreparedBridge;

use crate::bridge_readiness::{await_bridge_readiness, BridgeStarted};

use crate::bridge_config::prepare_bridge;

use super::BridgeTarget;

use tddy_daemon_kernel::config::DaemonConfig;

use crate::bridge_config::decrypt_desktop_password;

use tddy_host_service::host_prompts::answer_before_expiry;

use crate::bridge_config::desktop_prompt_subject;

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
            // SAFETY: pid > 0 is enforced by the `if pid > 0` guard in `record_bridge_pid`
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

    /// Spawn a bridge for the given target, and report whether its desktop came up.
    ///
    /// The **session-scoped** spawn. It reports a failed start the way the host path does
    /// ([`Self::spawn_bridge`]): a desktop that refused the sign-in, or a bridge that never came up,
    /// is not handed back as a stream that will carry no frame.
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
    ) -> Result<BridgeStarted, Status> {
        let prepared = prepare_bridge(
            config,
            target,
            username,
            bridge_identity,
            track_name,
            livekit_room,
            &*self.room_tokens,
        )
        .map_err(Status::failed_precondition)?
        .for_password(password);

        self.spawn_bridge(prepared, bridge_key).await
    }

    /// Start a bridge process, and wait for it to say whether the desktop came up.
    ///
    /// Takes a [`PreparedBridge`], so everything knowable without the desktop's password has
    /// already been settled. What is left is the spawn and the bridge's readiness line: a refused
    /// sign-in, a bridge that exits, or one silent past [`Self::bridge_ready_timeout`] fails the
    /// start with the status [`await_bridge_readiness`] maps it to, and the bridge is stopped
    /// before this returns.
    pub(crate) async fn spawn_bridge(
        &self,
        prepared: PreparedBridge,
        // Built by `session_bridge_key` or `host_bridge_key`: the scope decides how a running
        // bridge is addressed, the spawn itself does not care which.
        bridge_key: String,
    ) -> Result<BridgeStarted, Status> {
        let PreparedBridge {
            binary,
            spawn_config,
        } = prepared;

        let config_json = serde_json::to_vec(&spawn_config)
            .map_err(|e| Status::internal(format!("encoding the bridge's config: {e}")))?;

        // A second start for the same desktop must not strand the first process. `active_bridges`
        // holds one pid per key, so a spawn that overwrote one would leave a bridge nothing can
        // ever signal — running against a desktop nobody is watching.
        self.terminate_bridge(&bridge_key).await;

        let (mut child, stdout) = start_bridge_process(&binary, &config_json).await?;
        let pid = self.record_bridge_pid(&bridge_key, &child).await;

        info!(
            "bridge spawned: binary={} pid={} key={}",
            binary, pid, bridge_key
        );

        let started = self
            .await_bridge_started(&mut child, stdout, &bridge_key, pid)
            .await?;
        reap_bridge(
            Arc::clone(&self.active_bridges),
            Arc::clone(&self.x11vnc),
            child,
            bridge_key,
            pid,
        );
        Ok(started)
    }

    /// Wait for the bridge's readiness line, stopping the bridge if it never says it came up.
    ///
    /// The bridge is stopped here, not by the caller, so no failed start can return with its
    /// process still running.
    async fn await_bridge_started(
        &self,
        child: &mut Child,
        stdout: ChildStdout,
        bridge_key: &str,
        pid: u32,
    ) -> Result<BridgeStarted, Status> {
        let started = match await_bridge_readiness(stdout, self.bridge_ready_timeout).await {
            Ok(started) => started,
            Err(status) => {
                self.stop_failed_bridge(child, bridge_key, pid).await;
                return Err(status);
            }
        };
        info!(
            "bridge started: key={} desktop={}x{}",
            bridge_key, started.width, started.height
        );
        Ok(started)
    }

    /// Store the bridge's PID so the matching stop call can send SIGTERM, and return it.
    ///
    /// Recorded before the readiness wait, so a stop that arrives while the bridge is still
    /// starting reaches it. A child with no PID yields the zero sentinel, which is never recorded:
    /// signalling pid 0 would reach the daemon's whole process group, not the bridge.
    async fn record_bridge_pid(&self, bridge_key: &str, child: &Child) -> u32 {
        let pid = child.id().unwrap_or(0);
        if pid > 0 {
            self.active_bridges
                .lock()
                .await
                .insert(bridge_key.to_string(), pid);
        }
        pid
    }

    /// Stop a bridge whose start failed, and forget its PID.
    ///
    /// A bridge that refused, failed, or never reported must not outlive the start.
    async fn stop_failed_bridge(&self, child: &mut Child, bridge_key: &str, pid: u32) {
        if let Err(e) = child.kill().await {
            warn!(
                "bridge stop after a failed start: key={} err={}",
                bridge_key, e
            );
        }
        forget_bridge_pid(&self.active_bridges, bridge_key, pid).await;
    }
}

/// Spawn the bridge binary and hand it its config, returning the child and its stdout.
///
/// The JSON config is written to the bridge's stdin, which is then closed: the bridge reads to
/// EOF. A child whose pipes are missing, or that cannot be handed its config, is killed rather
/// than left running with nothing to do.
async fn start_bridge_process(
    binary: &str,
    config_json: &[u8],
) -> Result<(Child, ChildStdout), Status> {
    let mut child = Command::new(binary)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .map_err(|e| Status::internal(format!("spawning the bridge binary '{binary}': {e}")))?;

    let (Some(mut stdin), Some(stdout)) = (child.stdin.take(), child.stdout.take()) else {
        let _ = child.kill().await;
        return Err(Status::internal(
            "the bridge's stdin and stdout are not piped",
        ));
    };

    let written = stdin.write_all(config_json).await;
    drop(stdin);
    if let Err(e) = written {
        let _ = child.kill().await;
        return Err(Status::internal(format!(
            "writing the bridge's config to its stdin: {e}"
        )));
    }
    Ok((child, stdout))
}

/// Background task: wait for the process to exit (prevents zombie processes) and forget the PID
/// when it does.
///
/// A bridge that exits on its own, still the one its key names, takes its x11vnc with it. One that
/// was stopped or replaced does not: a stop stops its own x11vnc, and a reopen has already started
/// the one that belongs to the newer bridge under the same key.
fn reap_bridge(
    active_bridges: Arc<Mutex<HashMap<String, u32>>>,
    x11vnc: Arc<crate::x11vnc::X11vncServers>,
    mut child: Child,
    bridge_key: String,
    pid: u32,
) {
    tokio::spawn(async move {
        match child.wait().await {
            Ok(status) => {
                info!("bridge exited: key={} status={}", bridge_key, status)
            }
            Err(e) => error!("bridge wait error: key={} err={}", bridge_key, e),
        }
        if forget_bridge_pid(&active_bridges, &bridge_key, pid).await {
            x11vnc.stop(&bridge_key).await;
        }
    });
}

/// Forget `bridge_key`'s PID — only while the key still names *this* bridge.
///
/// A restart records a newer pid under the same key, and forgetting that one would leave the
/// running process unstoppable. Returns whether it did forget it.
async fn forget_bridge_pid(
    active_bridges: &Mutex<HashMap<String, u32>>,
    bridge_key: &str,
    pid: u32,
) -> bool {
    let mut bridges = active_bridges.lock().await;
    let names_this_bridge = bridges.get(bridge_key) == Some(&pid);
    if names_this_bridge {
        bridges.remove(bridge_key);
    }
    names_this_bridge
}
