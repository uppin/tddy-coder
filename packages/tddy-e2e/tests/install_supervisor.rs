//! What `./install --systemd` puts on a host once the supervisor owns the daemon's lifecycle.
//!
//! Functional, not textual: the script runs for real into a temp tree with
//! `INSTALL_NO_SYSTEMCTL=1`, and the assertions read the files it produced.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Output};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
}

/// An install tree containing the script, its config templates, and fake release binaries.
struct InstallTree {
    _workspace: tempfile::TempDir,
    root: PathBuf,
    bin_dir: PathBuf,
    config_dir: PathBuf,
    systemd_dir: PathBuf,
    web_dir: PathBuf,
}

fn an_install_tree() -> InstallTree {
    let workspace = tempfile::tempdir().expect("create install workspace");
    let root = workspace.path().to_path_buf();

    copy_executable(&repo_root().join("install"), &root.join("install"));
    copy_repo_file(&root, "daemon.yaml.production");
    copy_repo_file(&root, "supervisor.yaml.production");

    let dist = root.join("packages").join("tddy-web").join("dist");
    fs::create_dir_all(&dist).expect("create web dist");
    fs::write(dist.join("index.html"), "<!DOCTYPE html><html></html>\n").expect("write index");

    let release = root.join("target").join("release");
    fs::create_dir_all(&release).expect("create release dir");
    for binary in [
        "tddy-daemon",
        "tddy-coder",
        "tddy-tools",
        "tddy-supervisor",
        "tddy-remote-git-repo",
        // ssh's SSH_ASKPASS helper for a daemon-run clone; `install` requires it beside the daemon.
        "tddy-askpass",
        "tddy-session-sync",
        // Runs inside every jail the daemon spawns, so `install` requires it exactly as it
        // requires the daemon. A fixture missing it fails the preflight before any assertion runs.
        "tddy-sandbox-runner",
        // Spawned by the daemon as a sibling of its own executable, so `install` requires it for
        // the same reason and with the same preflight.
        "tddy-index-daemon",
    ] {
        write_executable(&release.join(binary), "fake-binary\n");
    }

    InstallTree {
        _workspace: workspace,
        bin_dir: root.join("bin"),
        config_dir: root.join("etc"),
        systemd_dir: root.join("systemd"),
        web_dir: root.join("web"),
        root,
    }
}

impl InstallTree {
    /// Run `./install --systemd` against this tree, asserting it succeeded.
    fn install(&self) -> &Self {
        self.install_with(&[])
    }

    /// Run `./install --systemd` with extra environment, asserting it succeeded.
    fn install_with(&self, extra_env: &[(&str, &str)]) -> &Self {
        let outcome = self.attempt_install_with(extra_env);
        assert!(
            outcome.status.success(),
            "install exited with {:?}\nstdout:\n{}\nstderr:\n{}",
            outcome.status,
            String::from_utf8_lossy(&outcome.stdout),
            String::from_utf8_lossy(&outcome.stderr)
        );
        self
    }

    /// Run `./install --systemd` with extra environment and return what it did, whether or not it
    /// succeeded.
    fn attempt_install_with(&self, extra_env: &[(&str, &str)]) -> Output {
        let mut command = Command::new("bash");
        command
            .current_dir(&self.root)
            .arg(self.root.join("install"))
            .arg("--systemd");
        self.apply_env(&mut command);
        for (name, value) in extra_env {
            command.env(name, value);
        }
        command.output().expect("run install")
    }

    /// Run `./install --systemd` with the most permissive umask a `sudo` caller can hand it.
    fn install_under_a_permissive_umask(&self) -> ExitStatus {
        let mut command = Command::new("bash");
        command
            .current_dir(&self.root)
            .arg("-c")
            .arg("umask 000 && exec bash \"$0\" --systemd")
            .arg(self.root.join("install"));
        self.apply_env(&mut command);
        command.status().expect("run install")
    }

    fn apply_env(&self, command: &mut Command) {
        command
            .env("INSTALL_NO_SYSTEMCTL", "1")
            .env("INSTALL_BIN_DIR", &self.bin_dir)
            .env("INSTALL_CONFIG_DIR", &self.config_dir)
            .env("INSTALL_SYSTEMD_DIR", &self.systemd_dir)
            .env("INSTALL_WEB_BUNDLE_DIR", &self.web_dir);
    }

    fn unit(&self, name: &str) -> String {
        let path = self.systemd_dir.join(name);
        fs::read_to_string(&path).unwrap_or_else(|error| {
            panic!("read unit {}: {error}", path.display());
        })
    }

    fn installed_config(&self, name: &str) -> String {
        let path = self.config_dir.join(name);
        fs::read_to_string(&path).unwrap_or_else(|error| {
            panic!("read installed config {}: {error}", path.display());
        })
    }

    fn assert_no_unit(&self, name: &str) -> &Self {
        let path = self.systemd_dir.join(name);
        assert!(
            !path.exists(),
            "install should no longer write {}",
            path.display()
        );
        self
    }

    /// Every unit file name install left in the systemd directory, sorted.
    fn installed_unit_names(&self) -> Vec<String> {
        file_names_in(&self.systemd_dir)
    }

    /// Every `__NAME__` placeholder still present in a file install generated, as
    /// `file: __NAME__`. A survivor means a template the script never substitutes or a
    /// substitution that silently did nothing — either way the installed config names a path that
    /// does not exist.
    fn unrendered_placeholders(&self) -> Vec<String> {
        let mut found = Vec::new();
        for directory in [&self.config_dir, &self.systemd_dir] {
            for name in file_names_in(directory) {
                let path = directory.join(&name);
                let contents = fs::read_to_string(&path)
                    .unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
                for placeholder in placeholders_in(&contents) {
                    found.push(format!("{name}: {placeholder}"));
                }
            }
        }
        found.sort();
        found.dedup();
        found
    }

    fn overwrite_installed_config(&self, name: &str, contents: &str) -> &Self {
        let path = self.config_dir.join(name);
        fs::write(&path, contents)
            .unwrap_or_else(|error| panic!("write {}: {error}", path.display()));
        self
    }
}

fn file_names_in(directory: &Path) -> Vec<String> {
    let entries = fs::read_dir(directory)
        .unwrap_or_else(|error| panic!("read dir {}: {error}", directory.display()));
    let mut names: Vec<String> = entries
        .map(|entry| {
            entry
                .unwrap_or_else(|error| {
                    panic!("read dir entry in {}: {error}", directory.display())
                })
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    names.sort();
    names
}

fn placeholders_in(contents: &str) -> Vec<String> {
    let mut found = Vec::new();
    let mut rest = contents;
    while let Some(opening) = rest.find("__") {
        let after_opening = &rest[opening + 2..];
        let Some(closing) = after_opening.find("__") else {
            break;
        };
        let name = &after_opening[..closing];
        let is_placeholder_name = !name.is_empty()
            && name
                .chars()
                .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_');
        if is_placeholder_name {
            found.push(format!("__{name}__"));
        }
        rest = &after_opening[closing + 2..];
    }
    found
}

fn mode_of(path: &Path) -> u32 {
    let metadata =
        fs::metadata(path).unwrap_or_else(|error| panic!("stat {}: {error}", path.display()));
    metadata.permissions().mode() & 0o7777
}

fn assert_mode(path: &Path, expected: u32) {
    assert_eq!(
        mode_of(path),
        expected,
        "{} should be mode {expected:04o}",
        path.display()
    );
}

fn make_directory_writable_by_everyone(path: &Path) {
    set_mode(path, 0o777);
}

fn make_file_writable_by_everyone(path: &Path) {
    set_mode(path, 0o666);
}

fn set_mode(path: &Path, mode: u32) {
    fs::set_permissions(path, fs::Permissions::from_mode(mode))
        .unwrap_or_else(|error| panic!("chmod {}: {error}", path.display()));
}

fn assert_contains(haystack: &str, needle: &str, what: &str) {
    assert!(
        haystack.contains(needle),
        "{what} should contain `{needle}`, got:\n{haystack}"
    );
}

fn copy_repo_file(root: &Path, name: &str) {
    let source = repo_root().join(name);
    fs::copy(&source, root.join(name))
        .unwrap_or_else(|error| panic!("copy {}: {error}", source.display()));
}

fn copy_executable(source: &Path, destination: &Path) {
    fs::copy(source, destination)
        .unwrap_or_else(|error| panic!("copy {}: {error}", source.display()));
    make_executable(destination);
}

fn write_executable(path: &Path, contents: &str) {
    fs::write(path, contents).unwrap_or_else(|error| panic!("write {}: {error}", path.display()));
    make_executable(path);
}

fn make_executable(path: &Path) {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o755))
        .unwrap_or_else(|error| panic!("chmod {}: {error}", path.display()));
}

#[test]
fn installs_a_root_supervisor_unit_that_delegates_a_cgroup_subtree() {
    // Given
    let tree = an_install_tree();

    // When
    tree.install();

    // Then
    let unit = tree.unit("tddy-supervisor.service");
    assert_contains(&unit, "User=root", "the supervisor unit");
    assert_contains(&unit, "Delegate=yes", "the supervisor unit");
    assert_contains(
        &unit,
        &format!("ExecStart={}/tddy-supervisor", tree.bin_dir.display()),
        "the supervisor unit",
    );
}

#[test]
fn installs_a_supervisor_config_declaring_the_daemon_as_an_unprivileged_service() {
    // Given
    let tree = an_install_tree();

    // When
    tree.install();

    // Then
    let config = tree.installed_config("supervisor.yaml");
    assert_contains(&config, "name: tddy-daemon", "the supervisor config");
    assert_contains(
        &config,
        &format!("exec_start: {}/tddy-daemon", tree.bin_dir.display()),
        "the supervisor config",
    );
    assert_contains(&config, "user: tddy", "the supervisor config");
}

#[test]
fn no_longer_installs_a_standalone_daemon_unit() {
    // Given
    let tree = an_install_tree();

    // When
    tree.install();

    // Then the supervisor owns the daemon's lifecycle; a second unit would start it twice.
    tree.assert_no_unit("tddy-daemon.service");
    assert_eq!(
        tree.installed_unit_names(),
        vec![
            "tddy-supervisor.service".to_string(),
            "tddy-supervisor.socket".to_string()
        ],
        "install should leave exactly the supervisor's two units behind"
    );
}

#[test]
fn declares_the_supervisor_unit_as_conflicting_with_the_legacy_daemon_unit() {
    // Given
    let tree = an_install_tree();

    // When
    tree.install();

    // Then starting tddy-daemon.service by hand cannot produce a second daemon fighting the
    // supervisor's own child for the web port, the client socket and the token storage.
    assert_contains(
        &tree.unit("tddy-supervisor.service"),
        "Conflicts=tddy-daemon.service",
        "the supervisor unit",
    );
}

#[test]
fn creates_the_privileged_rpc_socket_as_root_with_group_access_only() {
    // Given
    let tree = an_install_tree();

    // When
    tree.install();

    // Then systemd — not the supervisor — creates the socket, and group membership is the whole
    // access grant (every request is still authorized by SO_PEERCRED).
    let unit = tree.unit("tddy-supervisor.socket");
    assert_contains(
        &unit,
        "ListenStream=/run/tddy-supervisor.sock",
        "the socket unit",
    );
    assert_contains(&unit, "SocketUser=root", "the socket unit");
    assert_contains(&unit, "SocketGroup=tddy-clients", "the socket unit");
    assert_contains(&unit, "SocketMode=0660", "the socket unit");
}

/// Keeps the scan below honest: an empty result must mean "nothing left to substitute", not "the
/// scanner never matches anything".
#[test]
fn finds_every_placeholder_a_template_line_declares() {
    // Given
    let template_line = "  path: __DAEMON_SOCKET_PATH__  # under __INSTALL_BIN_DIR__\n";

    // When
    let found = placeholders_in(template_line);

    // Then
    assert_eq!(found, ["__DAEMON_SOCKET_PATH__", "__INSTALL_BIN_DIR__"]);
}

#[test]
fn leaves_no_unrendered_placeholder_in_any_installed_file() {
    // Given
    let tree = an_install_tree();

    // When
    tree.install();

    // Then
    assert_eq!(
        tree.unrendered_placeholders(),
        Vec::<String>::new(),
        "every __PLACEHOLDER__ must be substituted; a survivor names a path that does not exist"
    );
}

#[test]
fn renders_one_daemon_socket_override_into_both_sides_of_the_handover() {
    // Given
    let tree = an_install_tree();
    // Not a path ending in the default's file name, so "the default is gone" means what it says.
    let socket_path = tree.root.join("run").join("client.sock");
    let socket_path = socket_path.to_str().expect("socket path must be UTF-8");

    // When
    tree.install_with(&[("INSTALL_DAEMON_SOCKET_PATH", socket_path)]);

    // Then both files name the overridden path — the supervisor binds it as root and hands it to
    // the daemon as fd 3, so a disagreement leaves the daemon serving nowhere.
    let supervisor_config = tree.installed_config("supervisor.yaml");
    let daemon_config = tree.installed_config("daemon.yaml");
    assert_contains(&supervisor_config, socket_path, "the supervisor config");
    assert_contains(&daemon_config, socket_path, "the daemon config");
    assert!(
        !supervisor_config.contains("/run/tddy-daemon.sock"),
        "the supervisor config should not keep the default path:\n{supervisor_config}"
    );
    assert!(
        !daemon_config.contains("/run/tddy-daemon.sock"),
        "the daemon config should not keep the default path:\n{daemon_config}"
    );
}

#[test]
fn renders_a_daemon_socket_path_containing_a_hash_into_both_configs() {
    // Given a path carrying the character that used to end the substitution expression
    let tree = an_install_tree();
    let socket_path = "/run/tddy-daemon#1.sock";

    // When
    tree.install_with(&[("INSTALL_DAEMON_SOCKET_PATH", socket_path)]);

    // Then the value is data, not syntax: neither config is truncated or short of it.
    assert_contains(
        &tree.installed_config("supervisor.yaml"),
        socket_path,
        "the supervisor config",
    );
    assert_contains(
        &tree.installed_config("daemon.yaml"),
        socket_path,
        "the daemon config",
    );
}

#[test]
fn renders_a_daemon_socket_path_containing_an_ampersand_into_both_configs() {
    // Given a path carrying the character that used to expand to the whole match
    let tree = an_install_tree();
    let socket_path = "/run/tddy-daemon&1.sock";

    // When
    tree.install_with(&[("INSTALL_DAEMON_SOCKET_PATH", socket_path)]);

    // Then
    assert_contains(
        &tree.installed_config("supervisor.yaml"),
        socket_path,
        "the supervisor config",
    );
    assert_contains(
        &tree.installed_config("daemon.yaml"),
        socket_path,
        "the daemon config",
    );
}

#[test]
fn refuses_a_multi_line_socket_path_and_writes_no_config_at_all() {
    // Given
    let tree = an_install_tree();

    // When
    let outcome =
        tree.attempt_install_with(&[("INSTALL_DAEMON_SOCKET_PATH", "/run/first\n/run/second")]);

    // Then install fails loudly and leaves nothing behind: a half-written config would be kept by
    // the "do not overwrite an existing config" guard on every later install.
    assert!(
        !outcome.status.success(),
        "install should reject a socket path spanning two lines; it exited {:?}",
        outcome.status
    );
    assert_contains(
        &String::from_utf8_lossy(&outcome.stderr),
        "spans more than one line",
        "install's stderr",
    );
    assert_eq!(
        file_names_in(&tree.config_dir),
        Vec::<String>::new(),
        "a failed render must leave the config directory empty, not half written"
    );
}

#[test]
fn keeps_an_existing_supervisor_config_rather_than_overwriting_the_policy_in_it() {
    // Given a host whose operator has declared a spawn policy — the config still declares the
    // daemon service identity install must obey
    let tree = an_install_tree();
    tree.install();
    let operator_policy = format!(
        "{}spawn_policy:\n  allowed_session_users: [alice]\n",
        a_supervisor_config_declaring_the_daemon_as("tddy", Some("tddy"))
    );
    tree.overwrite_installed_config("supervisor.yaml", &operator_policy);

    // When
    tree.install();

    // Then — supervisor.yaml is the entire privilege surface of the host
    assert_eq!(tree.installed_config("supervisor.yaml"), operator_policy);
}

#[test]
fn creates_configs_no_one_but_root_can_write_even_under_a_permissive_umask() {
    // Given — `sudo` inherits the caller's umask on most distributions, so `umask 000` reaches here
    let tree = an_install_tree();

    // When
    let status = tree.install_under_a_permissive_umask();

    // Then a local user must not be able to replace the policy the root broker enforces, nor the
    // ExecStart= of a unit that runs as root.
    assert!(status.success(), "install exited with {status:?}");
    assert_mode(&tree.config_dir, 0o755);
    assert_mode(&tree.config_dir.join("supervisor.yaml"), 0o644);
    assert_mode(&tree.config_dir.join("daemon.yaml"), 0o644);
    assert_mode(&tree.systemd_dir.join("tddy-supervisor.service"), 0o644);
    assert_mode(&tree.systemd_dir.join("tddy-supervisor.socket"), 0o644);
}

#[test]
fn repairs_a_world_writable_config_left_behind_by_an_earlier_install() {
    // Given a config directory and supervisor config an earlier install left writable by anyone
    let tree = an_install_tree();
    tree.install();
    make_directory_writable_by_everyone(&tree.config_dir);
    make_file_writable_by_everyone(&tree.config_dir.join("supervisor.yaml"));

    // When
    tree.install();

    // Then the permissions are re-asserted on the file it kept, so a once-bad install does not
    // stay bad forever
    assert_mode(&tree.config_dir, 0o755);
    assert_mode(&tree.config_dir.join("supervisor.yaml"), 0o644);
}

#[test]
fn declares_the_daemons_local_socket_for_the_supervisor_to_create() {
    // Given
    let tree = an_install_tree();

    // When
    tree.install();

    // Then the supervisor owns the listener the daemon used to get from `tddy-daemon.socket`, and
    // both files must name the same path — the daemon is an unprivileged child and cannot bind in
    // /run for itself.
    let supervisor_config = tree.installed_config("supervisor.yaml");
    let daemon_config = tree.installed_config("daemon.yaml");
    assert_contains(&supervisor_config, "socket:", "the supervisor config");
    assert_contains(
        &supervisor_config,
        "/run/tddy-daemon.sock",
        "the supervisor config's service socket",
    );
    assert_contains(
        &daemon_config,
        "/run/tddy-daemon.sock",
        "the daemon config's local socket",
    );
}

#[test]
fn installs_the_supervisor_binary_alongside_the_daemon() {
    // Given
    let tree = an_install_tree();

    // When
    tree.install();

    // Then
    let installed = tree.bin_dir.join("tddy-supervisor");
    assert!(
        installed.is_file(),
        "expected {} to be installed",
        installed.display()
    );
}

/// The `GIT_SSH_COMMAND` shim is a client of a daemon rather than part of one, but it is installed
/// with the daemon so `git clone <instance>:<project>` resolves it on `PATH`
/// (docs/ft/daemon/remote-git-repo.md § Shipping).
#[test]
fn installs_the_git_ssh_shim_so_git_can_exec_it_from_the_path() {
    // Given
    let tree = an_install_tree();

    // When
    tree.install();

    // Then
    let installed = tree.bin_dir.join("tddy-remote-git-repo");
    assert!(
        installed.is_file(),
        "expected {} to be installed",
        installed.display()
    );
    assert_mode(&installed, 0o755);
}

/// The worktree mirror is a client too, and is installed for the same reason the shim is: a binary
/// whose own documentation says "put it on PATH" that no script ever puts anywhere is a binary
/// nobody has.
#[test]
fn installs_the_worktree_mirror_so_it_can_be_run_from_the_path() {
    // Given
    let tree = an_install_tree();

    // When
    tree.install();

    // Then
    let installed = tree.bin_dir.join("tddy-session-sync");
    assert!(
        installed.is_file(),
        "expected {} to be installed",
        installed.display()
    );
    assert_mode(&installed, 0o755);
}

// ---------------------------------------------------------------------------------------------
// The daemon learns it is supervised from the supervisor, not from daemon.yaml
// (docs/ft/daemon/systemd-install.md)
// ---------------------------------------------------------------------------------------------

fn declares_a_top_level(yaml: &str, key: &str) -> bool {
    yaml.lines()
        .any(|line| line.starts_with(&format!("{key}:")))
}

#[test]
fn a_fresh_install_renders_no_supervisor_block() {
    // Given
    let tree = an_install_tree();

    // When
    tree.install();

    // Then — tddy-supervisor tells the daemon its socket; daemon.yaml carries no switch to drift
    let daemon_config = tree.installed_config("daemon.yaml");
    assert!(
        !declares_a_top_level(&daemon_config, "supervisor"),
        "a fresh daemon.yaml has no supervisor: block:\n{daemon_config}"
    );
}

#[test]
fn upgrading_removes_the_supervisor_block_from_an_existing_daemon_yaml_and_says_so() {
    // Given — a host whose daemon.yaml still carries the block (as every system install did)
    let tree = an_install_tree();
    tree.install();
    tree.overwrite_installed_config(
        "daemon.yaml",
        "users: []\n# The privileged socket.\nsupervisor:\n  socket_path: \"/run/tddy-supervisor.sock\"\nallowed_tools: []\n",
    );

    // When
    let output = tree.attempt_install_with(&[]);

    // Then — the block is gone, everything else the operator wrote is kept, and the run says so
    assert!(
        output.status.success(),
        "install failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let daemon_config = tree.installed_config("daemon.yaml");
    assert!(
        !declares_a_top_level(&daemon_config, "supervisor"),
        "the obsolete block is removed:\n{daemon_config}"
    );
    assert!(
        declares_a_top_level(&daemon_config, "users")
            && declares_a_top_level(&daemon_config, "allowed_tools"),
        "the operator's other settings are kept:\n{daemon_config}"
    );
    let narration = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        narration.contains("removed the obsolete `supervisor:` block"),
        "the install reports the removal:\n{narration}"
    );
}

// ---------------------------------------------------------------------------------------------
// The daemon's OS identity comes from supervisor.yaml, not from the install environment
// (docs/ft/daemon/systemd-install.md)
// ---------------------------------------------------------------------------------------------

/// A supervisor config whose `tddy-daemon` service declares the given `user:`/`group:`, with the
/// surrounding keys the real template carries so the file reads like an operator-edited install.
fn a_supervisor_config_declaring_the_daemon_as(user: &str, group: Option<&str>) -> String {
    let mut lines: Vec<String> = vec![
        "socket:".into(),
        "  path: /run/tddy-supervisor.sock".into(),
        "  group: tddy-clients".into(),
        "  mode: \"0660\"".into(),
        "services:".into(),
        "  - name: tddy-daemon".into(),
        "    exec_start: /usr/local/bin/tddy-daemon".into(),
        "    args: [\"-c\", \"/etc/tddy/daemon.yaml\"]".into(),
    ];
    lines.push(format!("    user: {user}"));
    if let Some(group) = group {
        lines.push(format!("    group: {group}"));
    }
    lines.push("    restart:".into());
    lines.push("      max_retries: 5".into());
    lines.join("\n") + "\n"
}

#[test]
fn an_upgrade_derives_the_daemon_identity_from_the_existing_supervisor_config() {
    // Given a host whose operator changed the daemon service's account in supervisor.yaml
    let tree = an_install_tree();
    tree.install();
    tree.overwrite_installed_config(
        "supervisor.yaml",
        &a_supervisor_config_declaring_the_daemon_as("mantas", Some("mantas")),
    );

    // When
    let output = tree.attempt_install_with(&[]);

    // Then install obeys the config it preserved rather than its own env defaults
    assert!(
        output.status.success(),
        "install failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let narration = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        narration.contains("daemon service user read from") && narration.contains("mantas"),
        "the install says the identity came from the config:\n{narration}"
    );
}

#[test]
fn an_install_daemon_user_disagreeing_with_the_existing_config_is_refused() {
    // Given a host whose supervisor config declares one user while the caller passes another
    let tree = an_install_tree();
    tree.install();
    tree.overwrite_installed_config(
        "supervisor.yaml",
        &a_supervisor_config_declaring_the_daemon_as("tddy", Some("tddy")),
    );

    // When
    let outcome = tree.attempt_install_with(&[("INSTALL_DAEMON_USER", "someone-else")]);

    // Then the env var must not silently win or silently lose — the run stops and names both
    assert!(
        !outcome.status.success(),
        "install should refuse a disagreeing INSTALL_DAEMON_USER"
    );
    let stderr = String::from_utf8_lossy(&outcome.stderr);
    assert!(
        stderr.contains("someone-else") && stderr.contains("tddy"),
        "the refusal names both the env value and the config value:\n{stderr}"
    );
}

#[test]
fn an_install_daemon_group_disagreeing_with_the_existing_config_is_refused() {
    // Given a host whose supervisor config declares one group while the caller passes another
    let tree = an_install_tree();
    tree.install();
    tree.overwrite_installed_config(
        "supervisor.yaml",
        &a_supervisor_config_declaring_the_daemon_as("tddy", Some("tddy")),
    );

    // When
    let outcome = tree.attempt_install_with(&[("INSTALL_DAEMON_GROUP", "some-other-group")]);

    // Then the group env var has no more authority than the user one once the config exists
    assert!(
        !outcome.status.success(),
        "install should refuse a disagreeing INSTALL_DAEMON_GROUP"
    );
    let stderr = String::from_utf8_lossy(&outcome.stderr);
    assert!(
        stderr.contains("some-other-group") && stderr.contains("tddy"),
        "the refusal names both the env value and the config value:\n{stderr}"
    );
}

#[test]
fn an_existing_config_whose_daemon_service_declares_no_user_is_refused() {
    // Given a supervisor config whose tddy-daemon service lost its user key
    let tree = an_install_tree();
    tree.install();
    let config_without_user = a_supervisor_config_declaring_the_daemon_as("tddy", Some("tddy"))
        .lines()
        .filter(|line| !line.contains("user:"))
        .collect::<Vec<_>>()
        .join("\n");
    tree.overwrite_installed_config("supervisor.yaml", &format!("{config_without_user}\n"));

    // When
    let outcome = tree.attempt_install_with(&[]);

    // Then install fails rather than falling back to a default the operator never declared
    assert!(
        !outcome.status.success(),
        "install should refuse a config without a daemon user"
    );
    let stderr = String::from_utf8_lossy(&outcome.stderr);
    assert!(
        stderr.contains("declares no user") || stderr.contains("no `user:`"),
        "the refusal says the user key is missing and names the remedy:\n{stderr}"
    );
}

#[test]
fn an_existing_config_whose_daemon_service_declares_no_group_is_refused() {
    // Given a supervisor config whose tddy-daemon service lost its group key
    let tree = an_install_tree();
    tree.install();
    tree.overwrite_installed_config(
        "supervisor.yaml",
        &a_supervisor_config_declaring_the_daemon_as("tddy", None),
    );

    // When
    let outcome = tree.attempt_install_with(&[]);

    // Then install fails rather than guessing a group the config never named
    assert!(
        !outcome.status.success(),
        "install should refuse a config without a daemon group"
    );
    let stderr = String::from_utf8_lossy(&outcome.stderr);
    assert!(
        stderr.contains("declares no group") || stderr.contains("no `group:`"),
        "the refusal says the group key is missing and names the remedy:\n{stderr}"
    );
}

#[test]
fn an_existing_config_declaring_root_as_the_daemon_user_is_refused() {
    // Given a supervisor config whose daemon service names root — a child the supervisor itself
    // would refuse at config load
    let tree = an_install_tree();
    tree.install();
    tree.overwrite_installed_config(
        "supervisor.yaml",
        &a_supervisor_config_declaring_the_daemon_as("root", Some("root")),
    );

    // When
    let outcome = tree.attempt_install_with(&[]);

    // Then install refuses with the same message it uses for a root INSTALL_DAEMON_USER
    assert!(
        !outcome.status.success(),
        "install should refuse a root daemon user"
    );
    let stderr = String::from_utf8_lossy(&outcome.stderr);
    assert!(
        stderr.contains("declares the tddy-daemon service as user root"),
        "the refusal names the config as the source of the root user:\n{stderr}"
    );
}

#[test]
fn a_fresh_install_renders_the_requested_daemon_identity_into_the_supervisor_config() {
    // Given a host with no supervisor config yet and an operator choosing the service account
    let tree = an_install_tree();

    // When
    tree.install_with(&[
        ("INSTALL_DAEMON_USER", "svc-tddy"),
        ("INSTALL_DAEMON_GROUP", "svc-tddy"),
    ]);

    // Then the rendered config carries the requested identity — the seed the config then owns
    let config = tree.installed_config("supervisor.yaml");
    assert_contains(&config, "user: svc-tddy", "the supervisor config");
    assert_contains(&config, "group: svc-tddy", "the supervisor config");
}
