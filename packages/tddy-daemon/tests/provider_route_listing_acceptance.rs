//! Acceptance: `accounts.AccountsService/ListProviderRoutes` answers a dropped-in agent from its
//! spec's `coding_agent.route` (`#agent-tools 13/19`).
//!
//! Feature doc: `docs/ft/daemon/provider-accounts.md`. The daemon is built by the real
//! `runtime::build` from a stub-GitHub config whose `tddy_data_dir/tools` holds `acme.yaml` — an
//! agent no Rust names, routed to Anthropic Messages through its environment. The stub user's vault
//! is opened in the daemon's own memory, holding a Meta account (which serves Anthropic Messages)
//! and an OpenAI-compatible one (which does not), and the call goes through the daemon's real RPC
//! roster, as `accounts_stub_daemon_acceptance` does.

use std::sync::Arc;

use pretty_assertions::assert_eq;
use prost::Message;
use tddy_accounts::{endpoints_for, preset, ProviderAccount};
use tddy_credentials::{AccountId, SecretString};
use tddy_daemon::config::DaemonConfig;
use tddy_daemon::runtime::{self, RuntimeOptions};
use tddy_daemon_auth::SessionVaults;
use tddy_rpc::{MultiRpcService, RequestMetadata, RequestTransport, RpcBridge, RpcMessage, Status};
use tddy_service::proto::accounts::{
    ListProviderRoutesRequest, ListProviderRoutesResponse, ProviderRouteOption,
};
use tddy_service::proto::auth::{
    ExchangeCodeRequest, ExchangeCodeResponse, GetAuthUrlRequest, GetAuthUrlResponse,
};
use tempfile::TempDir;
use tokio::net::TcpListener;

const THE_STUB_USER: &str = "testuser";
const THE_STUB_USERS_CODE: &str = "testuser-code";
const THE_VAULT_PASSPHRASE: &str = "correct horse battery staple";
const A_RECORD_WRITTEN_AT: u64 = 1_760_000_000;
const THE_META_ACCOUNT: &str = "meta-hackathon";
const THE_COMPATIBLE_ACCOUNT: &str = "local-compatible";

/// `acme`: an agent declared in this one file, whose route reaches Anthropic Messages through its
/// environment, and whose refusals name it `Acme Code`.
const ACME_YAML: &str = r#"schema_version: 1
id: acme
summary: Acme Code — an agent added as a tool file alone
binary:
  command: acme
  env: TDDY_ACME_CLI
deps: [tddy-tools]
reads: []
run:
  transport: pty
  argv:
    - "{binary}"
    - { if: model, then: ["--model", "{model}"] }
  env: {}
extensions:
  session: { session_type: acme-cli, launch: agent_cli, label: Acme Code }
  coding_agent:
    product: Acme Code
    mcp: { kind: unsupported }
    tool_allow: unsupported
    tool_deny: unsupported
    permission_prompt: unsupported
    instructions:
      files: [AGENTS.md]
    hooks: { adapter: claude-settings-local }
    models:
      default: acme-1
      curated: [acme-1]
    route:
      providers: [anthropic]
      shape: env
    placements: [host]
home: { kind: session }
policy:
  jit: false
  fork: true
  sysctl_read: true
  exec_paths: [/usr/bin, /usr/libexec, /bin, /sbin, /System, /Library]
"#;

#[tokio::test]
async fn list_provider_routes_for_a_dropped_in_agent_offers_the_accounts_its_spec_providers_serve()
{
    // Given — acme.yaml in the daemon's tools, and the stub user's vault holding a Meta and an
    // OpenAI-compatible account
    let daemon = a_stub_daemon_whose_tools_hold(&[("acme.yaml", ACME_YAML)]).await;
    daemon.the_vault_of_the_stub_user_holding_a_meta_and_a_compatible_account();
    let token = daemon.a_login_as(THE_STUB_USERS_CODE).await;

    // When
    let routes = daemon
        .list_provider_routes(&token, "acme-cli")
        .await
        .expect("ListProviderRoutes answers");

    // Then — acme is routable; the Meta account, which serves Anthropic Messages, is offered, and
    // the compatible one is refused naming acme's product
    assert_eq!(
        routes,
        ListProviderRoutesResponse {
            options: vec![
                ProviderRouteOption {
                    account_id: THE_COMPATIBLE_ACCOUNT.to_string(),
                    label: "Local compatible".to_string(),
                    preset: "openai-compatible".to_string(),
                    supported: false,
                    reason: "Acme Code needs an account whose endpoint serves Anthropic \
                             Messages, and this one does not"
                        .to_string(),
                },
                ProviderRouteOption {
                    account_id: THE_META_ACCOUNT.to_string(),
                    label: "Meta hackathon".to_string(),
                    preset: "meta".to_string(),
                    supported: true,
                    reason: String::new(),
                },
            ],
            harness_routable: true,
            harness_reason: String::new(),
        }
    );
}

// ---------------------------------------------------------------------------------------------
// The daemon under test

/// A daemon built from a stub-GitHub config with `auth_storage`, and its RPC roster.
struct AStubDaemon {
    rpc: RpcBridge<MultiRpcService>,
    vaults: Arc<SessionVaults>,
    _dir: TempDir,
}

/// A daemon whose stub GitHub signs [`THE_STUB_USER`] in, whose `tddy_data_dir/tools` holds
/// `tools` (`(file name, YAML)`), and whose credential vaults live in a temporary directory.
async fn a_stub_daemon_whose_tools_hold(tools: &[(&str, &str)]) -> AStubDaemon {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let tools_dir = dir.path().join("tddy").join("tools");
    std::fs::create_dir_all(&tools_dir).unwrap();
    for (name, yaml) in tools {
        std::fs::write(tools_dir.join(name), yaml).unwrap();
    }
    let runtime = runtime::build(
        a_stub_config_in(&dir).await,
        RuntimeOptions::for_binary(tddy_spawn::supervisor_client::SpawnBackendChoice::ForkedWorker),
    )
    .await
    .expect("the daemon runtime builds");
    let vaults = Arc::clone(
        runtime
            .credential_vaults()
            .expect("a daemon with auth_storage keeps credential vaults"),
    );
    AStubDaemon {
        rpc: RpcBridge::new(MultiRpcService::new(runtime.entries)),
        vaults,
        _dir: dir,
    }
}

impl AStubDaemon {
    /// Hand the daemon the stub user's two provider accounts as a login would, then create the
    /// vault, which seals them and leaves it open.
    fn the_vault_of_the_stub_user_holding_a_meta_and_a_compatible_account(&self) {
        let meta = preset("meta").expect("the meta preset");
        let compatible = preset("openai-compatible").expect("the openai-compatible preset");
        let accounts = [
            ProviderAccount {
                account: AccountId::new(THE_META_ACCOUNT),
                preset: meta,
                label: "Meta hackathon".to_string(),
                endpoints: endpoints_for(meta, None, None).expect("meta endpoints"),
                api_key: SecretString::new("a-meta-key"),
            },
            ProviderAccount {
                account: AccountId::new(THE_COMPATIBLE_ACCOUNT),
                preset: compatible,
                label: "Local compatible".to_string(),
                endpoints: endpoints_for(compatible, None, Some("http://127.0.0.1:9/v1"))
                    .expect("openai-compatible endpoints"),
                api_key: SecretString::new("a-compatible-key"),
            },
        ];
        for account in accounts {
            self.vaults
                .retain(THE_STUB_USER, account.to_record(A_RECORD_WRITTEN_AT))
                .expect("the daemon holds the record for the vault");
        }
        self.vaults
            .create(THE_STUB_USER, &SecretString::new(THE_VAULT_PASSPHRASE))
            .expect("the vault is created and opened");
    }

    /// Sign in through the stub's redirect flow as whoever `code` names; returns the session token.
    async fn a_login_as(&self, code: &str) -> String {
        let url: GetAuthUrlResponse = self
            .call("auth.AuthService", "GetAuthUrl", GetAuthUrlRequest {})
            .await
            .expect("an authorize url is handed out");
        let exchanged: ExchangeCodeResponse = self
            .call(
                "auth.AuthService",
                "ExchangeCode",
                ExchangeCodeRequest {
                    code: code.to_string(),
                    state: url.state,
                },
            )
            .await
            .expect("the stub completes the login");
        exchanged.session_token
    }

    /// `ListProviderRoutes` for `harness`, its options ordered by account id.
    async fn list_provider_routes(
        &self,
        session_token: &str,
        harness: &str,
    ) -> Result<ListProviderRoutesResponse, Status> {
        let mut routes: ListProviderRoutesResponse = self
            .call(
                "accounts.AccountsService",
                "ListProviderRoutes",
                ListProviderRoutesRequest {
                    session_token: session_token.to_string(),
                    harness: harness.to_string(),
                },
            )
            .await?;
        routes
            .options
            .sort_by(|a, b| a.account_id.cmp(&b.account_id));
        Ok(routes)
    }

    async fn call<Req: Message, Res: Message + Default>(
        &self,
        service: &str,
        method: &str,
        request: Req,
    ) -> Result<Res, Status> {
        let message = RpcMessage::new(
            request.encode_to_vec(),
            RequestMetadata::over(RequestTransport::InProcess),
        );
        let wire_bytes = match self
            .rpc
            .handle_messages(service, method, &[message])
            .await?
        {
            tddy_rpc::ResponseBody::Complete(mut chunks) => chunks.remove(0),
            _ => panic!("{service}/{method} is unary"),
        };
        Ok(Res::decode(&wire_bytes[..]).expect("a unary response decodes"))
    }
}

/// Stub GitHub knowing [`THE_STUB_USER`] by [`THE_STUB_USERS_CODE`], a `users:` row for them, and
/// `auth_storage` and the data directory under `dir`.
async fn a_stub_config_in(dir: &TempDir) -> DaemonConfig {
    let yaml = format!(
        "listen:\n  web_port: {port}\n  web_host: 127.0.0.1\n\
         tddy_data_dir: \"{data_dir}\"\n\
         auth_storage: \"{auth_storage}\"\n\
         github:\n  stub: true\n  stub_codes: \"{THE_STUB_USERS_CODE}:{THE_STUB_USER}\"\n\
         users:\n  - github_user: {THE_STUB_USER}\n    os_user: {os_user}\n",
        port = a_free_tcp_port().await,
        data_dir = dir.path().join("tddy").display(),
        auth_storage = dir.path().join("auth").display(),
        os_user = the_os_user_this_process_runs_as(),
    );
    serde_yaml::from_str(&yaml).expect("the config fixture parses")
}

fn the_os_user_this_process_runs_as() -> String {
    // SAFETY: `getuid` has no preconditions and cannot fail.
    tddy_session_lifecycle::user_sessions_path::username_for_uid(unsafe { libc::getuid() })
        .expect("the test process runs as a named user")
}

/// A port that is free at this moment, so nothing in the fixture competes for a fixed one.
async fn a_free_tcp_port() -> u16 {
    TcpListener::bind(("127.0.0.1", 0))
        .await
        .expect("no loopback port was available")
        .local_addr()
        .expect("the bound listener has no address")
        .port()
}
