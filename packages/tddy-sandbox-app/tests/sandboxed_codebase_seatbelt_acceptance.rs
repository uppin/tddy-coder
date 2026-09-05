//! Acceptance: `--codebase-mode sandboxed` — the checkout and its build inside a **real** Seatbelt
//! jail, driven from the host exactly as the host-run agent drives it.
//!
//! PRD: `docs/ft/coder/sandboxed-codebase-mode.md` (criteria 9 and 5).
//! Changeset: `docs/dev/1-WIP/2026-09-05-sandboxed-codebase-mode.md`.
//!
//! The other two modes put the agent in the jail; this one puts the code there. So the claim under
//! test is not "the agent cannot reach the host" but "the *tool call* cannot" — and only a real
//! kernel jail can answer it. Every test here dispatches through the very socket the host
//! `tddy-tools --mcp` dispatches through (`dispatch_via_sandbox_ipc`), so what passes here is what
//! the agent gets, not a rehearsal of it.

#![cfg(target_os = "macos")]

use std::path::{Path, PathBuf};
use std::time::Duration;

use tddy_sandbox_app::sandboxed_session::{
    provision, provision_with_interrupt, repo_build_home, ProvisioningInterrupted,
    SandboxedCodebaseParams,
};
use tddy_testing_commons::wait::eventually;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};

/// Long enough for a jailed `sh -c` to finish, short enough that a wedged jail fails the test
/// rather than hanging the suite.
const SHELL_BLOCK_MS: u64 = 30_000;

/// What a host file outside the checkout contains. A jail that can read it leaks this exact string.
const HOST_SECRET: &str = "a-host-file-the-jail-must-never-read";

/// What the stand-in agent home holds. A jail that reaches it leaks this exact string.
const AGENT_CREDENTIALS: &str = "an-oauth-token-the-jail-must-never-reach";

/// What the test's stand-in for "the network" answers with once a tunnel reaches it.
const NETWORK_BANNER: &str = "reached-through-the-host-relay";

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

fn sandbox_runner_binary() -> PathBuf {
    std::env::var_os("CARGO_BIN_EXE_tddy-sandbox-runner")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/debug/tddy-sandbox-runner")
        })
}

fn tools_binary() -> PathBuf {
    std::env::var_os("CARGO_BIN_EXE_tddy-tools")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/debug/tddy-tools")
        })
}

/// A provisioned `sandboxed` session: a live Seatbelt jail holding the checkout, the host-side tool
/// IPC socket the agent's MCP server would dispatch through, and a host file deliberately left
/// outside the checkout for the jail to fail to reach.
struct SandboxedCodebase {
    session: tddy_sandbox_app::sandboxed_session::SandboxedCodebaseSession,
    checkout: PathBuf,
    host_secret_file: PathBuf,
    host_agent_home: PathBuf,
    _host_outside_dir: tempfile::TempDir,
    _checkout_dir: tempfile::TempDir,
    _session_base: tempfile::TempDir,
}

async fn a_sandboxed_codebase_session() -> SandboxedCodebase {
    let home = tempfile::tempdir().expect("build home tempdir");
    a_sandboxed_codebase_session_with_build_home(home.path()).await
}

/// A session whose jail keeps its `$HOME` at `build_home`. Two sessions given the same one share
/// whatever their builds cached there, which is the point of it being persistent.
async fn a_sandboxed_codebase_session_with_build_home(build_home: &Path) -> SandboxedCodebase {
    a_sandboxed_codebase_session_homed(|_checkout| build_home.to_path_buf()).await
}

/// A session homed the way the app homes one: not at a directory the caller picked, but at the
/// per-repository home [`repo_build_home`] derives under the base this host keeps them in. Two of
/// these share a base and nothing else, which is the arrangement every host with more than one
/// checkout is in.
async fn a_sandboxed_codebase_session_under_a_build_home_base(base: &Path) -> SandboxedCodebase {
    a_sandboxed_codebase_session_homed(|checkout| repo_build_home(base, &canonical(checkout))).await
}

/// The jail, the checkout it confines and the host tree it must not reach — with the build's
/// `$HOME` wherever `build_home_for` puts it for this session's checkout.
async fn a_sandboxed_codebase_session_homed(
    build_home_for: impl Fn(&Path) -> PathBuf,
) -> SandboxedCodebase {
    let runner = sandbox_runner_binary();
    let tools = tools_binary();
    assert!(
        runner.exists(),
        "build tddy-sandbox-runner first: {}",
        runner.display()
    );
    assert!(
        tools.exists(),
        "build tddy-tools first: {}",
        tools.display()
    );

    let checkout_dir = tempfile::tempdir().expect("checkout tempdir");
    let checkout = checkout_dir.path().to_path_buf();

    // A host file that is emphatically not in the checkout and not under the session directory —
    // the two trees the jail legitimately holds.
    let host_outside_dir = tempfile::tempdir().expect("host tempdir");
    let host_secret_file = host_outside_dir.path().join("host-secret.txt");
    std::fs::write(&host_secret_file, HOST_SECRET).expect("write host secret");

    // An agent home of the shape the real one has, built here rather than borrowed from the
    // machine. The claim under test is about the *kind* of directory an unconfined agent keeps its
    // credentials in, not about one path existing on whoever's laptop is running the suite — a test
    // that reached for the real `~/.claude` would pass by accident on any host that has none.
    let host_agent_home = host_outside_dir.path().join(".claude");
    std::fs::create_dir_all(&host_agent_home).expect("create the stand-in agent home");
    std::fs::write(host_agent_home.join(".credentials.json"), AGENT_CREDENTIALS)
        .expect("seed the stand-in agent credentials");

    let session_base = tempfile::tempdir().expect("session base tempdir");
    let session_id = uuid::Uuid::now_v7().to_string();
    let session_dir = session_base.path().join(&session_id);

    let session = provision(SandboxedCodebaseParams {
        repo: checkout.clone(),
        session_id,
        session_dir,
        sandbox_runner_path: Some(runner.to_string_lossy().into_owned()),
        tddy_tools_path: Some(tools.to_string_lossy().into_owned()),
        repo_build_home: build_home_for(&checkout),
    })
    .await
    .expect("a sandboxed-codebase session must provision on a host with Seatbelt");

    SandboxedCodebase {
        session,
        checkout,
        host_secret_file,
        host_agent_home,
        _host_outside_dir: host_outside_dir,
        _checkout_dir: checkout_dir,
        _session_base: session_base,
    }
}

/// Everything `provision` needs, before it is provisioned: the tests about an *interrupted*
/// provision never get a session back, so they cannot ask a session for its own parameters.
struct AJailRequest {
    params: SandboxedCodebaseParams,
    session_id: String,
    _checkout_dir: tempfile::TempDir,
    _session_base: tempfile::TempDir,
    _build_home: tempfile::TempDir,
}

fn a_jail_request() -> AJailRequest {
    let checkout_dir = tempfile::tempdir().expect("checkout tempdir");
    let session_base = tempfile::tempdir().expect("session base tempdir");
    let build_home = tempfile::tempdir().expect("build home tempdir");
    let session_id = uuid::Uuid::now_v7().to_string();

    AJailRequest {
        params: SandboxedCodebaseParams {
            repo: checkout_dir.path().to_path_buf(),
            session_id: session_id.clone(),
            session_dir: session_base.path().join(&session_id),
            sandbox_runner_path: Some(sandbox_runner_binary().to_string_lossy().into_owned()),
            tddy_tools_path: Some(tools_binary().to_string_lossy().into_owned()),
            repo_build_home: build_home.path().to_path_buf(),
        },
        session_id,
        _checkout_dir: checkout_dir,
        _session_base: session_base,
        _build_home: build_home,
    }
}

/// Whether any process on this host still carries `session_id` in its argv. The jail's
/// `sandbox-exec` leader is spawned with `--session-id <id>` and a profile path under the session
/// directory, so this is how a test asks the question the owning process can no longer answer:
/// did anything survive the handle that was supposed to kill it?
fn a_running_process_names(session_id: &str) -> bool {
    std::process::Command::new("pgrep")
        .arg("-f")
        .arg(session_id)
        .output()
        .expect("pgrep must be available on this host")
        .status
        .success()
}

impl SandboxedCodebase {
    /// Dispatch a tool the way the host-run agent's `tddy-tools --mcp` does: over the app-served
    /// IPC socket named by `TDDY_SANDBOX_TOOL_IPC`, which the app forwards into the jail.
    async fn dispatch(&self, tool: &str, args: serde_json::Value) -> ToolResult {
        let raw = tddy_tools::session_tool_client::dispatch_via_sandbox_ipc(
            self.session.tool_ipc_socket(),
            tool,
            &args,
        )
        .await;
        let body: serde_json::Value = serde_json::from_str(&raw)
            .unwrap_or_else(|e| panic!("{tool} result must be JSON ({e}): {raw}"));
        ToolResult { body }
    }

    /// Run `command` in the jail and return its `{stdout, stderr, exit_code}`.
    async fn shell(&self, command: &str) -> ShellResult {
        let result = self
            .dispatch(
                "Shell",
                serde_json::json!({ "command": command, "block_until_ms": SHELL_BLOCK_MS }),
            )
            .await;
        let body = result.assert_succeeded().body;
        ShellResult {
            // Absent output is empty output — that is the shape the tool engine returns.
            stdout: body["stdout"].as_str().unwrap_or_default().to_string(),
            stderr: body["stderr"].as_str().unwrap_or_default().to_string(),
            // Strict, unlike the two above, because every confinement test in this file spells
            // "the jail refused it" as `exit_code != 0`. A default would satisfy that assertion,
            // so a jail that stopped reporting exit codes would turn each of them green.
            exit_code: body["exit_code"]
                .as_i64()
                .unwrap_or_else(|| panic!("a blocking Shell result must carry exit_code: {body}")),
        }
    }
}

struct ToolResult {
    body: serde_json::Value,
}

impl ToolResult {
    /// Whether the dispatch itself failed.
    ///
    /// Absence means success, and that is the client's contract rather than a default chosen here:
    /// `dispatch_via_sandbox_ipc` returns the tool's own `result_json` verbatim when the call
    /// succeeds, and only synthesises `{"error": …, "is_error": true}` when it does not.
    fn is_error(&self) -> bool {
        self.body
            .get("is_error")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false)
    }

    fn message(&self) -> String {
        self.body
            .get("error")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .to_string()
    }

    fn assert_succeeded(self) -> Self {
        assert!(
            !self.is_error(),
            "the call must reach the jail and succeed; error was '{}'",
            self.message()
        );
        self
    }

    fn assert_refused(self) -> Self {
        assert!(
            self.is_error(),
            "the call must be refused; it answered: {}",
            self.body
        );
        self
    }
}

struct ShellResult {
    stdout: String,
    stderr: String,
    exit_code: i64,
}

/// A TCP listener standing in for "the network": it greets whoever connects with [`NETWORK_BANNER`]
/// and closes. Bound on an ephemeral loopback port, which is deliberately *not* in the jail's
/// `loopback_allow_ports` — so anything that reaches it did so through the host's relay.
async fn a_host_tcp_service() -> u16 {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind the host stand-in service");
    let port = listener.local_addr().expect("local addr").port();
    tokio::spawn(async move {
        while let Ok((mut stream, _)) = listener.accept().await {
            let _ = stream.write_all(NETWORK_BANNER.as_bytes()).await;
            let _ = stream.shutdown().await;
        }
    });
    port
}

/// A minimal HTTP responder standing in for crates.io: it answers any request with
/// [`NETWORK_BANNER`]. Bound on an ephemeral loopback port that is deliberately *not* in the jail's
/// `loopback_allow_ports`, so anything reaching it did so through the host's relay.
async fn a_host_http_service() -> u16 {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind the host stand-in http service");
    let port = listener.local_addr().expect("local addr").port();
    tokio::spawn(async move {
        while let Ok((mut stream, _)) = listener.accept().await {
            tokio::spawn(async move {
                let mut buf = [0u8; 2048];
                let _ = stream.read(&mut buf).await;
                let body = NETWORK_BANNER;
                let _ = stream
                    .write_all(
                        format!(
                            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                            body.len()
                        )
                        .as_bytes(),
                    )
                    .await;
                let _ = stream.shutdown().await;
            });
        }
    });
    port
}

// ---------------------------------------------------------------------------
// The jail holds the checkout
// ---------------------------------------------------------------------------

/// The jail is not merely a wall: a tool dispatched from the host still does its job, against the
/// real checkout, and what it wrote is there afterwards.
#[tokio::test]
async fn a_write_dispatched_from_the_host_lands_in_the_checkout_inside_the_jail() {
    // Given
    let codebase = a_sandboxed_codebase_session().await;

    // When
    let result = codebase
        .dispatch(
            "Write",
            serde_json::json!({ "path": "through-the-jail.txt", "contents": "written inside" }),
        )
        .await;

    // Then
    result.assert_succeeded();
    assert_eq!(
        std::fs::read_to_string(codebase.checkout.join("through-the-jail.txt"))
            .expect("the jailed write must be visible in the checkout"),
        "written inside"
    );
}

/// A jailed `Shell` runs in the checkout, so a relative command sees the session's own tree rather
/// than whatever directory the app happened to be started from.
#[tokio::test]
async fn a_shell_dispatched_from_the_host_runs_with_the_checkout_as_its_working_directory() {
    // Given
    let codebase = a_sandboxed_codebase_session().await;
    std::fs::write(codebase.checkout.join("marker.txt"), "in the checkout")
        .expect("seed a file in the checkout");

    // When
    let result = codebase.shell("cat marker.txt").await;

    // Then
    assert_eq!(
        result.exit_code, 0,
        "the jailed shell must find the file; stderr was: {}",
        result.stderr
    );
    assert_eq!(result.stdout.trim(), "in the checkout");
}

// ---------------------------------------------------------------------------
// …and nothing else of the host
// ---------------------------------------------------------------------------

/// The whole point of the inversion: a tool call the host-run agent makes cannot read the host,
/// only the checkout. The refusal must come from the kernel — the file is named by absolute path,
/// which is exactly the argument a tool-engine path check would have to catch and a jail simply
/// cannot serve.
#[tokio::test]
async fn a_read_of_a_host_file_outside_the_checkout_is_refused_by_the_jail() {
    // Given a host file outside the checkout, which this process reads perfectly well — without
    // that, the refusal below could be a missing file rather than a jail.
    let codebase = a_sandboxed_codebase_session().await;
    assert_eq!(
        std::fs::read_to_string(&codebase.host_secret_file).expect("the host itself can read it"),
        HOST_SECRET
    );

    // When
    let result = codebase
        .dispatch(
            "Read",
            serde_json::json!({ "path": codebase.host_secret_file.to_string_lossy() }),
        )
        .await;

    // Then
    let refused = result.assert_refused();
    assert!(
        !refused.body.to_string().contains(HOST_SECRET),
        "the host file's contents must not appear in the answer: {}",
        refused.body
    );
}

/// A `Shell` is the widest surface the jail serves, so the same claim is made again through it:
/// the host tree outside the checkout is not there to be read.
#[tokio::test]
async fn a_shell_cannot_read_a_host_file_outside_the_checkout() {
    // Given a host file outside the checkout, which this process reads perfectly well — without
    // that, the refusal below could be a missing file rather than a jail.
    let codebase = a_sandboxed_codebase_session().await;
    assert_eq!(
        std::fs::read_to_string(&codebase.host_secret_file).expect("the host itself can read it"),
        HOST_SECRET
    );

    // When
    let result = codebase
        .shell(&format!(
            "cat {}",
            codebase.host_secret_file.to_string_lossy()
        ))
        .await;

    // Then
    assert_ne!(
        result.exit_code, 0,
        "reading a host file from inside the jail must fail; stdout was: {}",
        result.stdout
    );
    assert!(
        !result.stdout.contains(HOST_SECRET),
        "the host secret must not reach the jail; stdout was: {}",
        result.stdout
    );
}

// ---------------------------------------------------------------------------
// A build that turns hostile changes nothing outside the checkout
// ---------------------------------------------------------------------------
//
// The reason the mode exists. `cargo build` runs `build.rs`, `bun install` runs postinstall
// scripts, and a test suite runs whatever it runs — none of it authored or audited by whoever
// started the session. Reading is the lesser half of that risk; these are about the greater one.

/// A build cannot write anywhere on the host but its own checkout.
#[tokio::test]
async fn a_build_cannot_write_outside_the_checkout() {
    // Given
    let codebase = a_sandboxed_codebase_session().await;
    let target = canonical(&codebase.host_secret_file);

    // When — the shape a postinstall script takes when it decides to be helpful about your dotfiles.
    let result = codebase
        .shell(&format!("echo overwritten > {}", target.to_string_lossy()))
        .await;

    // Then
    assert_ne!(
        result.exit_code, 0,
        "a jailed build wrote to a host file outside its checkout; stdout was: {}",
        result.stdout
    );
    assert_eq!(
        std::fs::read_to_string(&target).expect("the host file must still be readable"),
        HOST_SECRET,
        "the host file was modified from inside the jail"
    );
}

/// A build cannot create new files on the host outside its checkout either — refusing to overwrite
/// an existing file says nothing about whether a fresh one can be dropped somewhere.
#[tokio::test]
async fn a_build_cannot_create_new_files_outside_the_checkout() {
    // Given
    let codebase = a_sandboxed_codebase_session().await;
    let planted = canonical(codebase._host_outside_dir.path()).join("planted-by-the-build.sh");

    // The control, so a refusal below means confinement and not merely a shell that cannot
    // redirect: the identical command inside the checkout must work.
    let control = codebase.shell("echo pwned > inside-the-checkout.sh").await;
    assert_eq!(
        control.exit_code, 0,
        "the same redirect must succeed inside the checkout, or this test proves nothing; \
         stderr was: {}",
        control.stderr
    );

    // When
    codebase
        .shell(&format!("echo pwned > {}", planted.to_string_lossy()))
        .await;

    // Then
    assert!(
        !planted.exists(),
        "a jailed build created {} on the host",
        planted.display()
    );
}

/// The agent runs unconfined on this host with the real `~/.claude`, so its credentials are exactly
/// what a hostile build would want and exactly what the jail exists to keep away from it. Neither
/// readable nor writable from inside.
#[tokio::test]
async fn a_build_cannot_reach_the_agent_home_holding_the_hosts_credentials() {
    // Given an agent home outside the checkout, which this process reads perfectly well
    let codebase = a_sandboxed_codebase_session().await;
    let credentials = codebase.host_agent_home.join(".credentials.json");
    assert_eq!(
        std::fs::read_to_string(&credentials).expect("the host itself can read its credentials"),
        AGENT_CREDENTIALS
    );

    // When the jailed build goes looking for them
    let listed = codebase
        .shell(&format!(
            "ls {}",
            codebase.host_agent_home.to_string_lossy()
        ))
        .await;
    let read = codebase
        .shell(&format!("cat {}", credentials.to_string_lossy()))
        .await;
    let planted = codebase.host_agent_home.join("planted-by-the-build");
    codebase
        .shell(&format!("echo x > {}", planted.to_string_lossy()))
        .await;

    // Then it finds nothing, reads nothing, and leaves nothing
    assert_ne!(
        listed.exit_code, 0,
        "the jail listed the agent home; stdout was: {}",
        listed.stdout
    );
    assert!(
        !read.stdout.contains(AGENT_CREDENTIALS),
        "the jail read the host's credentials; stdout was: {}",
        read.stdout
    );
    assert!(
        !planted.exists(),
        "a jailed build planted a file in the host's agent home"
    );
}

// ---------------------------------------------------------------------------
// The build inside the jail can still reach the network
// ---------------------------------------------------------------------------

/// A jail that runs `cargo build` needs dependencies. The `--workspace-tools` jail has no network
/// of its own, so its egress shim tunnels `CONNECT` through the host relay — the same relay the
/// agent used to use from the other side of the jail, pointed at the build instead.
#[tokio::test]
async fn a_connect_tunnel_from_the_jails_egress_shim_reaches_the_network_through_the_host() {
    // Given
    let codebase = a_sandboxed_codebase_session().await;
    let target_port = a_host_tcp_service().await;
    let mut shim =
        tokio::net::TcpStream::connect(("127.0.0.1", codebase.session.egress_shim_port()))
            .await
            .expect("the jail's egress shim must be listening");

    // When
    shim.write_all(format!("CONNECT 127.0.0.1:{target_port} HTTP/1.1\r\n\r\n").as_bytes())
        .await
        .expect("send CONNECT");
    let mut reader = BufReader::new(&mut shim);
    let mut status = String::new();
    reader
        .read_line(&mut status)
        .await
        .expect("read the CONNECT status line");

    // Then
    assert!(
        status.contains("200"),
        "the shim must establish the tunnel; status line was: {status:?}"
    );
    let mut rest = String::new();
    tokio::time::timeout(Duration::from_secs(10), reader.read_to_string(&mut rest))
        .await
        .expect("the tunnelled bytes must arrive before the deadline")
        .expect("read the tunnelled bytes");
    assert!(
        rest.contains(NETWORK_BANNER),
        "the host must have opened the real socket and pumped its bytes back; got: {rest:?}"
    );
}

/// The claim the CONNECT relay exists for, made the only way that can prove it: from **inside** the
/// jail, by a subprocess of the kind a build spawns.
///
/// The sibling tunnel test dials the shim from the host process, which proves the shim listens and
/// the relay forwards — and proves nothing at all about a build, because a build reaches the shim
/// only if the proxy environment is present in the tool subprocess. It was not, and no test said so.
#[tokio::test]
async fn a_build_inside_the_jail_reaches_the_network_through_the_host_relay() {
    // Given
    let codebase = a_sandboxed_codebase_session().await;
    let target_port = a_host_http_service().await;

    // Two preconditions, asserted rather than assumed. The client is the one thing here the test
    // cannot provision for itself — a jail runs whatever toolchain the platform has, and this suite
    // is macOS-only, where `curl` is part of the base system. Naming it turns "curl: command not
    // found" into a sentence that says what the suite needs.
    let client = codebase.shell("command -v curl").await;
    assert_eq!(
        client.exit_code, 0,
        "this test needs curl in the jail to stand in for a build's fetcher; stderr was: {}",
        client.stderr
    );
    // And the jail must actually carry a proxy, or the request below would mean nothing.
    let proxy = codebase.shell("printf %s \"$HTTP_PROXY\"").await;
    assert!(
        proxy.stdout.contains("127.0.0.1"),
        "a jail with an egress shim must point its subprocesses at it; HTTP_PROXY was {:?}",
        proxy.stdout
    );

    // When — the shape `cargo` and `bun` take when they fetch. `--noproxy ""` overrides the jail's
    // `NO_PROXY`, which excludes loopback so a jailed tool cannot loop back through the shim it is
    // talking to. The stand-in for crates.io has to live on loopback for the test to own it, so the
    // bypass has to be waived to ask the question a real remote fetch would ask.
    let fetched = codebase
        .shell(&format!(
            "curl -sS --max-time 20 --noproxy \"\" -x \"$HTTP_PROXY\" http://127.0.0.1:{target_port}/"
        ))
        .await;

    // Then
    assert!(
        fetched.stdout.contains(NETWORK_BANNER),
        "a jailed build must reach the network through the host relay; exit={} stdout={:?} stderr={:?}",
        fetched.exit_code,
        fetched.stdout,
        fetched.stderr
    );
}

// ---------------------------------------------------------------------------
// The fixture's own premise
// ---------------------------------------------------------------------------

/// Asserted before anything builds on it: a session that came up *unjailed* would serve every tool
/// above from the bare host, and each assertion would be about the tool engine rather than about a
/// jail. The socket the agent dispatches through must exist, and the checkout must be the tree the
/// session was provisioned for.
#[tokio::test]
async fn a_provisioned_session_exposes_the_tool_socket_and_the_checkout_it_confines() {
    // Given / When
    let codebase = a_sandboxed_codebase_session().await;

    // Then
    assert!(
        codebase.session.tool_ipc_socket().exists(),
        "the host tool IPC socket must be bound at {}",
        codebase.session.tool_ipc_socket().display()
    );
    assert_eq!(
        canonical(codebase.session.worktree()),
        canonical(&codebase.checkout),
        "the jail must confine the checkout the session was provisioned for"
    );
    assert!(
        codebase.session.egress_shim_port() > 0,
        "a sandboxed-codebase jail must have an egress shim for its build"
    );
}

// ---------------------------------------------------------------------------
// The jail resolves its checkout without seeing what is beside it
// ---------------------------------------------------------------------------

/// The tool engine runs inside the jail, so it must resolve the checkout's own path — which walks
/// every ancestor. That lookup is all the jail is granted there: it may traverse those directories,
/// not read them. A jail that could list the checkout's parent could enumerate every other session's
/// tree beside it.
#[tokio::test]
async fn the_jail_resolves_its_checkout_without_being_able_to_list_its_parent() {
    // Given
    let codebase = a_sandboxed_codebase_session().await;
    // Canonicalized, and that is the whole point of the test: the jail's grants are written
    // against symlink-resolved paths, so `ls /tmp/…` fails while resolving `/tmp` itself and would
    // pass this test no matter what the ancestors were granted. `/private/tmp/…` is the path the
    // grant actually names, so listing it is the question the grant answers.
    let parent = canonical(
        codebase
            .checkout
            .parent()
            .expect("the checkout must have a parent"),
    );
    let sibling = parent.join("a-sibling-the-jail-must-not-enumerate");
    std::fs::create_dir_all(&sibling).expect("create a sibling of the checkout");

    // When — a Write proves the lookup itself succeeds, before asking what else it bought.
    codebase
        .dispatch(
            "Write",
            serde_json::json!({ "path": "resolved.txt", "contents": "the checkout resolved" }),
        )
        .await
        .assert_succeeded();
    let listing = codebase
        .shell(&format!("ls {}", parent.to_string_lossy()))
        .await;

    // Then
    assert!(
        !listing
            .stdout
            .contains("a-sibling-the-jail-must-not-enumerate"),
        "the jail listed its checkout's parent; stdout was: {}",
        listing.stdout
    );
}

// ---------------------------------------------------------------------------
// The build's home outlives the session
// ---------------------------------------------------------------------------

/// A build's dependency caches live in its `$HOME` (`~/.cargo`, `~/.bun`). A home discarded with the
/// session would have every run refetch them through the CONNECT relay, so the home is persistent
/// and session-independent — and what one session's build left there, the next one's finds.
#[tokio::test]
async fn a_build_home_persists_across_sessions_so_dependency_caches_survive() {
    // Given
    let build_home = tempfile::tempdir().expect("build home tempdir");
    {
        let first = a_sandboxed_codebase_session_with_build_home(build_home.path()).await;
        first
            .shell("mkdir -p \"$HOME/.cargo\" && echo cached > \"$HOME/.cargo/registry-marker\"")
            .await;
    }

    // When — a second session, its own jail and its own session dir, the same build home.
    let second = a_sandboxed_codebase_session_with_build_home(build_home.path()).await;
    let result = second.shell("cat \"$HOME/.cargo/registry-marker\"").await;

    // Then
    assert_eq!(
        result.exit_code, 0,
        "the second session must find the first's cache; stderr was: {}",
        result.stderr
    );
    assert_eq!(result.stdout.trim(), "cached");
}

/// The persistence above is per **repository**, not per host. One `$HOME` shared by every checkout
/// would be a channel out of the jail and into the developer's own projects: an unaudited build
/// writes `$HOME/.cargo/config.toml` (`rustc-wrapper`, `target.*.runner`) and the next session's
/// build — against a repository its owner trusts — runs whatever that names. The path arithmetic is
/// unit-tested beside `repo_build_home`; this asks the kernel, of two sessions homed the way the
/// app homes them, under the one base a host has.
#[tokio::test]
async fn a_build_cannot_poison_the_home_of_another_repositorys_build() {
    // Given — one host's build-home base, and a session whose build poisons its own home
    let base = tempfile::tempdir().expect("build home base tempdir");
    {
        let unaudited = a_sandboxed_codebase_session_under_a_build_home_base(base.path()).await;
        let poisoned = unaudited
            .shell(
                "mkdir -p \"$HOME/.cargo\" && echo poisoned > \"$HOME/.cargo/config.toml\" \
                 && cat \"$HOME/.cargo/config.toml\"",
            )
            .await;
        assert_eq!(
            poisoned.stdout.trim(),
            "poisoned",
            "the premise is that a build owns its own $HOME; stderr was: {}",
            poisoned.stderr
        );
    }

    // When — a second session, another checkout, the same base
    let audited = a_sandboxed_codebase_session_under_a_build_home_base(base.path()).await;
    let inherited = audited.shell("cat \"$HOME/.cargo/config.toml\"").await;

    // Then
    assert!(
        !inherited.stdout.contains("poisoned"),
        "one repository's build reached another's $HOME; stdout was: {}",
        inherited.stdout
    );
    assert_ne!(
        inherited.exit_code, 0,
        "the other repository's build config must not even exist for this one"
    );
}

/// The symlink-resolved form of `path`, which is the only form the jail's grants are written
/// against. Strict on purpose: a test that quietly fell back to the unresolved path would ask the
/// kernel a different question than the one it says it is asking — `the_jail_resolves_its_checkout_
/// without_being_able_to_list_its_parent` exists precisely because that difference hid a real leak.
fn canonical(path: &Path) -> PathBuf {
    std::fs::canonicalize(path)
        .unwrap_or_else(|e| panic!("canonicalize {} for a jail grant: {e}", path.display()))
}

// ---------------------------------------------------------------------------
// The host agent's own configuration is not the jail's to rewrite
// ---------------------------------------------------------------------------

/// The MCP config the host-run agent is pointed at names a `command` and an `env` that the
/// **unconfined host** executes on every MCP (re)connect. A jail that could rewrite that file would
/// not need to escape — the host would run its payload for it. So the file lives in a directory the
/// profile grants the jail nothing over, and that is what this asserts against the kernel.
#[tokio::test]
async fn a_build_cannot_write_into_the_directory_holding_the_host_agents_mcp_config() {
    // Given
    let codebase = a_sandboxed_codebase_session().await;
    let host_only = canonical(codebase.session.host_dir());
    let mcp_config = host_only.join("mcp-config.json");

    // The control, so a refusal below means confinement and not merely a shell that cannot
    // redirect: the identical command inside the checkout must work.
    let control = codebase
        .shell("echo pwned > redirect-works-in-the-checkout.json")
        .await;
    assert_eq!(
        control.exit_code, 0,
        "the same redirect must succeed inside the checkout, or this test proves nothing; \
         stderr was: {}",
        control.stderr
    );

    // When — the rewrite a hostile build would attempt: point the host's MCP server at a command
    // of its own choosing.
    codebase
        .shell(&format!(
            "echo '{{\"mcpServers\":{{\"tddy-tools\":{{\"command\":\"/bin/sh\"}}}}}}' > {}",
            mcp_config.to_string_lossy()
        ))
        .await;

    // Then
    assert!(
        !mcp_config.exists(),
        "a jailed build wrote the host agent's MCP config at {}",
        mcp_config.display()
    );
}

/// Anyone who can connect to the tool socket gets unrestricted `ExecuteTool` into the jail — the
/// full `Shell` surface, against the checkout, with no agent in the way. `bind` would leave it
/// `0777 & ~umask`, so the mode is the only thing between that and every other account on the host.
#[tokio::test]
async fn the_tool_socket_admits_only_the_user_who_started_the_session() {
    use std::os::unix::fs::PermissionsExt;

    // Given / When
    let codebase = a_sandboxed_codebase_session().await;

    // Then
    let socket = codebase.session.tool_ipc_socket();
    let mode = std::fs::metadata(socket)
        .expect("the tool socket must exist")
        .permissions()
        .mode()
        & 0o777;
    assert_eq!(
        mode,
        0o600,
        "the tool socket at {} must be reachable by its owner alone; its mode was {mode:o}",
        socket.display()
    );
}

// ---------------------------------------------------------------------------
// An interrupted provision takes its jail with it
// ---------------------------------------------------------------------------

/// Ctrl-C while the jail is coming up must kill the jail, not merely stop waiting for it. The
/// window is two minutes wide (the ready-marker budget) and the child is in its own process group,
/// so the terminal's SIGINT never reaches it: a jail abandoned here would hold the checkout
/// read-write, with an egress shim on loopback, for as long as the machine stays up.
#[tokio::test]
async fn an_interrupt_while_the_jail_is_coming_up_leaves_no_jail_behind() {
    // Given
    let request = a_jail_request();
    let session_id = request.session_id.clone();

    // When — the interrupt is already pending when the wait for the ready marker begins.
    let error = provision_with_interrupt(request.params, std::future::ready(()))
        .await
        .err()
        .expect("an interrupted provision must not answer with a live session");

    // Then
    assert!(
        error.downcast_ref::<ProvisioningInterrupted>().is_some(),
        "the caller must be able to tell an interrupt from a failure, so it can exit the way an \
         interrupted program does; the error was: {error:#}"
    );
    eventually(
        "no process on this host still belongs to the interrupted session",
        Duration::from_secs(30),
        || match a_running_process_names(&session_id) {
            true => Err(format!("a process still names session {session_id}")),
            false => Ok(()),
        },
    )
    .await;
}

/// The control for the probe above: a session that came up *is* visible to it. Without this, an
/// interrupt test would pass just as happily against a probe that can never see anything.
#[tokio::test]
async fn a_provisioned_session_is_visible_to_the_host_as_a_running_process() {
    // Given / When
    let codebase = a_sandboxed_codebase_session().await;

    // Then
    assert!(
        a_running_process_names(codebase.session.session_id()),
        "a live jail for session {} must be visible on this host",
        codebase.session.session_id()
    );
}
