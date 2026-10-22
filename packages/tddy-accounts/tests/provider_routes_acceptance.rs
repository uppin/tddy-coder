//! What `ListProviderRoutes` answers: the caller's provider accounts, each marked with whether a
//! harness can run on it and why not.
//!
//! The matrix itself is the daemon's (`tddy_core::provider_route`), injected as a [`RouteSupport`];
//! here a scripted one stands in, so what is pinned is that the service asks it per account and
//! reports its words verbatim — the web never re-derives the matrix.
//!
//! Feature: docs/ft/daemon/provider-accounts.md (Running a harness on an account)

use std::sync::Arc;

use tddy_accounts::{
    endpoints_for, preset, AccountStore, AccountsError, AccountsServiceImpl, ProviderAccount,
    RouteSupport,
};
use tddy_credentials::{AccountId, CredentialRecord, ProviderId, SecretString};
use tddy_rpc::{Code, Request};
use tddy_service::proto::accounts::{
    AccountsService, ListProviderRoutesRequest, ListProviderRoutesResponse, ProviderRouteOption,
};

const ADAS_SESSION: &str = "session-token-for-ada";

fn an_account(
    preset_id: &str,
    account: &str,
    label: &str,
    openai_base_url: Option<&str>,
) -> CredentialRecord {
    let preset = preset(preset_id).expect("a preset");
    ProviderAccount {
        account: AccountId::new(account),
        preset,
        label: label.to_string(),
        endpoints: endpoints_for(preset, None, openai_base_url).expect("endpoints"),
        api_key: SecretString::new(format!("key-for-{account}")),
    }
    .to_record(1_760_000_000)
}

/// Ada's vault: her GitHub login, a Meta account and a Baseten account.
struct AdasVault(Vec<CredentialRecord>);

impl AccountStore for AdasVault {
    fn list(&self, session_token: &str) -> Result<Vec<CredentialRecord>, AccountsError> {
        if session_token == ADAS_SESSION {
            Ok(self.0.clone())
        } else {
            Err(AccountsError::NoSuchSession)
        }
    }

    fn set_label(
        &self,
        _: &str,
        provider: &ProviderId,
        account: &AccountId,
        _: &str,
    ) -> Result<CredentialRecord, AccountsError> {
        Err(AccountsError::NotFound {
            provider: provider.clone(),
            account: account.clone(),
        })
    }

    fn remove(&self, _: &str, _: &ProviderId, _: &AccountId) -> Result<(), AccountsError> {
        Ok(())
    }
}

fn adas_vault() -> AdasVault {
    let github = CredentialRecord {
        provider: ProviderId::new("github"),
        account: AccountId::new("github-1024"),
        label: "Ada".to_string(),
        secret: SecretString::new("gho_token"),
        metadata: Default::default(),
        updated_at: 1,
        version: tddy_credentials::FIRST_VERSION,
    };
    AdasVault(vec![
        an_account("baseten", "baseten-kimi", "Kimi on Baseten", None),
        github,
        an_account("meta", "meta-hackathon", "Meta hackathon", None),
    ])
}

/// Claude Code needs Anthropic Messages; Cursor runs on nothing.
struct AScriptedMatrix;

impl RouteSupport for AScriptedMatrix {
    fn harness_routable(&self, harness: &str) -> Result<(), String> {
        if harness.starts_with("cursor") {
            Err("Cursor's agent cannot use another provider".to_string())
        } else {
            Ok(())
        }
    }

    fn supports(&self, harness: &str, _preset: &str, protocols: &[&str]) -> Result<(), String> {
        self.harness_routable(harness)?;
        if protocols.contains(&"anthropic-messages") {
            Ok(())
        } else {
            Err("Claude Code needs an endpoint that serves Anthropic Messages".to_string())
        }
    }
}

async fn routes_for(harness: &str) -> ListProviderRoutesResponse {
    AccountsServiceImpl::new(Arc::new(adas_vault()))
        .with_route_support(Arc::new(AScriptedMatrix))
        .list_provider_routes(Request::direct(ListProviderRoutesRequest {
            session_token: ADAS_SESSION.to_string(),
            harness: harness.to_string(),
        }))
        .await
        .expect("routes")
        .into_inner()
}

fn option(account_id: &str, label: &str, preset: &str, reason: &str) -> ProviderRouteOption {
    ProviderRouteOption {
        account_id: account_id.to_string(),
        label: label.to_string(),
        preset: preset.to_string(),
        supported: reason.is_empty(),
        reason: reason.to_string(),
    }
}

#[tokio::test]
async fn list_provider_routes_marks_each_account_for_the_harness() {
    // When
    let routes = routes_for("claude-cli").await;

    // Then: provider accounts only, in the vault's order, each with the matrix's verdict
    assert!(routes.harness_routable);
    assert_eq!(
        routes.options,
        vec![
            option(
                "baseten-kimi",
                "Kimi on Baseten",
                "baseten",
                "Claude Code needs an endpoint that serves Anthropic Messages"
            ),
            option("meta-hackathon", "Meta hackathon", "meta", ""),
        ]
    );
}

#[tokio::test]
async fn a_harness_that_cannot_be_routed_says_so_once_and_marks_every_account() {
    // When
    let routes = routes_for("cursor-cli").await;

    // Then
    assert!(!routes.harness_routable);
    assert_eq!(
        routes.harness_reason,
        "Cursor's agent cannot use another provider"
    );
    assert!(routes.options.iter().all(|o| !o.supported));
}

#[tokio::test]
async fn a_daemon_without_the_matrix_refuses_rather_than_guessing() {
    // When
    let refusal = AccountsServiceImpl::new(Arc::new(adas_vault()))
        .list_provider_routes(Request::direct(ListProviderRoutesRequest {
            session_token: ADAS_SESSION.to_string(),
            harness: "claude-cli".to_string(),
        }))
        .await
        .expect_err("unwired");

    // Then
    assert_eq!(refusal.code(), Code::FailedPrecondition);
}
