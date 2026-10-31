//! Unit: lowering a tool's generic jail sections — `home`, `files`, `secrets`, `policy` — into the
//! grants a jail's plan carries.
//!
//! PRD: `docs/ft/daemon/1-WIP/PRD-2026-10-31-claude-cli-jail-from-spec.md` (`#agent-tools 5/12`).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use pretty_assertions::assert_eq;
use tddy_actions::tool::{
    FileItem, HomeSection, PolicySection, RouteGuard, SecretItem, SecretItemSource, ToolHost,
};
use tddy_sandbox::{MountSpec, SecretSource, SecretSpec, SymlinkSpec};
use tddy_sandbox_recipes::{
    lower_files, lower_home, lower_policy, lower_secrets, FileGrants, HomeGrant, JailPaths,
};

const HOST_TOKEN: &str = "host-oauth-token";

/// A host whose only environment is `vars`; its filesystem is the real one.
struct AHostWithEnv(BTreeMap<String, String>);

impl ToolHost for AHostWithEnv {
    fn env_var(&self, name: &str) -> Option<String> {
        self.0.get(name).cloned()
    }
    fn path_dirs(&self) -> Vec<PathBuf> {
        Vec::new()
    }
    fn is_executable_file(&self, _path: &Path) -> bool {
        false
    }
    fn canonicalize(&self, path: &Path) -> std::io::Result<PathBuf> {
        std::fs::canonicalize(path)
    }
}

fn a_host_with(vars: &[(&str, &str)]) -> AHostWithEnv {
    AHostWithEnv(
        vars.iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect(),
    )
}

/// The directories one lowering runs against, each a real directory under one tempdir.
struct ALayout {
    _root: tempfile::TempDir,
    tddy_home: PathBuf,
    session_home: PathBuf,
    tmp: PathBuf,
    host_home: PathBuf,
}

impl ALayout {
    fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        let base = std::fs::canonicalize(root.path()).unwrap();
        let layout = Self {
            tddy_home: base.join("tddy"),
            session_home: base.join("session/home"),
            tmp: base.join("session/tmp"),
            host_home: base.join("host"),
            _root: root,
        };
        for dir in [
            &layout.tddy_home,
            &layout.session_home,
            &layout.tmp,
            &layout.host_home,
        ] {
            std::fs::create_dir_all(dir).unwrap();
        }
        layout
    }

    fn paths(&self) -> JailPaths<'_> {
        JailPaths {
            tddy_home: &self.tddy_home,
            session_home: &self.session_home,
            tmp: &self.tmp,
            host_home: &self.host_home,
        }
    }

    /// A file at `relative` under the host home, holding `contents`.
    fn with_host_file(&self, relative: &str, contents: &str) -> PathBuf {
        let path = self.host_home.join(relative);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, contents).unwrap();
        path
    }

    /// A tool home under the session dir that already holds `relative` with `contents`.
    fn a_home_holding(&self, relative: &str, contents: &str) -> PathBuf {
        let home = self.session_home.clone();
        let path = home.join(relative);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, contents).unwrap();
        home
    }
}

fn seed_once(from_host: &str, to: &str) -> FileItem {
    FileItem::SeedOnce {
        from_host: PathBuf::from(from_host),
        to: PathBuf::from(to),
    }
}

fn a_secret(env: &str, var: &str, when: Option<RouteGuard>) -> SecretItem {
    SecretItem {
        env: env.to_string(),
        source: SecretItemSource::HostEnv {
            var: var.to_string(),
        },
        when,
    }
}

#[test]
fn a_persistent_home_is_lowered_to_homes_key_mounted_read_write() {
    // Given
    let layout = ALayout::new();
    let section = HomeSection::Persistent {
        key: "claude".to_string(),
    };

    // When
    let grant = lower_home(&section, &layout.paths());

    // Then
    let home = layout.tddy_home.join("homes/claude");
    assert_eq!(
        grant,
        Ok(HomeGrant {
            home: home.clone(),
            mount: Some(MountSpec::read_write(home)),
        })
    );
}

#[test]
fn a_session_home_is_the_jails_own_scratch_home_with_no_mount() {
    // Given
    let layout = ALayout::new();

    // When
    let grant = lower_home(&HomeSection::Session, &layout.paths());

    // Then
    assert_eq!(
        grant,
        Ok(HomeGrant {
            home: layout.session_home.clone(),
            mount: None,
        })
    );
}

#[test]
fn a_seed_once_item_copies_the_host_file_into_a_home_that_lacks_it_readable_by_its_owner_only() {
    // Given
    let layout = ALayout::new();
    layout.with_host_file(".claude/.credentials.json", "host-credentials");
    let home = layout.session_home.clone();

    // When
    let grants = lower_files(
        &[seed_once(
            ".claude/.credentials.json",
            ".claude/.credentials.json",
        )],
        &home,
        &layout.paths(),
    )
    .expect("the files lower");

    // Then
    let seeded = home.join(".claude/.credentials.json");
    assert_eq!(
        std::fs::read_to_string(&seeded).unwrap(),
        "host-credentials"
    );
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(&seeded).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
    assert_eq!(grants, FileGrants::default());
}

#[test]
fn a_seed_once_item_leaves_a_file_the_home_already_holds() {
    // Given — a home whose tool has since refreshed its own credentials
    let layout = ALayout::new();
    layout.with_host_file(".claude/.credentials.json", "host-credentials");
    let home = layout.a_home_holding(".claude/.credentials.json", "refreshed-in-the-jail");

    // When
    lower_files(
        &[seed_once(
            ".claude/.credentials.json",
            ".claude/.credentials.json",
        )],
        &home,
        &layout.paths(),
    )
    .expect("the files lower");

    // Then
    assert_eq!(
        std::fs::read_to_string(home.join(".claude/.credentials.json")).unwrap(),
        "refreshed-in-the-jail"
    );
}

#[test]
fn a_mirror_item_becomes_a_symlink_from_the_home_to_the_host_path() {
    // Given
    let layout = ALayout::new();
    let host_install = layout.with_host_file(".local/bin/claude", "#!/bin/sh\n");
    let home = layout.session_home.clone();
    let mirror = FileItem::Mirror {
        from_host: PathBuf::from(".local/bin/claude"),
        to: PathBuf::from(".local/bin/claude"),
    };

    // When
    let grants = lower_files(&[mirror], &home, &layout.paths()).expect("the files lower");

    // Then
    assert_eq!(
        grants.symlinks,
        vec![SymlinkSpec {
            link: home.join(".local/bin/claude"),
            target: host_install,
        }]
    );
    assert_eq!(grants.env, BTreeMap::new());
}

#[test]
fn an_overlay_item_sets_its_env_with_the_jails_tmp_joined_in() {
    // Given
    let layout = ALayout::new();
    let home = layout.session_home.clone();
    let overlays = [
        FileItem::Overlay {
            env: "CLAUDE_CODE_TMPDIR".to_string(),
            value: "{tmp}".to_string(),
        },
        FileItem::Overlay {
            env: "XDG_CONFIG_HOME".to_string(),
            value: "{home}/.config".to_string(),
        },
    ];

    // When
    let grants = lower_files(&overlays, &home, &layout.paths()).expect("the files lower");

    // Then
    assert_eq!(
        grants.env,
        BTreeMap::from([
            (
                "CLAUDE_CODE_TMPDIR".to_string(),
                layout.tmp.to_string_lossy().into_owned()
            ),
            (
                "XDG_CONFIG_HOME".to_string(),
                format!("{}/.config", home.display())
            ),
        ])
    );
    assert_eq!(grants.symlinks, Vec::new());
}

#[test]
fn a_host_env_secret_becomes_a_secret_spec_carrying_the_hosts_value() {
    // Given
    let host = a_host_with(&[("CLAUDE_CODE_OAUTH_TOKEN", HOST_TOKEN)]);
    let secret = a_secret("CLAUDE_CODE_OAUTH_TOKEN", "CLAUDE_CODE_OAUTH_TOKEN", None);

    // When
    let secrets = lower_secrets(&[secret], false, &host);

    // Then
    assert_eq!(
        secrets,
        vec![SecretSpec {
            env_name: "CLAUDE_CODE_OAUTH_TOKEN".to_string(),
            source: SecretSource::Value(HOST_TOKEN.to_string()),
        }]
    );
}

#[test]
fn a_host_file_secret_becomes_a_secret_spec_naming_the_file() {
    // Given
    let host = a_host_with(&[]);
    let secret = SecretItem {
        env: "API_KEY".to_string(),
        source: SecretItemSource::HostFile {
            path: PathBuf::from("/etc/tddy/api-key"),
        },
        when: None,
    };

    // When
    let secrets = lower_secrets(&[secret], false, &host);

    // Then
    assert_eq!(
        secrets,
        vec![SecretSpec {
            env_name: "API_KEY".to_string(),
            source: SecretSource::HostFile(PathBuf::from("/etc/tddy/api-key")),
        }]
    );
}

#[test]
fn a_when_unrouted_secret_is_dropped_on_a_routed_launch_and_an_unguarded_one_is_kept() {
    // Given
    let host = a_host_with(&[
        ("CLAUDE_CODE_OAUTH_TOKEN", HOST_TOKEN),
        ("TELEMETRY_KEY", "telemetry"),
    ]);
    let secrets = [
        a_secret(
            "CLAUDE_CODE_OAUTH_TOKEN",
            "CLAUDE_CODE_OAUTH_TOKEN",
            Some(RouteGuard::Unrouted),
        ),
        a_secret("TELEMETRY_KEY", "TELEMETRY_KEY", None),
    ];

    // When
    let lowered = lower_secrets(&secrets, true, &host);

    // Then
    assert_eq!(
        lowered,
        vec![SecretSpec {
            env_name: "TELEMETRY_KEY".to_string(),
            source: SecretSource::Value("telemetry".to_string()),
        }]
    );
}

#[test]
fn a_host_env_secret_whose_variable_is_unset_delivers_nothing() {
    // Given
    let host = a_host_with(&[("SET_TOKEN", "set")]);
    let secrets = [
        a_secret("UNSET_TOKEN", "UNSET_TOKEN", None),
        a_secret("SET_TOKEN", "SET_TOKEN", None),
    ];

    // When
    let lowered = lower_secrets(&secrets, false, &host);

    // Then
    assert_eq!(
        lowered,
        vec![SecretSpec {
            env_name: "SET_TOKEN".to_string(),
            source: SecretSource::Value("set".to_string()),
        }]
    );
}

#[test]
fn policy_sections_union_their_allows_and_concatenate_exec_paths_without_repeats() {
    // Given
    let agent = PolicySection {
        jit: true,
        fork: true,
        exec_paths: vec![PathBuf::from("/usr/bin"), PathBuf::from("/bin")],
        sysctl_read: false,
    };
    let helper = PolicySection {
        jit: false,
        fork: false,
        exec_paths: vec![PathBuf::from("/bin"), PathBuf::from("/opt/homebrew")],
        sysctl_read: true,
    };

    // When
    let policy = lower_policy(&[agent, helper]);

    // Then
    assert_eq!(
        (
            policy.allow_dynamic_code_generation,
            policy.allow_process_fork,
            policy.sysctl_read,
            policy.pseudo_tty,
            policy.exec_paths,
        ),
        (
            true,
            true,
            true,
            false,
            vec![
                PathBuf::from("/usr/bin"),
                PathBuf::from("/bin"),
                PathBuf::from("/opt/homebrew"),
            ],
        )
    );
}
