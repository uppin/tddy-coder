//! `DescribeSessionStartOptions` answered on this host (`#start-parity` 5/9).
//!
//! Each list is read from the RPC the New-session form fills its picker from, called in-process on
//! this host: the same handler, the same code path, with no peer routing (the tool forwards a call
//! that names another host to that host whole).
//!
//! A list whose source this host does not have, or that its source refused, is empty and says why in
//! its `*_unavailable` field. Any other failure of a source fails the answer.

use tddy_discovery::catalog_service::CatalogHandler;
use tddy_host_service::HostServiceImpl;
use tddy_projects::ProjectHandler;
use tddy_rpc::Request;
use tddy_service::proto::accounts::ProviderRouteOption;
use tddy_service::proto::catalog::{
    ListAgentModelsRequest, ListAgentsRequest, ListSubagentsRequest,
};
use tddy_service::proto::host::{
    HostService, ListDerivedHostsRequest, ListSshConfigHostsRequest, ProbeOutcome,
};
use tddy_service::proto::project::ListProjectBranchesRequest;
use tddy_session_lifecycle::connection_service::launch_ports::LaunchSessions;
use tddy_tool_engine::start_options::{
    models_agent, provider_harness, DerivedHostChoice, LabelledChoice, ModelChoices,
    ProviderRouteChoice, StartOptions, StartOptionsQuery,
};

use super::project_home::status_error;
use crate::families::ProviderAccountsPort;
use crate::{CatalogRpcHandler, ProjectRpcHandler};

/// Why a host with no credential vaults has no provider accounts to answer for.
const NO_PROVIDER_ACCOUNTS: &str =
    "this host keeps no credential vaults, so it has no provider accounts to offer";

/// The sources one answer is read from, on this host, for one caller.
pub(super) struct StartOptionsSources<'a> {
    pub(super) session_token: &'a str,
    pub(super) daemon_instance_id: &'a str,
    pub(super) launch: &'a LaunchSessions,
    pub(super) catalog: &'a CatalogRpcHandler,
    pub(super) projects: &'a ProjectRpcHandler,
    pub(super) host_service: &'a HostServiceImpl,
    pub(super) provider_accounts: Option<&'a dyn ProviderAccountsPort>,
}

/// The host's choices for a start `query` narrows, for `project_id`.
pub(super) async fn describe_start_options(
    sources: StartOptionsSources<'_>,
    project_id: &str,
    query: StartOptionsQuery,
) -> anyhow::Result<StartOptions> {
    let listed = listed_agents(sources.catalog).await?;
    let agents = listed
        .iter()
        .map(|agent| LabelledChoice {
            id: agent.id.clone(),
            label: agent.label.clone(),
        })
        .collect();
    // The coding agents' session types, from this host's declarations — not from the catalogue's
    // rows, which list only the agents daemon.yaml or a profile names.
    let registry = sources.launch.coding_agents.agents();
    let harness_choices: Vec<tddy_tool_engine::start_session_args::AgentChoice> = registry
        .session_types()
        .into_iter()
        .filter_map(|declared| {
            let spec = registry.for_session_type(&declared.session_type)?;
            Some(tddy_tool_engine::start_session_args::AgentChoice {
                session_type: declared.session_type.clone(),
                label: declared.label.clone(),
                requires_model: spec.has(tddy_coding_agent_specs::Capability::RequiresModel),
                permission_modes: spec.agent.permission_modes.clone(),
                routable: !spec.route_support().providers.is_empty(),
            })
        })
        .collect();
    let effort_levels = registry
        .session_types()
        .into_iter()
        .filter_map(|declared| {
            let slot = registry
                .for_session_type(&declared.session_type)?
                .effort()?;
            Some(tddy_tool_engine::start_options::AgentEffortLevels {
                session_type: declared.session_type.clone(),
                default: slot.default.clone().unwrap_or_default(),
                levels: slot.levels.clone(),
            })
        })
        .collect();
    let session_types = registry
        .session_types()
        .into_iter()
        .map(|declared| LabelledChoice {
            id: declared.session_type.clone(),
            label: declared.label.clone(),
        })
        .collect();
    let subagents = subagent_names(sources.catalog).await?;
    let (models, models_unavailable) = model_choices(&sources, &query, &harness_choices).await?;
    let (provider_routes, provider_routes_unavailable) =
        provider_route_choices(&sources, &query, &harness_choices).await;
    let (ssh_config_hosts, ssh_config_hosts_unavailable) =
        ssh_config_hosts(sources.host_service, sources.session_token).await?;
    let (derived_hosts, derived_hosts_unavailable) =
        derived_hosts(sources.host_service, sources.session_token).await?;
    let branches = branch_names(sources.projects, sources.session_token, project_id).await?;
    Ok(StartOptions {
        daemon_instance_id: sources.daemon_instance_id.to_string(),
        session_types,
        agents,
        models,
        models_unavailable,
        effort_levels,
        provider_routes,
        provider_routes_unavailable,
        ssh_config_hosts,
        ssh_config_hosts_unavailable,
        derived_hosts,
        derived_hosts_unavailable,
        subagents,
        branches,
    })
}

/// The agents this host's catalogue offers, as the agent picker lists them.
async fn listed_agents(
    catalog: &CatalogRpcHandler,
) -> anyhow::Result<Vec<tddy_service::proto::catalog::AgentInfo>> {
    let listed = catalog
        .list_agents(Request::direct(ListAgentsRequest {}))
        .await
        .map_err(status_error)?
        .into_inner();
    Ok(listed.agents)
}

/// The subagents this host resolves, each qualified by this host as `name@daemon_instance_id`.
async fn subagent_names(catalog: &CatalogRpcHandler) -> anyhow::Result<Vec<String>> {
    let listed = catalog
        .list_subagents(Request::direct(ListSubagentsRequest {}))
        .await
        .map_err(status_error)?
        .into_inner();
    Ok(listed
        .subagents
        .into_iter()
        .map(|subagent| subagent.agent_id)
        .collect())
}

/// The models the query names: one provider account's, or one agent's from the vendor catalogue.
/// Its reason when the query names neither, or when the account's models cannot be listed.
async fn model_choices(
    sources: &StartOptionsSources<'_>,
    query: &StartOptionsQuery,
    agents: &[tddy_tool_engine::start_session_args::AgentChoice],
) -> anyhow::Result<(ModelChoices, String)> {
    let account_id = query.provider_account_id.trim();
    if !account_id.is_empty() {
        return Ok(account_models(sources, account_id).await);
    }
    let agent = match models_agent(query, agents) {
        Ok(agent) => agent,
        Err(reason) => return Ok((ModelChoices::default(), reason)),
    };
    let listed = sources
        .catalog
        .list_agent_models(Request::direct(ListAgentModelsRequest {
            session_token: sources.session_token.to_string(),
            agent,
            daemon_instance_id: String::new(),
        }))
        .await
        .map_err(status_error)?
        .into_inner();
    let models = ModelChoices {
        default: listed.default_model,
        choices: listed
            .models
            .into_iter()
            .map(|model| LabelledChoice {
                id: model.id,
                label: model.label,
            })
            .collect(),
    };
    Ok((models, String::new()))
}

/// One provider account's models, the first preselected as the form preselects them.
async fn account_models(
    sources: &StartOptionsSources<'_>,
    account_id: &str,
) -> (ModelChoices, String) {
    let Some(port) = sources.provider_accounts else {
        return (ModelChoices::default(), NO_PROVIDER_ACCOUNTS.to_string());
    };
    match port
        .provider_account_models(sources.session_token, account_id)
        .await
    {
        Ok(ids) => {
            let default = ids.first().cloned().unwrap_or_default();
            let choices = ids
                .into_iter()
                .map(|id| LabelledChoice {
                    label: id.clone(),
                    id,
                })
                .collect();
            (ModelChoices { default, choices }, String::new())
        }
        Err(status) => (ModelChoices::default(), status.message().to_string()),
    }
}

/// The accounts that can run the query's harness, with the reason when none can be asked.
async fn provider_route_choices(
    sources: &StartOptionsSources<'_>,
    query: &StartOptionsQuery,
    agents: &[tddy_tool_engine::start_session_args::AgentChoice],
) -> (Vec<ProviderRouteChoice>, String) {
    let harness = match provider_harness(query, agents) {
        Ok(harness) => harness,
        Err(reason) => return (Vec::new(), reason),
    };
    let Some(port) = sources.provider_accounts else {
        return (Vec::new(), NO_PROVIDER_ACCOUNTS.to_string());
    };
    match port.provider_routes(sources.session_token, &harness).await {
        Ok(options) => (
            options.into_iter().map(route_choice).collect(),
            String::new(),
        ),
        Err(status) => (Vec::new(), status.message().to_string()),
    }
}

fn route_choice(option: ProviderRouteOption) -> ProviderRouteChoice {
    ProviderRouteChoice {
        account_id: option.account_id,
        label: option.label,
        preset: option.preset,
        supported: option.supported,
        reason: option.reason,
    }
}

/// The SSH config hosts this host lists, or the reason it could not list them.
async fn ssh_config_hosts(
    host_service: &HostServiceImpl,
    session_token: &str,
) -> anyhow::Result<(Vec<String>, String)> {
    let answer = HostService::list_ssh_config_hosts(
        host_service,
        Request::direct(ListSshConfigHostsRequest {
            session_token: session_token.to_string(),
            daemon_instance_id: String::new(),
        }),
    )
    .await
    .map_err(status_error)?
    .into_inner();
    if answer.outcome == ProbeOutcome::Ok as i32 {
        return Ok((
            answer.hosts.into_iter().map(|host| host.alias).collect(),
            String::new(),
        ));
    }
    let reason = if answer.failure_reason.is_empty() {
        "this host could not list its SSH config".to_string()
    } else {
        answer.failure_reason
    };
    Ok((Vec::new(), reason))
}

/// The derived hosts this host reaches, or the reason it could not list them.
async fn derived_hosts(
    host_service: &HostServiceImpl,
    session_token: &str,
) -> anyhow::Result<(Vec<DerivedHostChoice>, String)> {
    let answer = HostService::list_derived_hosts(
        host_service,
        Request::direct(ListDerivedHostsRequest {
            session_token: session_token.to_string(),
            daemon_instance_id: String::new(),
        }),
    )
    .await
    .map_err(status_error)?
    .into_inner();
    if answer.outcome == ProbeOutcome::Failed as i32 {
        let reason = if answer.failure_reason.is_empty() {
            "this host could not list its derived hosts".to_string()
        } else {
            answer.failure_reason
        };
        return Ok((Vec::new(), reason));
    }
    let choices = answer
        .hosts
        .into_iter()
        .map(|host| DerivedHostChoice {
            id: host.id,
            label: host.label,
            kind: tddy_service::proto::host::DerivedHostKind::try_from(host.kind)
                .ok()
                .and_then(tddy_host_service::derived_hosts::kind_segment)
                .unwrap_or_default()
                .to_string(),
            parent_daemon_instance_id: host.parent_instance_id,
            usable: host.usable,
            unusable_reason: host.unusable_reason,
        })
        .collect();
    Ok((choices, String::new()))
}

/// The branches of `project_id`'s checkout on this host.
async fn branch_names(
    projects: &ProjectRpcHandler,
    session_token: &str,
    project_id: &str,
) -> anyhow::Result<Vec<String>> {
    let listed = projects
        .list_project_branches(Request::direct(ListProjectBranchesRequest {
            session_token: session_token.to_string(),
            project_id: project_id.to_string(),
            daemon_instance_id: String::new(),
        }))
        .await
        .map_err(status_error)?
        .into_inner();
    Ok(listed.branches)
}
