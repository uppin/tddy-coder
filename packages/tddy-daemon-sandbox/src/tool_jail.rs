//! The jail of a tool run from its definition (`#agent-tools 5/12`).
//!
//! Feature: `docs/ft/daemon/claude-cli-session.md` § The jail from the `claude` tool definition.
//!
//! [`lower_tool_jail`] turns a resolved tool closure and the definitions it came from into the
//! grants one jail needs: the persistent or session `HOME` and its files, the secrets the process
//! reads out of band, the non-file policy, and the egress policy — the union of the closure's
//! `network:` sections plus the host of the provider route the session runs on.
//! [`build_tool_jail_plan`] puts those grants on a [`SandboxRunnerSpawn`]'s plan.
//!
//! Nothing here names a tool: the sandboxed `claude-cli` start is its first caller.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use tddy_actions::tool::{FileItem, PolicySection, ResolvedTool, SecretItem, ToolDef, ToolHost};
use tddy_core::provider_route::HarnessRoute;
use tddy_rpc::Status;
use tddy_sandbox::builder::MachPolicy;
use tddy_sandbox::{
    process_exec_reads, EgressPolicy, HostPattern, MountSpec, PolicySpec, ReadSpec, SandboxError,
    SandboxPlan, SecretSpec, SymlinkSpec,
};
use tddy_sandbox_recipes::homes::HomeError;
use tddy_sandbox_recipes::{
    lower_files, lower_home, lower_network, lower_policy, lower_secrets, lower_tools,
    HostBinaryLibraries, JailPaths,
};

use crate::sandbox_session::{build_sandbox_plan, SandboxRunnerSpawn};

/// What one tool jail is lowered from.
pub struct ToolJailInputs<'a> {
    /// The tool the jail exists to run: its `home:` is the jail's `HOME`.
    pub tool: &'a ResolvedTool,
    /// The tool and its resolved deps, deps first (a closure from `resolve_closure`).
    pub closure: &'a [ResolvedTool],
    /// The definition of each member of [`Self::closure`].
    pub defs: &'a [ToolDef],
    /// `<tddyhome>`: where persistent homes live.
    pub tddy_home: &'a Path,
    /// The host user's home, which a definition's `from_host` paths are relative to.
    pub host_home: &'a Path,
    /// The jail's scratch tree (`<sandbox>/.work`): its session `home`, `tmp` and tool `bin`.
    pub scratch_dir: &'a Path,
    /// Where a `host-env` secret is read.
    pub host: &'a dyn ToolHost,
    /// The provider route the session runs on; `None` for the tool's own vendor.
    pub route: Option<&'a HarnessRoute>,
}

/// What a tool jail's plan is granted, lowered from a tool's definition.
#[derive(Debug, Clone)]
pub struct ToolJailGrants {
    /// `HOME` of the jailed process.
    pub home: PathBuf,
    /// Env the definition sets on the process: its `overlay` files items.
    pub env: BTreeMap<String, String>,
    /// The jail-local directory of tool links, to go first on the process's `PATH`.
    pub bin_dir: PathBuf,
    pub mounts: Vec<MountSpec>,
    pub reads: Vec<ReadSpec>,
    pub symlinks: Vec<SymlinkSpec>,
    pub secrets: Vec<SecretSpec>,
    pub policy: PolicySpec,
    /// Which hosts the jail may reach, with a provider route's host admitted.
    pub egress: EgressPolicy,
    /// Whether the jail takes an OAuth callback: declared by a tool, and only on its own vendor.
    pub oauth_inbound: bool,
}

/// Lower `inputs` into the grants of one jail.
///
/// Not pure: a persistent home is created (and a legacy one migrated), and a `seed-once` file is
/// copied into it, before the jail exists.
pub fn lower_tool_jail(inputs: &ToolJailInputs<'_>) -> Result<ToolJailGrants, Status> {
    let ToolJailInputs {
        tool,
        closure,
        defs,
        tddy_home,
        host_home,
        scratch_dir,
        host,
        route,
    } = *inputs;
    let tool_def = defs.iter().find(|def| def.id == tool.id).ok_or_else(|| {
        Status::internal(format!("no definition for the jail's tool {}", tool.id))
    })?;
    let session_home = scratch_dir.join("home");
    let tmp = scratch_dir.join("tmp");
    let paths = JailPaths {
        tddy_home,
        session_home: &session_home,
        tmp: &tmp,
        host_home,
    };

    let home = lower_home(&tool_def.home, &paths).map_err(home_status)?;
    let files: Vec<FileItem> = defs.iter().flat_map(|def| def.files.clone()).collect();
    let file_grants = lower_files(&files, &home.home, &paths).map_err(|e| {
        Status::internal(format!("place the files of {} in its home: {e}", tool.id))
    })?;
    let secret_items: Vec<SecretItem> = defs.iter().flat_map(|def| def.secrets.clone()).collect();
    let secrets = lower_secrets(&secret_items, route.is_some(), host);
    let policy_sections: Vec<PolicySection> = defs.iter().map(|def| def.policy.clone()).collect();
    // The knobs a tool's `policy:` cannot name: an interactive process reaches any Mach service and
    // owns a pseudo-terminal.
    let policy = PolicySpec {
        mach_lookup: MachPolicy::All,
        pseudo_tty: true,
        ..lower_policy(&policy_sections)
    };

    let (declared, oauth_declared) = lower_network(closure);
    let egress = with_route_hosts(declared, route);
    let bin_dir = scratch_dir.join("bin");
    let tool_grants = lower_tools(closure, &bin_dir, &HostBinaryLibraries);
    // What the binary itself needs to run under the jail: the OS baseline, toolchain directories
    // and its loader dependencies.
    let mut reads = process_exec_reads(&tool.binary);
    reads.extend(tool_grants.reads);
    reads.extend(file_grants.reads);
    let mut symlinks = tool_grants.symlinks;
    symlinks.extend(file_grants.symlinks);
    let mut env = file_grants.env;
    env.insert("HOME".to_string(), home.home.to_string_lossy().into_owned());

    Ok(ToolJailGrants {
        home: home.home,
        env,
        bin_dir,
        mounts: home.mount.into_iter().collect(),
        reads,
        symlinks,
        secrets,
        policy,
        egress,
        // A provider route's credentials are its own: the vendor's login callback is not opened
        // beside them.
        oauth_inbound: oauth_declared && route.is_none(),
    })
}

fn home_status(error: HomeError) -> Status {
    match error {
        HomeError::BothExist { .. } => Status::failed_precondition(error.to_string()),
        HomeError::Io { .. } => Status::internal(error.to_string()),
    }
}

/// `declared` with the host of each URL `route` points the tool at admitted: a session on a
/// provider route reaches its base-URL host and nothing else the tool did not declare.
///
/// A jail that declares no network at all (`none`) and has a route reaches that host alone;
/// `any` already admits it.
fn with_route_hosts(declared: EgressPolicy, route: Option<&HarnessRoute>) -> EgressPolicy {
    let route_hosts = route.map(route_hosts).unwrap_or_default();
    if route_hosts.is_empty() {
        return declared;
    }
    match declared {
        EgressPolicy::Any => EgressPolicy::Any,
        EgressPolicy::None => EgressPolicy::Hosts(route_hosts),
        EgressPolicy::Hosts(mut hosts) => {
            for host in route_hosts {
                if !hosts.contains(&host) {
                    hosts.push(host);
                }
            }
            EgressPolicy::Hosts(hosts)
        }
    }
}

/// The `host:port` of every `http(s)` URL `route` names: in its environment, whatever the variable is
/// named, and in its files — OpenCode's configuration names its provider's base URL there, not in the
/// environment. The port is the scheme's own when the URL names none.
pub fn route_hosts(route: &HarnessRoute) -> Vec<HostPattern> {
    let env_urls = route.env.iter().filter_map(|(_, value)| url_host(value));
    let file_urls = route
        .files
        .iter()
        .flat_map(|file| urls_in(&file.contents))
        .filter_map(url_host);
    env_urls.chain(file_urls).collect()
}

/// The `http(s)` URLs written in `text`: each is a whitespace- or punctuation-delimited token that
/// starts with its scheme, which is how a configuration file quotes one.
fn urls_in(text: &str) -> impl Iterator<Item = &str> {
    text.split(|c: char| {
        c.is_whitespace()
            || matches!(
                c,
                '"' | '\'' | ',' | '{' | '}' | '[' | ']' | '(' | ')' | '\\'
            )
    })
    .filter(|token| token.starts_with("http://") || token.starts_with("https://"))
}

fn url_host(value: &str) -> Option<HostPattern> {
    let (default_port, rest) = if let Some(rest) = value.strip_prefix("https://") {
        (443, rest)
    } else {
        (80, value.strip_prefix("http://")?)
    };
    let authority = rest.split(['/', '?', '#']).next()?;
    let authority = authority
        .rsplit_once('@')
        .map_or(authority, |(_, host)| host);
    let (host, port) = match authority.rsplit_once(':') {
        Some((host, port)) => (host, port.parse::<u16>().ok()?),
        None => (authority, default_port),
    };
    (!host.is_empty()).then(|| HostPattern::new(host.to_ascii_lowercase(), port))
}

/// The plan of a tool jail: `spawn`'s plan with `grants` on it. `spawn.mounts` must already hold
/// [`ToolJailGrants::mounts`].
///
/// The plan is built from the runner's argv alone, so it holds the runner's own reads and policy; the
/// tool's reads, links, secrets, policy and egress come from its definition and replace what the
/// runner's argv would imply.
pub fn build_tool_jail_plan(
    spawn: SandboxRunnerSpawn,
    grants: ToolJailGrants,
) -> Result<SandboxPlan, SandboxError> {
    let mut plan = build_sandbox_plan(spawn)?;
    plan.reads.extend(grants.reads);
    plan.symlinks.extend(grants.symlinks);
    plan.policy = grants.policy;
    plan.network.egress = grants.egress;
    plan.network.allow_oauth_inbound = grants.oauth_inbound;
    for secret in grants.secrets {
        plan.add_secret(secret);
    }
    Ok(plan)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn a_route(env: &[(&str, &str)]) -> HarnessRoute {
        HarnessRoute {
            env: env
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
            ..HarnessRoute::default()
        }
    }

    #[test]
    fn a_routes_base_url_host_is_admitted_with_its_scheme_default_port() {
        // Given
        let route = a_route(&[
            ("ANTHROPIC_BASE_URL", "https://Gateway.Example.com/v1"),
            ("ANTHROPIC_MODEL", "opus"),
            ("OTHER_URL", "http://127.0.0.1:4242/x"),
        ]);

        // When
        let hosts = route_hosts(&route);

        // Then
        assert_eq!(
            hosts,
            vec![
                HostPattern::new("gateway.example.com", 443),
                HostPattern::new("127.0.0.1", 4242),
            ]
        );
    }

    #[test]
    fn a_base_url_a_route_file_names_is_admitted_as_well() {
        // Given — OpenCode's configuration names its provider's base URL in the file, not the
        // environment, and the key is referenced rather than written
        let route = HarnessRoute {
            files: vec![tddy_core::provider_route::RouteFile {
                name: "opencode.json".to_string(),
                contents: r#"{"provider":{"route":{"options":{"baseURL":"http://127.0.0.1:9000/v1","apiKey":"{env:KEY}"}}}}"#
                    .to_string(),
            }],
            ..HarnessRoute::default()
        };

        // When
        let hosts = route_hosts(&route);

        // Then
        assert_eq!(hosts, vec![HostPattern::new("127.0.0.1", 9000)]);
    }

    #[test]
    fn a_route_host_joins_declared_hosts_and_opens_a_networkless_jail_to_itself_alone() {
        // Given
        let route = a_route(&[("ANTHROPIC_BASE_URL", "http://127.0.0.1:9000")]);
        let declared = EgressPolicy::Hosts(vec![HostPattern::new("api.anthropic.com", 443)]);

        // When
        let joined = with_route_hosts(declared, Some(&route));
        let alone = with_route_hosts(EgressPolicy::None, Some(&route));

        // Then
        assert_eq!(
            (joined, alone),
            (
                EgressPolicy::Hosts(vec![
                    HostPattern::new("api.anthropic.com", 443),
                    HostPattern::new("127.0.0.1", 9000),
                ]),
                EgressPolicy::Hosts(vec![HostPattern::new("127.0.0.1", 9000)]),
            )
        );
    }

    #[test]
    fn a_session_without_a_route_keeps_the_declared_policy() {
        // When
        let policy = with_route_hosts(EgressPolicy::Any, None);

        // Then
        assert_eq!(policy, EgressPolicy::Any);
    }
}
