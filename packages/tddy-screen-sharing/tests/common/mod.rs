//! A daemon's screen-sharing service wired over throwaway storage, with the bridge binary pointed
//! at a `/bin/sh` stand-in whose behaviour each test scripts.
//!
//! A minimal re-creation of the fixtures in `screen_sharing_service.rs`'s inline `mod tests`
//! (`FakeBridge`, `AnOperatorAtTheKeyboard`, `TheSessionsCredentials`, `DaemonUnderTest`), kept
//! outside the service file so `#host-desktops` node 1's split does not move it. Only what drives
//! the public surface is here: everything below the RPC is the production code path.
//!
//! `#host-desktops` 8/8 adds the sign-in half: a stand-in that also answers `--inspect`, an
//! operator who answers a `DESKTOP_CREDENTIALS` question with a structured credential, and — for
//! the Remember tests — the operator's real sealed credential vault in place of the in-memory
//! session store, so what is and is not written is read back from the vault itself.

#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use prost::Message as _;
use tddy_core::session_metadata::{
    write_initial_tool_session_metadata, InitialToolSessionMetadataOpts,
};
use tddy_credentials::{AccountId, CredentialRecord, CredentialStore, SecretString, SessionVaults};
use tddy_daemon_extensions::bridge_readiness::{BridgeReadiness, FailureKind};
use tddy_daemon_extensions::desktop_inspect::{AdvertisedMethod, InspectReport};
use tddy_daemon_kernel::config::{DaemonConfig, LiveKitConfig, ScreenSharingConfig};
use tddy_host_service::host_desktop_targets::FileHostDesktopTargetStore;
use tddy_host_service::host_keypair::{FileHostKeypair, HostKeypair, PublishedKey};
use tddy_host_service::host_prompts::{
    AnswerHandoff, AnswerRejection, HostPromptRegistry, InMemoryHostPromptRegistry, PendingPrompt,
    PromptKind,
};
use tddy_rpc::{Request, Response, Status};
use tddy_screen_sharing::screen_sharing_records::{
    account_for, record_for, screen_sharing_provider, target_from, ScreenSharingTargetStore,
    TargetError,
};
use tddy_screen_sharing::{
    ScreenSharingServiceImpl, DEFAULT_BRIDGE_READY_TIMEOUT, DEFAULT_X11VNC_ACCEPT_TIMEOUT,
};
use tddy_service::proto::screen_sharing::{
    AddHostTargetRequest, AddTargetRequest, DesktopAuthMethod, DesktopCredentialAnswer,
    InspectHostDesktopRequest, InspectHostDesktopResponse, Protocol, ScreenSharingService,
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
pub const A_VNC_PORT: u32 = 5900;
pub const AN_RDP_PORT: u32 = 3389;
/// Unless a test says otherwise every desktop here is password-less, so that is what the operator
/// types.
const NO_PASSWORD: &str = "";
/// What the operator's credential vault is sealed under.
const THE_OPERATORS_VAULT_PASSPHRASE: &str = "the-passphrase-the-operator-chose";
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
    /// Writes this readiness line, stays up for [`A_REPORTED_BRIDGE_EXITS_AFTER_SECS`], then exits
    /// on its own: a bridge that came up and later crashed.
    ReportsThenExits(BridgeReadiness),
}

/// How long a bridge that [`TheBridge::ReportsThenExits`] stays up after its readiness line: long
/// enough that the daemon has read that line and the start has returned, so the exit is a running
/// bridge's rather than a failed start's.
const A_REPORTED_BRIDGE_EXITS_AFTER_SECS: &str = "0.5";

impl TheBridge {
    pub fn ready_at(width: u32, height: u32) -> Self {
        Self::Reports(BridgeReadiness::Ready { width, height })
    }

    /// Ready at this size, then gone on its own shortly after.
    pub fn ready_at_then_exits(width: u32, height: u32) -> Self {
        Self::ReportsThenExits(BridgeReadiness::Ready { width, height })
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
///
/// Run with `--inspect` it instead records the request on its stdin, prints the [`InspectReport`]
/// line it was installed with and exits — the bridge's inspect mode — without touching the pid or
/// config it records as a bridge.
pub struct StandInBridge {
    pub binary: PathBuf,
    pid_file: PathBuf,
    /// What the daemon wrote to the bridge's stdin, the last time it was started as a bridge.
    config_file: PathBuf,
    /// Every config the daemon wrote, one JSON document per line, in start order.
    configs_log: PathBuf,
    /// What the daemon wrote to the bridge's stdin in inspect mode.
    inspect_request_file: PathBuf,
    /// The report line it prints in inspect mode.
    inspection_line: String,
}

impl StandInBridge {
    fn installed_in(dir: &Path, behaviour: &TheBridge, inspection: &InspectReport) -> Self {
        let bridge = Self {
            binary: dir.join("tddy-vnc-stand-in"),
            pid_file: dir.join("bridge-pid"),
            config_file: dir.join("bridge-config.json"),
            configs_log: dir.join("bridge-configs.jsonl"),
            inspect_request_file: dir.join("bridge-inspect-request.json"),
            inspection_line: serde_json::to_string(inspection).expect("an inspect report line"),
        };
        bridge.now_does(behaviour);
        bridge.warm_up();
        bridge
    }

    /// Re-script what the bridge does from its next start on — a desktop whose password changed
    /// between two opens, say. A bridge already running keeps doing what it was started doing.
    pub fn now_does(&self, behaviour: &TheBridge) {
        let pid = self.pid_file.display();
        let config = self.config_file.display();
        let configs = self.configs_log.display();
        let inspect_request = self.inspect_request_file.display();
        let inspection = &self.inspection_line;
        let after_config = match behaviour {
            TheBridge::Reports(readiness) => {
                let line = serde_json::to_string(readiness).expect("a readiness line");
                format!("printf '%s\\n' '{line}'\nexec sleep {A_BRIDGE_STAYS_UP_FOR_SECS}\n")
            }
            TheBridge::NeverReports => format!("exec sleep {A_BRIDGE_STAYS_UP_FOR_SECS}\n"),
            TheBridge::ExitsBeforeReporting => "exit 1\n".to_string(),
            TheBridge::ReportsThenExits(readiness) => {
                let line = serde_json::to_string(readiness).expect("a readiness line");
                format!(
                    "printf '%s\\n' '{line}'\nsleep {A_REPORTED_BRIDGE_EXITS_AFTER_SECS}\nexit 1\n"
                )
            }
            // The x11vnc is found by the port in the bridge's config: the launch log line naming
            // `-rfbport <that port>` carries its pid. `-rfbport` is never x11vnc's last argument,
            // so the trailing space keeps one port from matching another that it prefixes.
            TheBridge::ExitsWithItsX11vnc(readiness) => {
                let line = serde_json::to_string(readiness).expect("a readiness line");
                // The stand-in x11vnc logs its launches into the directory the bridge is installed in.
                let launches = self
                    .binary
                    .parent()
                    .expect("the stand-in bridge is installed in a directory")
                    .join(X11VNC_LAUNCH_LOG);
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
        // or none. Each config is appended to the log followed by a newline, so a reader counting
        // whole lines sees whole configs.
        std::fs::write(
            &self.binary,
            format!(
                "#!/bin/sh\n\
                 [ \"$1\" = {WARM_UP_ARGUMENT} ] && exit 0\n\
                 if [ \"$1\" = \"--inspect\" ]; then\n\
                 cat > \"{inspect_request}\"\n\
                 printf '%s\\n' '{inspection}'\n\
                 exit 0\n\
                 fi\n\
                 echo $$ > \"{pid}.part\"\n\
                 mv -f \"{pid}.part\" \"{pid}\"\n\
                 cat > \"{config}\"\n\
                 cat \"{config}\" >> \"{configs}\"\n\
                 echo >> \"{configs}\"\n\
                 {after_config}"
            ),
        )
        .expect("writing the stand-in bridge");
        make_executable(&self.binary);
    }

    /// The config the daemon wrote to the `nth` bridge it started (0-based), once all of it is in.
    pub async fn nth_config_read_from_its_stdin(&self, nth: usize) -> serde_json::Value {
        eventually(
            "the bridge has been started and has read its whole config",
            A_BRIDGE_RECORDS_ITS_PID_WITHIN,
            || {
                let log = std::fs::read_to_string(&self.configs_log).unwrap_or_default();
                let whole_lines: Vec<&str> = log
                    .split_inclusive('\n')
                    .filter(|line| line.ends_with('\n'))
                    .collect();
                let line = whole_lines.get(nth).ok_or_else(|| {
                    format!("{} bridge configs so far: {log:?}", whole_lines.len())
                })?;
                serde_json::from_str(line.trim_end())
                    .map_err(|e| format!("config {nth} is not JSON: {e}: {line:?}"))
            },
        )
        .await
    }

    /// What the daemon wrote to the bridge's stdin when it last ran it in inspect mode.
    pub fn inspect_request(&self) -> serde_json::Value {
        let written = std::fs::read(&self.inspect_request_file).unwrap_or_else(|e| {
            panic!(
                "the bridge was never run in inspect mode ({}: {e})",
                self.inspect_request_file.display()
            )
        });
        serde_json::from_slice(&written).expect("the inspect request is JSON")
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

/// What the operator at the browser answers every question this host raises with.
#[derive(Debug, Clone)]
pub enum TheOperator {
    /// The empty password: every desktop here is password-less unless a test says otherwise.
    TypesNoPassword,
    /// A structured credential, as the sign-in form sends it for a `DESKTOP_CREDENTIALS` question:
    /// the protobuf-encoded [`DesktopCredentialAnswer`].
    SignsInWith(DesktopCredentialAnswer),
}

impl TheOperator {
    /// The plaintext the browser encrypts.
    fn plaintext(&self) -> Vec<u8> {
        match self {
            Self::TypesNoPassword => NO_PASSWORD.as_bytes().to_vec(),
            Self::SignsInWith(answer) => answer.encode_to_vec(),
        }
    }
}

/// The operator at the browser: answers every question this host raises at once, encrypted under
/// this host's published key exactly as the browser's `encryptForHost` does, and remembers every
/// question it was asked.
///
/// The registry underneath is the real [`InMemoryHostPromptRegistry`]; only who answers is stood in
/// for.
struct AnOperatorAtTheBrowser {
    registry: InMemoryHostPromptRegistry,
    keypair: Arc<FileHostKeypair>,
    answers_with: TheOperator,
    asked: Mutex<Vec<PendingPrompt>>,
}

impl HostPromptRegistry for AnOperatorAtTheBrowser {
    fn issue(
        &self,
        issued_for: &str,
        kind: PromptKind,
        subject: &str,
        now_unix_ms: i64,
    ) -> PendingPrompt {
        let issued = self.registry.issue(issued_for, kind, subject, now_unix_ms);
        self.asked
            .lock()
            .expect("the questions asked")
            .push(issued.clone());
        let ciphertext = encrypted_under(&self.keypair, &self.answers_with.plaintext());
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

/// An answer as the browser sends it: RSA-OAEP(SHA-256) under this host's published key.
fn encrypted_under(keypair: &FileHostKeypair, plaintext: &[u8]) -> Vec<u8> {
    use rsa::pkcs8::DecodePublicKey;

    let PublishedKey { spki_der, .. } = keypair.published().expect("this host's published key");
    rsa::RsaPublicKey::from_public_key_der(&spki_der)
        .expect("a published SPKI DER public key")
        .encrypt(
            &mut rand::thread_rng(),
            rsa::Oaep::new::<sha2::Sha256>(),
            plaintext,
        )
        .expect("an answer fits comfortably in an OAEP payload")
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
    operator: Arc<AnOperatorAtTheBrowser>,
    /// The operator's sealed credential vault, when the daemon was built over one.
    vaults: Option<Arc<SessionVaults>>,
    /// Everything the daemon writes — and nothing else.
    storage: tempfile::TempDir,
    /// The stand-in binaries and what they record, kept apart from [`Self::storage`] so a search of
    /// what the daemon wrote does not find what the test's own bridge wrote down.
    tools: tempfile::TempDir,
}

/// Builds a [`DaemonUnderTest`] whose bridge behaves as given.
pub struct DaemonBuilder {
    bridge: TheBridge,
    ready_timeout: Duration,
    x11vnc: TheX11vnc,
    x11vnc_accept_timeout: Duration,
    inspection: InspectReport,
    operator: TheOperator,
    the_operators_vault: bool,
    /// Whether the operator's vault, when wired, is left sealed rather than unlocked.
    the_operators_vault_locked: bool,
}

/// A daemon whose bridge does `bridge` once it is spawned.
pub fn a_daemon_whose_bridge(bridge: TheBridge) -> DaemonBuilder {
    DaemonBuilder {
        bridge,
        ready_timeout: DEFAULT_BRIDGE_READY_TIMEOUT,
        x11vnc: TheX11vnc::AcceptsConnections,
        x11vnc_accept_timeout: DEFAULT_X11VNC_ACCEPT_TIMEOUT,
        inspection: InspectReport {
            methods: vec![AdvertisedMethod::VncPassword],
        },
        operator: TheOperator::TypesNoPassword,
        the_operators_vault: false,
        the_operators_vault_locked: false,
    }
}

impl DaemonBuilder {
    /// The bridge, run in inspect mode, reports that the desktop advertises `methods`.
    pub fn whose_desktop_advertises(mut self, methods: Vec<AdvertisedMethod>) -> Self {
        self.inspection = InspectReport { methods };
        self
    }

    /// The operator answers every question with `answer`.
    pub fn whose_operator_signs_in_with(mut self, answer: DesktopCredentialAnswer) -> Self {
        self.operator = TheOperator::SignsInWith(answer);
        self
    }

    /// Wire the operator's real credential vault — sealed on disk under this daemon's storage and
    /// unlocked — instead of the in-memory session store, as `runtime.rs` wires
    /// `with_credential_vaults`.
    pub fn with_the_operators_vault(mut self) -> Self {
        self.the_operators_vault = true;
        self
    }

    /// Wire the operator's real credential vault, sealed on disk but not unlocked — the state it is
    /// in after a restart, before the operator has opened it again.
    pub fn with_the_operators_vault_locked(mut self) -> Self {
        self.the_operators_vault = true;
        self.the_operators_vault_locked = true;
        self
    }

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
        let tools = tempfile::tempdir().expect("a directory for the stand-in binaries");
        let bridge = StandInBridge::installed_in(tools.path(), &self.bridge, &self.inspection);
        let x11vnc = StandInX11vnc::installed_in(tools.path(), self.x11vnc);

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
                rdp_binary_path: bridge.binary.display().to_string(),
                x11vnc_binary_path: x11vnc.binary.display().to_string(),
                ..Default::default()
            }),
            ..Default::default()
        });

        let sessions_base = storage.path().to_path_buf();
        let keypair = Arc::new(FileHostKeypair::new(storage.path()));
        let targets = Arc::new(FileHostDesktopTargetStore::new(storage.path()));
        let operator = Arc::new(AnOperatorAtTheBrowser {
            registry: InMemoryHostPromptRegistry::new(),
            keypair: Arc::clone(&keypair),
            answers_with: self.operator,
            asked: Mutex::new(Vec::new()),
        });
        let vaults = self.the_operators_vault.then(|| {
            if self.the_operators_vault_locked {
                the_operators_locked_vault_in(storage.path())
            } else {
                the_operators_unlocked_vault_in(storage.path())
            }
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
            Arc::clone(&operator) as Arc<dyn HostPromptRegistry>,
        )
        .with_bridge_ready_timeout(self.ready_timeout)
        .with_x11vnc_accept_timeout(self.x11vnc_accept_timeout);
        let service = match &vaults {
            Some(vaults) => service.with_credential_vaults(Some(Arc::clone(vaults))),
            None => service.with_target_store(Arc::new(TheSessionsCredentials::default())),
        };

        DaemonUnderTest {
            service,
            bridge,
            x11vnc,
            operator,
            vaults,
            storage,
            tools,
        }
    }
}

/// The operator's credential vault, created on disk under `storage` and unlocked — the state a
/// signed-in operator's vault is in.
fn the_operators_unlocked_vault_in(storage: &Path) -> Arc<SessionVaults> {
    let vaults = Arc::new(SessionVaults::new(storage.join("auth")));
    let passphrase = SecretString::new(THE_OPERATORS_VAULT_PASSPHRASE);
    CredentialStore::create(&vaults.path_for(THE_OS_USER), &passphrase, THE_OS_USER)
        .expect("the operator's vault is created");
    vaults
        .unlock(THE_OS_USER, &passphrase)
        .expect("the operator's passphrase opens the vault");
    vaults
}

/// The operator's credential vault, created on disk under `storage` and left sealed.
fn the_operators_locked_vault_in(storage: &Path) -> Arc<SessionVaults> {
    let vaults = Arc::new(SessionVaults::new(storage.join("auth")));
    let passphrase = SecretString::new(THE_OPERATORS_VAULT_PASSPHRASE);
    CredentialStore::create(&vaults.path_for(THE_OS_USER), &passphrase, THE_OS_USER)
        .expect("the operator's vault is created");
    vaults
}

/// A credential as the sign-in form sends it: VNC Authentication with no password, not
/// remembered, until a test says otherwise.
pub fn a_credential() -> DesktopCredentialAnswer {
    DesktopCredentialAnswer {
        method: DesktopAuthMethod::VncPassword as i32,
        ..Default::default()
    }
}

/// Fluent setters for [`a_credential`].
pub trait CredentialBuilder: Sized {
    fn for_method(self, method: DesktopAuthMethod) -> Self;
    fn with_username(self, username: &str) -> Self;
    fn with_domain(self, domain: &str) -> Self;
    fn with_password(self, password: &str) -> Self;
    /// With Remember ticked.
    fn remembered(self) -> Self;
}

impl CredentialBuilder for DesktopCredentialAnswer {
    fn for_method(mut self, method: DesktopAuthMethod) -> Self {
        self.method = method as i32;
        self
    }

    fn with_username(mut self, username: &str) -> Self {
        self.username = username.to_string();
        self
    }

    fn with_domain(mut self, domain: &str) -> Self {
        self.domain = domain.to_string();
        self
    }

    fn with_password(mut self, password: &str) -> Self {
        self.password = password.to_string();
        self
    }

    fn remembered(mut self) -> Self {
        self.remember = true;
        self
    }
}

impl DaemonUnderTest {
    /// Attach an RDP desktop to `host`, as the Hosts screen does for an RDP channel.
    pub async fn attach_an_rdp_desktop_to(&self, host: &str) -> String {
        self.service
            .add_host_target(Request::direct(AddHostTargetRequest {
                session_token: A_SESSION_TOKEN.to_string(),
                daemon_instance_id: host.to_string(),
                label: "windows box".to_string(),
                host: "127.0.0.1".to_string(),
                port: AN_RDP_PORT,
                protocol: Protocol::Rdp as i32,
                username: String::new(),
                x11_display: None,
                domain: String::new(),
            }))
            .await
            .expect("attaching an RDP desktop to a host")
            .into_inner()
            .target_id
    }

    /// Ask which sign-in methods a host's desktop advertises, as the viewer does before it opens it.
    ///
    /// Bounded, so an inspection that never finishes fails the test instead of hanging the suite.
    pub async fn inspect_the_desktop_of(
        &self,
        host: &str,
        target_id: &str,
    ) -> Result<InspectHostDesktopResponse, Status> {
        tokio::time::timeout(
            A_START_FINISHES_WITHIN,
            self.service
                .inspect_host_desktop(Request::direct(InspectHostDesktopRequest {
                    session_token: A_SESSION_TOKEN.to_string(),
                    daemon_instance_id: host.to_string(),
                    target_id: target_id.to_string(),
                })),
        )
        .await
        .expect("the inspection finishes, one way or the other")
        .map(Response::into_inner)
    }

    /// Every question this host has asked its operator, in order.
    pub fn questions_asked(&self) -> Vec<PendingPrompt> {
        self.operator
            .asked
            .lock()
            .expect("the questions asked")
            .clone()
    }

    /// The `screen-sharing` record under `account` in the operator's credential vault, if any.
    pub fn the_operators_screen_sharing_record(&self, account: &str) -> Option<CredentialRecord> {
        self.the_operators_vault()
            .get(&screen_sharing_provider(), &AccountId::new(account))
            .expect("the operator's vault is readable")
    }

    /// Every `screen-sharing` record in the operator's credential vault.
    pub fn the_operators_screen_sharing_records(&self) -> Vec<CredentialRecord> {
        self.the_operators_vault()
            .list(Some(&screen_sharing_provider()))
            .expect("the operator's vault is readable")
    }

    fn the_operators_vault(&self) -> Arc<tddy_credentials::SessionVault> {
        self.vaults
            .as_ref()
            .expect("this daemon was built with_the_operators_vault()")
            .use_open(THE_OS_USER)
            .expect("the operator's vault is open")
    }

    /// Every file under the daemon's storage whose bytes contain `needle`.
    pub fn files_the_daemon_wrote_holding(&self, needle: &str) -> Vec<PathBuf> {
        files_under(self.storage.path())
            .into_iter()
            .filter(|path| {
                std::fs::read(path)
                    .map(|bytes| bytes.windows(needle.len()).any(|w| w == needle.as_bytes()))
                    .unwrap_or(false)
            })
            .collect()
    }

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
                domain: String::new(),
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
                domain: String::new(),
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

fn files_under(dir: &Path) -> Vec<PathBuf> {
    std::fs::read_dir(dir)
        .map(|entries| {
            entries
                .flatten()
                .flat_map(|entry| {
                    let path = entry.path();
                    if path.is_dir() {
                        files_under(&path)
                    } else {
                        vec![path]
                    }
                })
                .collect()
        })
        .unwrap_or_default()
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
