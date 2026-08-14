//! Acceptance-test harness for the real `tddy-supervisor` binary.
//!
//! Tests drive the actual process over its actual socket. The only thing that differs from a
//! production host is what the config points at: managed services are shell scripts in a temp
//! directory, and the cgroup base is a temp directory instead of `/sys/fs/cgroup`. The
//! supervisor takes the same code path either way.
//!
//! Privilege drop is a no-op in these tests because the config declares the user that is
//! already running them — everything else (fork, exec, reap, backoff, socket bind, peer
//! credential authorization, RPC) is real.
//!
//! ⚠️ Which is why every test that makes the supervisor *spawn* something carries
//! `#[cfg(target_os = "linux")]`. The first step of every pre-exec plan is `PR_SET_PDEATHSIG`, and
//! off Linux `spawn_broker` refuses it rather than skipping it — a child it cannot tie to the
//! supervisor's lifetime is one it will not start — so the spawn fails by design. The gates mark
//! the tests whose subject is a live child; everything the supervisor decides without forking
//! (peer authorization, policy denials, scope bookkeeping, config) is asserted on every host.

#![allow(dead_code)]

use std::collections::BTreeMap;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use tddy_supervisor::{
    AppliedLimits, ScopeHandle, ServiceState, ServiceStatus, SupervisorClient, SupervisorError,
};
use tempfile::TempDir;

const READY_TIMEOUT: Duration = Duration::from_secs(10);
const SETTLE_TIMEOUT: Duration = Duration::from_secs(15);
const POLL_INTERVAL: Duration = Duration::from_millis(25);

// ---------------------------------------------------------------------------------------------
// Managed-service fixtures
// ---------------------------------------------------------------------------------------------

/// Declaration of a service the supervisor should manage, plus the script that backs it.
pub struct ServiceFixture {
    name: String,
    body: String,
    max_retries: u32,
    initial_backoff_ms: u64,
    stability_threshold_ms: u64,
    declares_socket: bool,
}

/// A managed service that stays alive until something kills it.
pub fn a_service(name: &str) -> ServiceFixture {
    ServiceFixture {
        name: name.to_string(),
        body: "exec sleep 600".to_string(),
        max_retries: 5,
        initial_backoff_ms: 20,
        stability_threshold_ms: 10_000,
        declares_socket: false,
    }
}

impl ServiceFixture {
    /// Runs until killed. This is the default.
    pub fn that_stays_alive(mut self) -> Self {
        self.body = "exec sleep 600".to_string();
        self
    }

    /// Exits with a failure status the moment it is exec'd.
    pub fn that_exits_immediately(mut self) -> Self {
        self.body = "exit 1".to_string();
        self
    }

    /// Declares a listening socket for the supervisor to create as root and hand over, and makes
    /// the script report what it actually received.
    pub fn with_a_listening_socket(mut self) -> Self {
        self.declares_socket = true;
        self.body = HANDOFF_REPORT_BODY.to_string();
        self
    }

    /// Keeps the reporting body but withdraws the socket declaration, so a test can tell the
    /// difference between "reported nothing" and "was handed nothing".
    pub fn declaring_no_socket(mut self) -> Self {
        self.declares_socket = false;
        self
    }

    pub fn with_max_retries(mut self, max_retries: u32) -> Self {
        self.max_retries = max_retries;
        self
    }

    pub fn with_initial_backoff_ms(mut self, initial_backoff_ms: u64) -> Self {
        self.initial_backoff_ms = initial_backoff_ms;
        self
    }

    /// Uptime after which the supervisor should consider the service healthy and reset backoff.
    pub fn with_stability_threshold_ms(mut self, stability_threshold_ms: u64) -> Self {
        self.stability_threshold_ms = stability_threshold_ms;
        self
    }
}

// ---------------------------------------------------------------------------------------------
// Supervisor fixture
// ---------------------------------------------------------------------------------------------

pub struct SupervisorFixture {
    services: Vec<ServiceFixture>,
    allowed_session_users: Vec<String>,
    allowed_tool_paths: Vec<PathBuf>,
    allowed_mount_roots: Vec<PathBuf>,
    memory_max_ceiling: Option<u64>,
    cpu_max_ceiling: Option<String>,
    pids_max_ceiling: Option<u64>,
    shutdown_grace_secs: u64,
}

/// A supervisor with no declared services and an empty spawn policy — deny-everything defaults.
pub fn a_supervisor() -> SupervisorFixture {
    SupervisorFixture {
        services: Vec::new(),
        allowed_session_users: Vec::new(),
        allowed_tool_paths: Vec::new(),
        allowed_mount_roots: Vec::new(),
        memory_max_ceiling: None,
        cpu_max_ceiling: None,
        pids_max_ceiling: None,
        shutdown_grace_secs: 2,
    }
}

impl SupervisorFixture {
    pub fn managing(mut self, service: ServiceFixture) -> Self {
        self.services.push(service);
        self
    }

    pub fn allowing_session_user(mut self, user: &str) -> Self {
        self.allowed_session_users.push(user.to_string());
        self
    }

    /// Allow sessions to run as whoever is running the tests.
    pub fn allowing_the_current_user(mut self) -> Self {
        self.allowed_session_users.push(current_username());
        self
    }

    pub fn allowing_tool(mut self, tool_path: &Path) -> Self {
        self.allowed_tool_paths.push(tool_path.to_path_buf());
        self
    }

    pub fn allowing_mount_root(mut self, path: &Path) -> Self {
        self.allowed_mount_roots.push(path.to_path_buf());
        self
    }

    pub fn with_memory_ceiling(mut self, bytes: u64) -> Self {
        self.memory_max_ceiling = Some(bytes);
        self
    }

    pub fn with_cpu_ceiling(mut self, cpu_max: &str) -> Self {
        self.cpu_max_ceiling = Some(cpu_max.to_string());
        self
    }

    pub fn with_pids_ceiling(mut self, pids: u64) -> Self {
        self.pids_max_ceiling = Some(pids);
        self
    }

    /// Launch the real binary and wait until its socket accepts connections.
    pub async fn start(self) -> RunningSupervisor {
        let workspace = TempDir::new().expect("create supervisor test workspace");
        let root = workspace.path().to_path_buf();
        let socket_path = root.join("supervisor.sock");
        let cgroup_base = root.join("cgroup");
        fs::create_dir_all(&cgroup_base).expect("create cgroup base");

        let user = current_username();
        let mut service_yaml = String::new();
        for service in &self.services {
            let script = write_service_script(&root, service);
            service_yaml.push_str(&format!(
                concat!(
                    "  - name: {name}\n",
                    "    exec_start: {script}\n",
                    "    user: {user}\n",
                    "    restart:\n",
                    "      max_retries: {max_retries}\n",
                    "      initial_backoff_ms: {initial_backoff_ms}\n",
                    "      max_backoff_ms: 1000\n",
                    "      stability_threshold_ms: {stability_threshold_ms}\n",
                    "{socket}",
                ),
                name = service.name,
                script = script.display(),
                user = user,
                max_retries = service.max_retries,
                initial_backoff_ms = service.initial_backoff_ms,
                stability_threshold_ms = service.stability_threshold_ms,
                socket = if service.declares_socket {
                    format!(
                        "    socket:\n      path: {}\n      mode: \"0660\"\n",
                        service_socket_path(&root, &service.name).display()
                    )
                } else {
                    String::new()
                },
            ));
        }

        let config_yaml = format!(
            concat!(
                "socket:\n",
                "  path: {socket}\n",
                "  mode: \"0600\"\n",
                "shutdown_grace_secs: {grace}\n",
                "services:\n{services}",
                "spawn_policy:\n",
                "  allowed_session_users: [{users}]\n",
                "  allowed_tool_paths: [{tools}]\n",
                "  allowed_mount_roots: [{mount_roots}]\n",
                "cgroup:\n",
                "  base_override: {base}\n",
                "  mount_root: {base}\n",
                "  controllers: [memory, cpu, pids]\n",
                "  supervisor_leaf: supervisor\n",
                "{ceilings}",
            ),
            socket = socket_path.display(),
            grace = self.shutdown_grace_secs,
            services = if service_yaml.is_empty() {
                "  []\n".to_string()
            } else {
                service_yaml
            },
            users = yaml_list(&self.allowed_session_users),
            tools = yaml_list(
                &self
                    .allowed_tool_paths
                    .iter()
                    .map(|p| p.display().to_string())
                    .collect::<Vec<_>>()
            ),
            mount_roots = yaml_list(
                &self
                    .allowed_mount_roots
                    .iter()
                    .map(|p| p.display().to_string())
                    .collect::<Vec<_>>()
            ),
            base = cgroup_base.display(),
            ceilings = self.ceilings_yaml(),
        );

        let config_path = root.join("supervisor.yaml");
        fs::write(&config_path, &config_yaml).expect("write supervisor config");

        let process = Command::new(env!("CARGO_BIN_EXE_tddy-supervisor"))
            .arg("--config")
            .arg(&config_path)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("launch tddy-supervisor");

        let mut running = RunningSupervisor {
            workspace,
            process: Some(process),
            socket_path,
            cgroup_base,
            config_yaml,
        };
        running.await_ready().await;
        running
    }

    fn ceilings_yaml(&self) -> String {
        let mut yaml = String::new();
        if let Some(memory) = self.memory_max_ceiling {
            yaml.push_str(&format!("  memory_max_ceiling: {memory}\n"));
        }
        if let Some(cpu) = &self.cpu_max_ceiling {
            yaml.push_str(&format!("  cpu_max_ceiling: \"{cpu}\"\n"));
        }
        if let Some(pids) = self.pids_max_ceiling {
            yaml.push_str(&format!("  pids_max_ceiling: {pids}\n"));
        }
        yaml
    }
}

/// A live supervisor process. Killed on drop so a failing test cannot leak `sleep` processes.
pub struct RunningSupervisor {
    workspace: TempDir,
    process: Option<Child>,
    socket_path: PathBuf,
    cgroup_base: PathBuf,
    config_yaml: String,
}

impl RunningSupervisor {
    pub fn socket_path(&self) -> &Path {
        &self.socket_path
    }

    pub fn cgroup_base(&self) -> &Path {
        &self.cgroup_base
    }

    pub fn pid(&self) -> u32 {
        self.process.as_ref().expect("supervisor is running").id()
    }

    pub async fn client(&self) -> SupervisorClient {
        SupervisorClient::connect(&self.socket_path)
            .await
            .expect("connect to the supervisor socket")
    }

    /// What the named service reported about the socket handover it received.
    pub async fn await_handoff_report(&self, service: &str) -> BTreeMap<String, String> {
        let path = handoff_report_path(self.workspace.path(), service);
        let deadline = Instant::now() + SETTLE_TIMEOUT;
        while Instant::now() < deadline {
            if let Ok(contents) = fs::read_to_string(&path) {
                let report: BTreeMap<String, String> = contents
                    .lines()
                    .filter_map(|line| line.split_once('='))
                    .map(|(key, value)| (key.to_string(), value.to_string()))
                    .collect();
                if report.contains_key("fd3") {
                    return report;
                }
            }
            tokio::time::sleep(POLL_INTERVAL).await;
        }
        panic!(
            "service '{service}' never reported a socket handover at {}",
            path.display()
        );
    }

    /// Poll until the named session reports `Exited`, then return its final status.
    pub async fn await_session_exit(
        &self,
        client: &SupervisorClient,
        pid: u32,
    ) -> tddy_supervisor::SessionStatus {
        let deadline = Instant::now() + SETTLE_TIMEOUT;
        let mut last = None;
        while Instant::now() < deadline {
            let status = client.session_status(pid).await.expect("session status");
            if status.state == tddy_supervisor::SessionState::Exited {
                return status;
            }
            last = Some(status);
            tokio::time::sleep(POLL_INTERVAL).await;
        }
        panic!("session {pid} never reported Exited; last status was {last:?}");
    }

    /// Discard a previous handover report, so a later `await_handoff_report` can only observe a
    /// fresh one rather than re-reading the dead instance's.
    pub fn clear_handoff_report(&self, service: &str) {
        let path = handoff_report_path(self.workspace.path(), service);
        if path.exists() {
            fs::remove_file(&path).expect("clear the handoff report");
        }
    }

    /// Path the named service's declared socket should have been bound at.
    pub fn declared_socket_path(&self, service: &str) -> PathBuf {
        service_socket_path(self.workspace.path(), service)
    }

    /// Every pid the named service's script has recorded across all of its starts, in order.
    pub fn recorded_starts(&self, service: &str) -> Vec<u32> {
        let log = self.workspace.path().join(format!("{service}.starts"));
        let Ok(contents) = fs::read_to_string(&log) else {
            return Vec::new();
        };
        contents
            .lines()
            .filter(|line| !line.trim().is_empty())
            .map(|line| line.trim().parse().expect("recorded start pid"))
            .collect()
    }

    /// Block until the named service reports `state`, then return its status.
    pub async fn await_service_state(&self, service: &str, state: ServiceState) -> ServiceStatus {
        let client = self.client().await;
        let deadline = Instant::now() + SETTLE_TIMEOUT;
        let mut last = None;
        while Instant::now() < deadline {
            let status = client
                .service_status(service)
                .await
                .expect("read service status");
            if status.state == state {
                return status;
            }
            last = Some(status);
            tokio::time::sleep(POLL_INTERVAL).await;
        }
        panic!("service '{service}' never reached {state:?}; last status was {last:?}");
    }

    /// Block until the named service reports a pid different from `previous`.
    pub async fn await_service_restart(&self, service: &str, previous: u32) -> ServiceStatus {
        let client = self.client().await;
        let deadline = Instant::now() + SETTLE_TIMEOUT;
        while Instant::now() < deadline {
            let status = client
                .service_status(service)
                .await
                .expect("read service status");
            if status.state == ServiceState::Running && status.pid != Some(previous) {
                return status;
            }
            tokio::time::sleep(POLL_INTERVAL).await;
        }
        panic!("service '{service}' was never restarted away from pid {previous}");
    }

    /// Block until the named service's script has recorded `count` starts.
    pub async fn await_recorded_starts(&self, service: &str, count: usize) -> Vec<u32> {
        let deadline = Instant::now() + SETTLE_TIMEOUT;
        while Instant::now() < deadline {
            let starts = self.recorded_starts(service);
            if starts.len() >= count {
                return starts;
            }
            tokio::time::sleep(POLL_INTERVAL).await;
        }
        panic!(
            "service '{service}' recorded {} starts, expected {count}",
            self.recorded_starts(service).len()
        );
    }

    /// Send `SIGTERM` and wait for the supervisor to exit.
    pub async fn terminate(&mut self) {
        let pid = self.pid() as i32;
        // SAFETY: `pid` is our own direct child, still unreaped, so it cannot have been recycled.
        unsafe { libc::kill(pid, libc::SIGTERM) };
        let mut process = self.process.take().expect("supervisor is running");
        let deadline = Instant::now() + SETTLE_TIMEOUT;
        while Instant::now() < deadline {
            if process.try_wait().expect("poll supervisor").is_some() {
                return;
            }
            tokio::time::sleep(POLL_INTERVAL).await;
        }
        let _ = process.kill();
        let _ = process.wait();
        panic!("supervisor did not exit within {SETTLE_TIMEOUT:?} of SIGTERM");
    }

    /// Block until the supervisor actually answers on its socket.
    ///
    /// Waiting for the socket *file* is not the same as waiting for a live supervisor: dropping a
    /// `UnixListener` does not unlink its inode, so a supervisor that dies at any point after it
    /// binds — while starting its declared services, say — leaves the path behind. A fixture that
    /// stops watching the child the moment that file appears therefore calls the start a success,
    /// and the death resurfaces later as an unexplained `Connection refused` from whichever RPC
    /// the test happens to make first. So poll both the connect and the child's exit status for
    /// the whole window, and report an exit as an exit.
    async fn await_ready(&mut self) {
        let deadline = Instant::now() + READY_TIMEOUT;
        // Assigned by the failed connect below. The deadline check that reports it is only
        // reachable after an attempt has been made, so there is no "never attempted" state to name.
        let mut last_error;
        loop {
            if let Some(status) = self
                .process
                .as_mut()
                .expect("supervisor is running")
                .try_wait()
                .expect("poll supervisor")
            {
                // Safe to read stderr to EOF only here: the child is confirmed dead, so its end
                // of the pipe is closed and the read cannot block.
                panic!(
                    "supervisor exited with {status} before answering on its socket\n\
                     --- stderr ---\n{}\n--- config ---\n{}",
                    self.drain_stderr(),
                    self.config_yaml
                );
            }
            match SupervisorClient::connect(&self.socket_path).await {
                Ok(_) => return,
                Err(e) => last_error = e.to_string(),
            }
            if Instant::now() >= deadline {
                panic!(
                    "supervisor never answered on {} within {READY_TIMEOUT:?}; last connect \
                     error: {last_error}",
                    self.socket_path.display()
                );
            }
            tokio::time::sleep(POLL_INTERVAL).await;
        }
    }

    /// Reads the supervisor's stderr to EOF. **Only call this once the process is confirmed
    /// exited** — against a live child the read blocks until it closes the pipe.
    fn drain_stderr(&mut self) -> String {
        use std::io::Read;
        let mut buffer = String::new();
        if let Some(stderr) = self
            .process
            .as_mut()
            .and_then(|process| process.stderr.as_mut())
        {
            let _ = stderr.read_to_string(&mut buffer);
        }
        buffer
    }
}

impl Drop for RunningSupervisor {
    fn drop(&mut self) {
        if let Some(mut process) = self.process.take() {
            let _ = process.kill();
            let _ = process.wait();
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Session tool fixtures
// ---------------------------------------------------------------------------------------------

/// A script that records the pid of whichever process exec'd it, then stays alive.
pub struct ToolFixture {
    path: PathBuf,
    parent_pid_log: PathBuf,
    _workspace: TempDir,
}

/// A tool that exits immediately with `code`, for asserting on reported exit statuses.
pub fn a_tool_that_exits_with(code: i32) -> ToolFixture {
    let workspace = TempDir::new().expect("create tool workspace");
    let path = workspace.path().join("exiting-tool");
    let parent_pid_log = workspace.path().join("parent.pid");
    write_script(&path, &format!("exit {code}"));
    ToolFixture {
        path,
        parent_pid_log,
        _workspace: workspace,
    }
}

pub fn a_tool_that_records_its_parent() -> ToolFixture {
    let workspace = TempDir::new().expect("create tool workspace");
    let path = workspace.path().join("recording-tool");
    let parent_pid_log = workspace.path().join("parent.pid");
    write_script(
        &path,
        &format!(
            concat!(
                "{{\n",
                // Field 5 of /proc/self/stat is the process group id. Asked of the kernel rather
                // than inferred, because a shell has no portable way to report its own pgid.
                "  echo \"ppid=$PPID\"\n",
                "  echo \"pgid=$(awk '{{print $5}}' /proc/self/stat)\"\n",
                "  echo \"pid=$$\"\n",
                "}} > {log}\n",
                "exec sleep 600",
            ),
            log = parent_pid_log.display()
        ),
    );
    ToolFixture {
        path,
        parent_pid_log,
        _workspace: workspace,
    }
}

impl ToolFixture {
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Block until the tool has recorded its own process group id.
    pub async fn await_recorded_process_group(&self) -> u32 {
        self.await_report()
            .await
            .get("pgid")
            .and_then(|value| value.parse().ok())
            .expect("tool recorded a process group id")
    }

    /// Block until the tool has recorded its own pid.
    pub async fn await_recorded_pid(&self) -> u32 {
        self.await_report()
            .await
            .get("pid")
            .and_then(|value| value.parse().ok())
            .expect("tool recorded its own pid")
    }

    async fn await_report(&self) -> BTreeMap<String, String> {
        let deadline = Instant::now() + SETTLE_TIMEOUT;
        while Instant::now() < deadline {
            if let Ok(contents) = fs::read_to_string(&self.parent_pid_log) {
                let report: BTreeMap<String, String> = contents
                    .lines()
                    .filter_map(|line| line.split_once('='))
                    .map(|(key, value)| (key.to_string(), value.trim().to_string()))
                    .collect();
                if report.contains_key("pid") {
                    return report;
                }
            }
            tokio::time::sleep(POLL_INTERVAL).await;
        }
        panic!(
            "tool never wrote a complete report at {}",
            self.parent_pid_log.display()
        );
    }

    /// Block until the tool has recorded the pid of the process that exec'd it.
    pub async fn await_recorded_parent_pid(&self) -> u32 {
        self.await_report()
            .await
            .get("ppid")
            .and_then(|value| value.parse().ok())
            .expect("tool recorded a parent pid")
    }
}

// ---------------------------------------------------------------------------------------------
// Assertions
// ---------------------------------------------------------------------------------------------

pub trait ServiceStatusAssertions {
    fn assert_named(&self, name: &str) -> &Self;
    fn assert_state(&self, state: ServiceState) -> &Self;
    fn assert_running_with_a_live_pid(&self) -> &Self;
    fn assert_has_no_pid(&self) -> &Self;
    fn assert_restart_count(&self, restarts: u32) -> &Self;
    fn pid(&self) -> u32;
}

impl ServiceStatusAssertions for ServiceStatus {
    fn assert_named(&self, name: &str) -> &Self {
        assert_eq!(self.name, name, "service name mismatch");
        self
    }

    fn assert_state(&self, state: ServiceState) -> &Self {
        assert_eq!(self.state, state, "state of service '{}'", self.name);
        self
    }

    fn assert_running_with_a_live_pid(&self) -> &Self {
        self.assert_state(ServiceState::Running);
        let pid = self.pid();
        assert!(
            process_is_alive(pid),
            "service '{}' reports pid {pid} but no such process is alive",
            self.name
        );
        self
    }

    fn assert_has_no_pid(&self) -> &Self {
        assert_eq!(
            self.pid, None,
            "service '{}' should report no pid",
            self.name
        );
        self
    }

    fn assert_restart_count(&self, restarts: u32) -> &Self {
        assert_eq!(
            self.restarts, restarts,
            "restart count of service '{}'",
            self.name
        );
        self
    }

    fn pid(&self) -> u32 {
        self.pid
            .unwrap_or_else(|| panic!("service '{}' reported no pid", self.name))
    }
}

pub trait ServiceListAssertions {
    fn assert_names_in_order(&self, names: &[&str]) -> &Self;
}

impl ServiceListAssertions for Vec<ServiceStatus> {
    fn assert_names_in_order(&self, names: &[&str]) -> &Self {
        let actual: Vec<&str> = self.iter().map(|status| status.name.as_str()).collect();
        assert_eq!(actual, names, "declared service order mismatch");
        self
    }
}

pub trait ScopeAssertions {
    fn assert_applied_limits(&self, expected: AppliedLimits) -> &Self;
    fn assert_wrote(&self, file: &str, contents: &str) -> &Self;
    fn assert_directory_exists(&self) -> &Self;
}

impl ScopeAssertions for ScopeHandle {
    fn assert_applied_limits(&self, expected: AppliedLimits) -> &Self {
        assert_eq!(
            self.applied, expected,
            "applied limits of scope '{}'",
            self.name
        );
        self
    }

    fn assert_wrote(&self, file: &str, contents: &str) -> &Self {
        let path = self.path.join(file);
        let actual = fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
        assert_eq!(actual.trim(), contents, "contents of {}", path.display());
        self
    }

    fn assert_directory_exists(&self) -> &Self {
        assert!(
            self.path.is_dir(),
            "expected scope directory {} to exist",
            self.path.display()
        );
        self
    }
}

pub trait DenialAssertions {
    fn assert_denied_without_disclosure(self);
}

impl<T: std::fmt::Debug> DenialAssertions for Result<T, SupervisorError> {
    /// A denial must be opaque: the same variant and the same message whatever was refused, so a
    /// caller cannot probe for the existence of a user, a path or a scope.
    fn assert_denied_without_disclosure(self) {
        let error = self.expect_err("expected the request to be denied");
        assert_eq!(error, SupervisorError::Denied, "expected an opaque denial");
        assert_eq!(
            error.to_string(),
            "request denied",
            "denial message leaked detail"
        );
    }
}

// ---------------------------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------------------------

/// Process group id of `pid`, from field 5 of `/proc/<pid>/stat`.
///
/// Read rather than assumed: a process's group id is only its own pid when something made it a
/// group leader, which is exactly the property a caller may want to assert.
pub fn process_group_of(pid: u32) -> u32 {
    let stat = fs::read_to_string(format!("/proc/{pid}/stat"))
        .unwrap_or_else(|error| panic!("read /proc/{pid}/stat: {error}"));
    // The comm field can contain spaces and parentheses, so fields are counted from after the
    // final ')' rather than by splitting the whole line.
    let after_comm = stat
        .rfind(')')
        .map(|index| &stat[index + 1..])
        .unwrap_or_else(|| panic!("malformed /proc/{pid}/stat: {stat}"));
    after_comm
        .split_whitespace()
        .nth(2)
        .and_then(|field| field.parse().ok())
        .unwrap_or_else(|| panic!("no pgid in /proc/{pid}/stat: {stat}"))
}

pub fn process_is_alive(pid: u32) -> bool {
    // SAFETY: signal 0 performs the permission and existence check without delivering a signal.
    unsafe { libc::kill(pid as i32, 0) == 0 }
}

pub fn current_username() -> String {
    let output = Command::new("id")
        .arg("-un")
        .output()
        .expect("resolve the current username with `id -un`");
    String::from_utf8(output.stdout)
        .expect("username is utf-8")
        .trim()
        .to_string()
}

fn write_service_script(root: &Path, service: &ServiceFixture) -> PathBuf {
    let path = root.join(format!("{}.sh", service.name));
    let log = root.join(format!("{}.starts", service.name));
    let body = service.body.replace(
        "__REPORT__",
        &handoff_report_path(root, &service.name)
            .display()
            .to_string(),
    );
    write_script(
        &path,
        &format!(
            "echo \"$$\" >> {log}\n{body}",
            log = log.display(),
            body = body
        ),
    );
    path
}

/// Where a socket-declaring service's script writes what the supervisor handed it.
fn handoff_report_path(root: &Path, service: &str) -> PathBuf {
    root.join(format!("{service}.handoff"))
}

fn service_socket_path(root: &Path, service: &str) -> PathBuf {
    root.join(format!("{service}.sock"))
}

/// What a service declaring a socket does first: record the handover it observed, then stay alive.
///
/// `test -S /proc/self/fd/3` is the honest check — it asks the kernel what fd 3 actually *is*,
/// rather than trusting `LISTEN_FDS` to be telling the truth about it.
const HANDOFF_REPORT_BODY: &str = "\
{
  echo \"listen_fds=${LISTEN_FDS:-unset}\"
  echo \"listen_pid=${LISTEN_PID:-unset}\"
  echo \"own_pid=$$\"
  if [ -S /proc/self/fd/3 ]; then echo \"fd3=socket\"; else echo \"fd3=absent\"; fi
} > __REPORT__
exec sleep 600";

fn write_script(path: &Path, body: &str) {
    fs::write(path, format!("#!/bin/sh\n{body}\n")).expect("write script");
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).expect("make script executable");
}

fn yaml_list(items: &[String]) -> String {
    items
        .iter()
        .map(|item| format!("\"{item}\""))
        .collect::<Vec<_>>()
        .join(", ")
}
