//! A daemon's screen-sharing service wired over throwaway storage, with the bridge binary pointed
//! at a `/bin/sh` stand-in whose behaviour each test scripts.
//!
//! A minimal re-creation of the fixtures in `screen_sharing_service.rs`'s inline `mod tests`
//! (`FakeBridge`, `AnOperatorAtTheKeyboard`, `TheSessionsCredentials`, `DaemonUnderTest`), kept
//! outside the service file so `#host-desktops` node 1's split does not move it. Only what drives
//! the public surface is here: everything below the RPC is the production code path.

#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tddy_core::session_metadata::{
    write_initial_tool_session_metadata, InitialToolSessionMetadataOpts,
};
use tddy_daemon_extensions::bridge_readiness::{BridgeReadiness, FailureKind};
use tddy_daemon_kernel::config::{DaemonConfig, LiveKitConfig, ScreenSharingConfig};
use tddy_host_service::host_desktop_targets::FileHostDesktopTargetStore;
use tddy_host_service::host_keypair::{FileHostKeypair, HostKeypair, PublishedKey};
use tddy_host_service::host_prompts::{
    AnswerHandoff, AnswerRejection, HostPromptRegistry, InMemoryHostPromptRegistry, PendingPrompt,
    PromptKind,
};
use tddy_rpc::{Request, Response, Status};
use tddy_screen_sharing::screen_sharing_records::{
    account_for, record_for, target_from, ScreenSharingTargetStore, TargetError,
};
use tddy_screen_sharing::{
    ScreenSharingServiceImpl, DEFAULT_BRIDGE_READY_TIMEOUT, DEFAULT_X11VNC_ACCEPT_TIMEOUT,
};
use tddy_service::proto::screen_sharing::{
    AddHostTargetRequest, AddTargetRequest, Protocol, ScreenSharingService,
    ScreenSharingTarget as ProtoScreenSharingTarget, StartHostStreamRequest, StartStreamRequest,
    StartStreamResponse, StopHostStreamRequest,
};
use tddy_testing_commons::process_is_alive;
use tddy_testing_commons::stub_scripts::make_executable;
use tddy_testing_commons::wait::eventually;

pub const A_SESSION_TOKEN: &str = "a-valid-session-token";
pub const THE_OS_USER: &str = "ada";
pub const A_HOST: &str = "workstation-1";
pub const A_SESSION: &str = "session-with-a-desktop";
const THE_COMMON_ROOM: &str = "tddy-hosts-lobby";
const THE_SESSIONS_OWN_ROOM: &str = "room-of-one-session";
const A_VNC_PORT: u32 = 5900;
/// Every desktop here is password-less, so that is what the operator types.
const NO_PASSWORD: &str = "";
const A_WRITE_CLOCK: u64 = 1_758_240_000;

/// Longer than any test here runs, so a stand-in bridge only ever exits because it was stopped.
const A_BRIDGE_STAYS_UP_FOR_SECS: u32 = 300;

/// Safety nets, not predictions: a stand-in that has recorded nothing by now was never spawned,
/// and a stopped bridge still alive by now was never stopped.
const A_BRIDGE_RECORDS_ITS_PID_WITHIN: Duration = Duration::from_secs(10);
pub const A_STOPPED_BRIDGE_EXITS_WITHIN: Duration = Duration::from_secs(10);

/// How a start that should end on its own is kept from hanging the suite when it does not.
pub const A_START_FINISHES_WITHIN: Duration = Duration::from_secs(20);

/// The argument that makes the stand-in exit at once, before it records anything.
const WARM_UP_ARGUMENT: &str = "--warm-up";

/// Where the stand-in x11vnc logs its launches, in the daemon's storage directory. A bridge that
/// [`TheBridge::ExitsWithItsX11vnc`] reads it too, to find the x11vnc it was pointed at.
const X11VNC_LAUNCH_LOG: &str = "x11vnc-launches";

/// How often a bridge that [`TheBridge::ExitsWithItsX11vnc`] checks whether its x11vnc is still up.
const A_BRIDGE_CHECKS_ITS_X11VNC_EVERY: &str = "0.02";

/// What the stand-in bridge does once the daemon has written its config to it.
#[derive(Debug, Clone)]
pub enum TheBridge {
    /// Writes this readiness line to its stdout (single-quoted in the script, so a message here
    /// must not contain `'`), then stays up until it is stopped — as a real
    /// bridge does after a refusal it has not yet acted on, which is why the daemon must stop it.
    Reports(BridgeReadiness),
    /// Writes nothing and stays up: a bridge hung somewhere in its sign-in.
    NeverReports,
    /// Exits without writing anything: a bridge that crashed before it knew anything.
    ExitsBeforeReporting,
    /// Writes this readiness line, then exits as soon as the x11vnc serving the port it was pointed
    /// at is gone — as a real `tddy-vnc` does when its VNC server goes away under it.
    ExitsWithItsX11vnc(BridgeReadiness),
}

impl TheBridge {
    pub fn ready_at(width: u32, height: u32) -> Self {
        Self::Reports(BridgeReadiness::Ready { width, height })
    }

    /// Ready at this size, and gone as soon as its x11vnc is.
    pub fn ready_at_until_its_x11vnc_stops(width: u32, height: u32) -> Self {
        Self::ExitsWithItsX11vnc(BridgeReadiness::Ready { width, height })
    }

    pub fn refusing_the_sign_in_with(reason: &str) -> Self {
        Self::Reports(BridgeReadiness::Failed {
            kind: FailureKind::Authentication,
            message: reason.to_string(),
        })
    }
}

/// A stand-in for `tddy-vnc` that records its pid, reads its config, then does what
/// [`TheBridge`] says.
///
/// `exec sleep` rather than `sleep`, so the recorded pid is the process still running when the
/// daemon signals it — a forked `sleep` would outlive a signalled shell and the assertion would be
/// about the wrong process.
pub struct StandInBridge {
    pub binary: PathBuf,
    pid_file: PathBuf,
    /// What the daemon wrote to the bridge's stdin.
    config_file: PathBuf,
}

impl StandInBridge {
    fn installed_in(dir: &Path, behaviour: &TheBridge) -> Self {
        let bridge = Self {
            binary: dir.join("tddy-vnc-stand-in"),
            pid_file: dir.join("bridge-pid"),
            config_file: dir.join("bridge-config.json"),
        };
        let pid = bridge.pid_file.display();
        let config = bridge.config_file.display();
        let after_config = match behaviour {
            TheBridge::Reports(readiness) => {
                let line = serde_json::to_string(readiness).expect("a readiness line");
                format!("printf '%s\\n' '{line}'\nexec sleep {A_BRIDGE_STAYS_UP_FOR_SECS}\n")
            }
            TheBridge::NeverReports => format!("exec sleep {A_BRIDGE_STAYS_UP_FOR_SECS}\n"),
            TheBridge::ExitsBeforeReporting => "exit 1\n".to_string(),
            // The x11vnc is found by the port in the bridge's config: the launch log line naming
            // `-rfbport <that port>` carries its pid. `-rfbport` is never x11vnc's last argument,
            // so the trailing space keeps one port from matching another that it prefixes.
            TheBridge::ExitsWithItsX11vnc(readiness) => {
                let line = serde_json::to_string(readiness).expect("a readiness line");
                let launches = dir.join(X11VNC_LAUNCH_LOG);
                let launches = launches.display();
                format!(
                    "port=$(sed -n 's/.*\"port\":\\([0-9]*\\).*/\\1/p' \"{config}\")\n\
                     x11vnc=$(grep -e \"-rfbport $port \" \"{launches}\" | sed 's/ .*//')\n\
                     printf '%s\\n' '{line}'\n\
                     while kill -0 \"$x11vnc\" 2>/dev/null; do sleep {A_BRIDGE_CHECKS_ITS_X11VNC_EVERY}; done\n\
                     exit 1\n"
                )
            }
        };
        // The pid is written to a temp file and `mv -f`'d into place, so a reader sees a whole pid
        // or none.
        std::fs::write(
            &bridge.binary,
            format!(
                "#!/bin/sh\n\
                 [ \"$1\" = {WARM_UP_ARGUMENT} ] && exit 0\n\
                 echo $$ > \"{pid}.part\"\n\
                 mv -f \"{pid}.part\" \"{pid}\"\n\
                 cat > \"{config}\"\n\
                 {after_config}"
            ),
        )
        .expect("writing the stand-in bridge");
        make_executable(&bridge.binary);
        bridge.warm_up();
        bridge
    }

    /// Run the freshly written script once, to completion, before any test clock starts.
    ///
    /// The first exec of a new executable can take seconds on a loaded macOS machine (the system
    /// scans it first); without this, a start with a short readiness budget gave up on, and killed,
    /// a stand-in that had not yet begun to run, and the test saw no pid ever recorded.
    fn warm_up(&self) {
        let status = std::process::Command::new(&self.binary)
            .arg(WARM_UP_ARGUMENT)
            .status()
            .expect("warming up the stand-in bridge");
        assert!(status.success(), "the stand-in bridge warm-up: {status}");
    }

    /// The pid the stand-in recorded when the daemon spawned it.
    pub async fn pid(&self) -> u32 {
        eventually(
            "the bridge records its pid",
            A_BRIDGE_RECORDS_ITS_PID_WITHIN,
            || {
                std::fs::read_to_string(&self.pid_file)
                    .map_err(|e| format!("{} is not readable yet: {e}", self.pid_file.display()))?
                    .trim()
                    .parse::<u32>()
                    .map_err(|e| format!("{} holds no pid yet: {e}", self.pid_file.display()))
            },
        )
        .await
    }

    /// Whether the daemon has spawned this bridge at all. Read after a start has returned, by
    /// which time any bridge it spawned has long since recorded its pid.
    pub fn was_ever_started(&self) -> bool {
        self.pid_file.exists()
    }

    /// The config the daemon wrote to the bridge's stdin, once the bridge has read all of it.
    pub async fn config_read_from_its_stdin(&self) -> serde_json::Value {
        // The pid is recorded before stdin is read, so a bridge with a pid has a config on its way.
        self.pid().await;
        eventually(
            "the bridge has read its whole config",
            A_BRIDGE_RECORDS_ITS_PID_WITHIN,
            || {
                let written = std::fs::read(&self.config_file).map_err(|e| {
                    format!("{} is not readable yet: {e}", self.config_file.display())
                })?;
                serde_json::from_slice(&written).map_err(|e| {
                    format!("{} is not whole JSON yet: {e}", self.config_file.display())
                })
            },
        )
        .await
    }

    /// Wait until the bridge process is gone.
    pub async fn has_exited(&self) {
        let pid = self.pid().await;
        self.has_exited_as(pid).await;
    }

    /// Wait until the bridge that ran as `pid` is gone — for a bridge since replaced, whose pid is
    /// no longer the recorded one.
    pub async fn has_exited_as(&self, pid: u32) {
        eventually(
            "the bridge process has exited",
            A_STOPPED_BRIDGE_EXITS_WITHIN,
            || {
                (!process_is_alive(pid))
                    .then_some(())
                    .ok_or_else(|| format!("bridge pid {pid} is still alive"))
            },
        )
        .await
    }

    /// Kill whatever is still running under the recorded pid, so no stand-in outlives its test.
    fn kill_if_running(&self) {
        let Ok(recorded) = std::fs::read_to_string(&self.pid_file) else {
            return;
        };
        let Ok(pid) = recorded.trim().parse::<i32>() else {
            return;
        };
        // SAFETY: signalling a pid affects no memory in this process.
        unsafe { libc::kill(pid, libc::SIGKILL) };
    }
}

/// What the stand-in `x11vnc` does once the daemon has started it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TheX11vnc {
    /// Accepts connections on the `-rfbport` it was given, as a real x11vnc does once it has
    /// attached to its display.
    AcceptsConnections,
    /// Stays up and never accepts: an x11vnc stuck attaching to its display.
    NeverAccepts,
}

/// One start of the stand-in x11vnc: the pid it ran as and the arguments it was given.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct X11vncLaunch {
    pub pid: u32,
    pub argv: Vec<String>,
}

impl X11vncLaunch {
    fn parsed_from(line: &str) -> Option<Self> {
        let mut words = line.split_whitespace();
        let pid = words.next()?.parse().ok()?;
        Some(Self {
            pid,
            argv: words.map(str::to_string).collect(),
        })
    }

    /// The value following `flag`, if the launch was given that flag.
    fn value_of(&self, flag: &str) -> Option<&str> {
        self.argv
            .iter()
            .position(|arg| arg == flag)
            .and_then(|at| self.argv.get(at + 1))
            .map(String::as_str)
    }

    /// The port x11vnc was told to serve on.
    pub fn rfb_port(&self) -> u16 {
        self.value_of("-rfbport")
            .and_then(|port| port.parse().ok())
            .unwrap_or_else(|| panic!("x11vnc was given no -rfbport: {:?}", self.argv))
    }

    pub fn assert_serves_display(&self, display: u32) -> &Self {
        assert_eq!(
            self.value_of("-display"),
            Some(format!(":{display}").as_str()),
            "x11vnc was not pointed at display :{display}: {:?}",
            self.argv
        );
        self
    }

    pub fn assert_listens_on_loopback_only(&self) -> &Self {
        assert!(
            self.argv.iter().any(|arg| arg == "-localhost"),
            "x11vnc would listen beyond loopback: {:?}",
            self.argv
        );
        self
    }
}

/// A stand-in for `x11vnc` that appends `<pid> <argv…>` to a launch log on every start, then
/// sleeps until it is stopped.
///
/// The listening half is done by the test rather than the script, so the stand-in needs no `nc` or
/// `perl` on the machine running the suite: when it [`TheX11vnc::AcceptsConnections`], a watcher
/// thread binds `127.0.0.1:<-rfbport>` for every launch it sees — a bound listener completes TCP
/// handshakes from its backlog, which is all the daemon's readiness check asks of it.
pub struct StandInX11vnc {
    pub binary: PathBuf,
    launch_log: PathBuf,
    stop_watching: Arc<std::sync::atomic::AtomicBool>,
}

/// How long the watcher sleeps between reads of the launch log.
const X11VNC_WATCH_INTERVAL: Duration = Duration::from_millis(20);
/// Safety net for a launch that should happen: a stand-in that has logged nothing by now was never
/// started.
const AN_X11VNC_LOGS_ITS_LAUNCH_WITHIN: Duration = Duration::from_secs(10);

impl StandInX11vnc {
    fn installed_in(dir: &Path, behaviour: TheX11vnc) -> Self {
        let x11vnc = Self {
            binary: dir.join("x11vnc-stand-in"),
            launch_log: dir.join(X11VNC_LAUNCH_LOG),
            stop_watching: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        };
        // One `echo` per launch, so a reader sees whole lines.
        std::fs::write(
            &x11vnc.binary,
            format!(
                "#!/bin/sh\n\
                 [ \"$1\" = {WARM_UP_ARGUMENT} ] && exit 0\n\
                 echo \"$$ $*\" >> \"{log}\"\n\
                 exec sleep {A_BRIDGE_STAYS_UP_FOR_SECS}\n",
                log = x11vnc.launch_log.display()
            ),
        )
        .expect("writing the stand-in x11vnc");
        make_executable(&x11vnc.binary);
        x11vnc.warm_up();
        if behaviour == TheX11vnc::AcceptsConnections {
            x11vnc.listen_for_each_launch();
        }
        x11vnc
    }

    /// Run the freshly written script once, to completion, before any test clock starts — as
    /// [`StandInBridge::warm_up`] does, and for the same reason.
    ///
    /// The first exec of a new executable can take seconds on a loaded macOS machine. A test about
    /// an x11vnc that never accepts gives the start a short accept window, and without this the
    /// daemon gave up on, and killed, a stand-in that had not yet logged its launch, so the test
    /// saw no launch ever happen.
    fn warm_up(&self) {
        let status = std::process::Command::new(&self.binary)
            .arg(WARM_UP_ARGUMENT)
            .status()
            .expect("warming up the stand-in x11vnc");
        assert!(status.success(), "the stand-in x11vnc warm-up: {status}");
    }

    fn listen_for_each_launch(&self) {
        let launch_log = self.launch_log.clone();
        let stop = Arc::clone(&self.stop_watching);
        std::thread::spawn(move || {
            let mut listeners = Vec::new();
            while !stop.load(std::sync::atomic::Ordering::SeqCst) {
                let launches = read_launches(&launch_log);
                for launch in launches.iter().skip(listeners.len()) {
                    listeners.push(
                        std::net::TcpListener::bind(("127.0.0.1", launch.rfb_port()))
                            .expect("listening where x11vnc was told to"),
                    );
                }
                std::thread::sleep(X11VNC_WATCH_INTERVAL);
            }
        });
    }

    /// Every start so far, in order.
    pub fn launches(&self) -> Vec<X11vncLaunch> {
        read_launches(&self.launch_log)
    }

    /// The `nth` start (0-based), once it has happened.
    pub async fn launch(&self, nth: usize) -> X11vncLaunch {
        eventually(
            "x11vnc is started",
            AN_X11VNC_LOGS_ITS_LAUNCH_WITHIN,
            || {
                let launches = self.launches();
                launches
                    .get(nth)
                    .cloned()
                    .ok_or_else(|| format!("{} launches so far: {launches:?}", launches.len()))
            },
        )
        .await
    }

    /// Wait until the process of `launch` is gone.
    pub async fn has_exited(&self, launch: &X11vncLaunch) {
        let pid = launch.pid;
        eventually(
            "the x11vnc process has exited",
            A_STOPPED_BRIDGE_EXITS_WITHIN,
            || {
                (!process_is_alive(pid))
                    .then_some(())
                    .ok_or_else(|| format!("x11vnc pid {pid} is still alive"))
            },
        )
        .await
    }

    /// Check, for `window`, that the process of `launch` stays up — a stop that is coming would
    /// have arrived by then.
    pub async fn keeps_running(&self, launch: &X11vncLaunch, window: Duration) {
        let until = tokio::time::Instant::now() + window;
        while tokio::time::Instant::now() < until {
            assert!(
                process_is_alive(launch.pid),
                "x11vnc pid {} was stopped while its channel was still open",
                launch.pid
            );
            tokio::time::sleep(X11VNC_WATCH_INTERVAL).await;
        }
    }

    fn kill_every_launch(&self) {
        self.stop_watching
            .store(true, std::sync::atomic::Ordering::SeqCst);
        for launch in self.launches() {
            // SAFETY: signalling a pid affects no memory in this process.
            unsafe { libc::kill(launch.pid as i32, libc::SIGKILL) };
        }
    }
}

fn read_launches(launch_log: &Path) -> Vec<X11vncLaunch> {
    std::fs::read_to_string(launch_log)
        .unwrap_or_default()
        .lines()
        .filter_map(X11vncLaunch::parsed_from)
        .collect()
}

/// The operator at the browser: answers every desktop-password question with the empty password,
/// encrypted under this host's published key exactly as the browser's `encryptForHost` does.
///
/// The registry underneath is the real [`InMemoryHostPromptRegistry`]; only who answers is stood in
/// for.
struct AnOperatorWhoTypesNoPassword {
    registry: InMemoryHostPromptRegistry,
    keypair: Arc<FileHostKeypair>,
}

impl HostPromptRegistry for AnOperatorWhoTypesNoPassword {
    fn issue(
        &self,
        issued_for: &str,
        kind: PromptKind,
        subject: &str,
        now_unix_ms: i64,
    ) -> PendingPrompt {
        let issued = self.registry.issue(issued_for, kind, subject, now_unix_ms);
        let ciphertext = encrypted_under(&self.keypair, NO_PASSWORD);
        self.registry
            .answer(&issued.prompt_id, THE_OS_USER, ciphertext, now_unix_ms)
            .expect("the operator answers the question raised for them");
        issued
    }

    fn pending(&self, issued_for: &str, now_unix_ms: i64) -> Vec<PendingPrompt> {
        self.registry.pending(issued_for, now_unix_ms)
    }

    fn answer(
        &self,
        prompt_id: &str,
        answered_by: &str,
        encrypted_answer: Vec<u8>,
        now_unix_ms: i64,
    ) -> Result<(), AnswerRejection> {
        self.registry
            .answer(prompt_id, answered_by, encrypted_answer, now_unix_ms)
    }

    fn awaited_answer(&self, prompt_id: &str) -> Option<AnswerHandoff> {
        self.registry.awaited_answer(prompt_id)
    }

    fn subscribe(&self) -> tokio::sync::broadcast::Receiver<PendingPrompt> {
        self.registry.subscribe()
    }
}

/// A password as the browser sends it: RSA-OAEP(SHA-256) under this host's published key.
fn encrypted_under(keypair: &FileHostKeypair, password: &str) -> Vec<u8> {
    use rsa::pkcs8::DecodePublicKey;

    let PublishedKey { spki_der, .. } = keypair.published().expect("this host's published key");
    rsa::RsaPublicKey::from_public_key_der(&spki_der)
        .expect("a published SPKI DER public key")
        .encrypt(
            &mut rand::thread_rng(),
            rsa::Oaep::new::<sha2::Sha256>(),
            password.as_bytes(),
        )
        .expect("a password fits comfortably in an OAEP payload")
}

/// The session's credential store, in memory. Storage only: a desktop's record goes through the
/// real [`record_for`] and [`target_from`].
#[derive(Default)]
struct TheSessionsCredentials {
    records: Mutex<Vec<tddy_credentials::CredentialRecord>>,
}

impl TheSessionsCredentials {
    fn admits(&self, session_token: &str) -> Result<(), TargetError> {
        (session_token == A_SESSION_TOKEN)
            .then_some(())
            .ok_or(TargetError::NoSuchSession)
    }
}

impl ScreenSharingTargetStore for TheSessionsCredentials {
    fn list(&self, session_token: &str) -> Result<Vec<ProtoScreenSharingTarget>, TargetError> {
        self.admits(session_token)?;
        self.records
            .lock()
            .expect("the session's credentials")
            .iter()
            .map(target_from)
            .collect()
    }

    fn add(
        &self,
        session_token: &str,
        target: &ProtoScreenSharingTarget,
        password: &str,
    ) -> Result<ProtoScreenSharingTarget, TargetError> {
        self.admits(session_token)?;
        let mut records = self.records.lock().expect("the session's credentials");
        let mut stored = target.clone();
        stored.id = format!("target-{}", records.len());
        records.push(record_for(&stored, password, A_WRITE_CLOCK));
        Ok(stored)
    }

    fn remove(&self, session_token: &str, target_id: &str) -> Result<(), TargetError> {
        self.admits(session_token)?;
        self.records
            .lock()
            .expect("the session's credentials")
            .retain(|record| record.account != account_for(target_id));
        Ok(())
    }

    fn password_for(&self, session_token: &str, target_id: &str) -> Result<String, TargetError> {
        self.admits(session_token)?;
        self.records
            .lock()
            .expect("the session's credentials")
            .iter()
            .find(|record| record.account == account_for(target_id))
            .map(|record| record.secret.expose().to_string())
            .ok_or_else(|| TargetError::Malformed(format!("no target {target_id}")))
    }
}

/// The service wired the way `runtime.rs` wires it, over a throwaway directory, with its bridge
/// binary pointed at a [`StandInBridge`].
pub struct DaemonUnderTest {
    service: ScreenSharingServiceImpl,
    pub bridge: StandInBridge,
    pub x11vnc: StandInX11vnc,
    storage: tempfile::TempDir,
}

/// Builds a [`DaemonUnderTest`] whose bridge behaves as given.
pub struct DaemonBuilder {
    bridge: TheBridge,
    ready_timeout: Duration,
    x11vnc: TheX11vnc,
    x11vnc_accept_timeout: Duration,
}

/// A daemon whose bridge does `bridge` once it is spawned.
pub fn a_daemon_whose_bridge(bridge: TheBridge) -> DaemonBuilder {
    DaemonBuilder {
        bridge,
        ready_timeout: DEFAULT_BRIDGE_READY_TIMEOUT,
        x11vnc: TheX11vnc::AcceptsConnections,
        x11vnc_accept_timeout: DEFAULT_X11VNC_ACCEPT_TIMEOUT,
    }
}

impl DaemonBuilder {
    /// Wait at most `timeout` for the bridge's readiness line — production configuration, set
    /// short here so a test about a silent bridge costs a moment rather than the default wait.
    pub fn waiting_for_readiness_at_most(mut self, timeout: Duration) -> Self {
        self.ready_timeout = timeout;
        self
    }

    /// The installed x11vnc does `x11vnc` once it is started.
    pub fn with_an_x11vnc_that(mut self, x11vnc: TheX11vnc) -> Self {
        self.x11vnc = x11vnc;
        self
    }

    /// Wait at most `timeout` for x11vnc to accept — set short here so a test about one that never
    /// does costs a moment rather than the default wait.
    pub fn waiting_for_x11vnc_at_most(mut self, timeout: Duration) -> Self {
        self.x11vnc_accept_timeout = timeout;
        self
    }

    pub fn start(self) -> DaemonUnderTest {
        let storage = tempfile::tempdir().expect("a storage directory");
        let bridge = StandInBridge::installed_in(storage.path(), &self.bridge);
        let x11vnc = StandInX11vnc::installed_in(storage.path(), self.x11vnc);

        let config = Arc::new(DaemonConfig {
            livekit: Some(LiveKitConfig {
                url: Some("ws://livekit.internal:7880".to_string()),
                public_url: Some("wss://livekit.example.com".to_string()),
                api_key: Some("an-api-key".to_string()),
                api_secret: Some("an-api-secret".to_string()),
                common_room: Some(THE_COMMON_ROOM.to_string()),
                ..Default::default()
            }),
            screen_sharing: Some(ScreenSharingConfig {
                vnc_binary_path: bridge.binary.display().to_string(),
                x11vnc_binary_path: x11vnc.binary.display().to_string(),
                ..Default::default()
            }),
            ..Default::default()
        });

        let sessions_base = storage.path().to_path_buf();
        let keypair = Arc::new(FileHostKeypair::new(storage.path()));
        let targets = Arc::new(FileHostDesktopTargetStore::new(storage.path()));
        let operator = Arc::new(AnOperatorWhoTypesNoPassword {
            registry: InMemoryHostPromptRegistry::new(),
            keypair: Arc::clone(&keypair),
        });

        let service = ScreenSharingServiceImpl::new(
            Arc::new(|token: &str| (token == A_SESSION_TOKEN).then(|| THE_OS_USER.to_string())),
            Arc::new(move |_user: &str| Some(sessions_base.clone())),
        )
        .with_config(config)
        .with_room_tokens(Arc::new(AMinterThatAlwaysMints))
        .with_host_scope(
            targets,
            keypair as Arc<dyn HostKeypair>,
            operator as Arc<dyn HostPromptRegistry>,
        )
        .with_target_store(Arc::new(TheSessionsCredentials::default()))
        .with_bridge_ready_timeout(self.ready_timeout)
        .with_x11vnc_accept_timeout(self.x11vnc_accept_timeout);

        DaemonUnderTest {
            service,
            bridge,
            x11vnc,
            storage,
        }
    }
}

impl DaemonUnderTest {
    /// Attach a password-less VNC desktop to `host`, as the Hosts screen does.
    pub async fn attach_a_desktop_to(&self, host: &str) -> String {
        self.service
            .add_host_target(Request::direct(AddHostTargetRequest {
                session_token: A_SESSION_TOKEN.to_string(),
                daemon_instance_id: host.to_string(),
                label: "dev box".to_string(),
                host: "127.0.0.1".to_string(),
                port: A_VNC_PORT,
                protocol: Protocol::Vnc as i32,
                username: THE_OS_USER.to_string(),
                x11_display: None,
            }))
            .await
            .expect("attaching a desktop to a host")
            .into_inner()
            .target_id
    }

    /// Attach X11 display `:display` of `host` as a desktop, as the Hosts screen does when an
    /// operator presses Connect on that display's channel.
    pub async fn attach_an_x11_display_to(&self, host: &str, display: u32) -> String {
        self.service
            .add_host_target(Request::direct(AddHostTargetRequest {
                session_token: A_SESSION_TOKEN.to_string(),
                daemon_instance_id: host.to_string(),
                label: format!("Display :{display}"),
                host: "127.0.0.1".to_string(),
                port: 0,
                protocol: Protocol::Vnc as i32,
                username: String::new(),
                x11_display: Some(display),
            }))
            .await
            .expect("attaching an X11 display to a host")
            .into_inner()
            .target_id
    }

    /// Close a host's open desktop, as the viewer does when the operator closes it.
    pub async fn close_the_desktop_of(&self, host: &str, target_id: &str) {
        self.service
            .stop_host_stream(Request::direct(StopHostStreamRequest {
                session_token: A_SESSION_TOKEN.to_string(),
                daemon_instance_id: host.to_string(),
                target_id: target_id.to_string(),
            }))
            .await
            .expect("closing a host's desktop");
    }

    /// Open a host's desktop and hand back whatever the start came to, refusal included.
    ///
    /// Bounded, so a start that never finishes fails the test instead of hanging the suite.
    pub async fn open_the_desktop_of(
        &self,
        host: &str,
        target_id: &str,
    ) -> Result<StartStreamResponse, Status> {
        tokio::time::timeout(
            A_START_FINISHES_WITHIN,
            self.service
                .start_host_stream(Request::direct(StartHostStreamRequest {
                    session_token: A_SESSION_TOKEN.to_string(),
                    daemon_instance_id: host.to_string(),
                    target_id: target_id.to_string(),
                })),
        )
        .await
        .expect("the start finishes, one way or the other")
        .map(Response::into_inner)
    }

    /// A session holding one password-less desktop in its credential store.
    pub async fn a_session_holding_a_desktop(&self) -> String {
        let session_dir = self.storage.path().join("sessions").join(A_SESSION);
        std::fs::create_dir_all(&session_dir).expect("a session directory");
        write_initial_tool_session_metadata(
            &session_dir,
            InitialToolSessionMetadataOpts {
                project_id: "a-project".to_string(),
                livekit_room: Some(THE_SESSIONS_OWN_ROOM.to_string()),
                ..Default::default()
            },
        )
        .expect("a session's metadata");

        self.service
            .add_target(Request::direct(AddTargetRequest {
                session_token: A_SESSION_TOKEN.to_string(),
                session_id: A_SESSION.to_string(),
                label: "dev box".to_string(),
                host: "127.0.0.1".to_string(),
                port: A_VNC_PORT,
                password: String::new(),
                protocol: Protocol::Vnc as i32,
                username: THE_OS_USER.to_string(),
            }))
            .await
            .expect("adding a desktop to the session's credentials")
            .into_inner()
            .target
            .expect("the added target")
            .id
    }

    /// Open a session's desktop and hand back whatever the start came to, refusal included.
    pub async fn open_the_session_desktop(
        &self,
        target_id: &str,
    ) -> Result<StartStreamResponse, Status> {
        tokio::time::timeout(
            A_START_FINISHES_WITHIN,
            self.service
                .start_stream(Request::direct(StartStreamRequest {
                    session_token: A_SESSION_TOKEN.to_string(),
                    session_id: A_SESSION.to_string(),
                    target_id: target_id.to_string(),
                })),
        )
        .await
        .expect("the start finishes, one way or the other")
        .map(Response::into_inner)
    }
}

impl Drop for DaemonUnderTest {
    /// A stand-in left sleeping is a bad neighbour to every run after it.
    fn drop(&mut self) {
        self.bridge.kill_if_running();
        self.x11vnc.kill_every_launch();
    }
}

/// Mints a fixed token for any room: these suites are about what the daemon does with a bridge,
/// not about LiveKit, so the daemon is always able to hand its bridge a token.
struct AMinterThatAlwaysMints;

impl tddy_daemon_extensions::RoomTokenMinter for AMinterThatAlwaysMints {
    fn mint(&self, _room: &str, _identity: &str, _ttl: Duration) -> Result<String, Status> {
        Ok("a-minted-bridge-token".to_string())
    }
}
