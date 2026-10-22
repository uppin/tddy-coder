//! The screen-sharing and accounts entries, and the credential-sync engine the accounts service
//! reports on.

use super::build_context::BuildContext;
use super::mount::Mounted;
use super::*;
use tddy_daemon_auth::SessionVaults;

type AccountsService = tddy_accounts::AccountsServiceImpl<tddy_accounts::SessionVaultAccountStore>;

impl BuildContext {
    pub(super) fn mount_screen_sharing_entry(&mut self, m: &Mounted) {
        let tddy_data_dir = &self.identity.keys.tddy_data_dir;
        let ss_user_resolver = m.mount.user_resolver.clone();
        let config_arc = &m.mount.config_arc;
        let room_tokens = &self.identity.keys.room_tokens;
        let host_prompts = &m.shared.host_prompts;
        // Screen sharing service — target records in the credential store + VNC/RDP bridge
        // spawning. Wire `sessions_base` with the daemon's resolved `tddy_data_dir` so session
        // metadata is read from the config-only tddy home (config → profile default →
        // `$HOME/.tddy`), matching `sessions_base_resolver` above.
        let ss_sessions_base: tddy_screen_sharing::SessionsBase = {
            let dd = tddy_data_dir.clone();
            Arc::new(move |user: &str| {
                tddy_session_lifecycle::user_sessions_path::sessions_base_for_user(user, Some(&dd))
            })
        };
        // Host scope, alongside the host registry rather than under any session: a desktop belongs
        // to a machine, and deleting a session must not delete the host's target. The keypair is
        // the one `#hosts-screen 6/8` publishes and reads from the same directory — one key per
        // host, so a password the browser encrypted for a prompt is readable here too.
        let ss_host_targets: Arc<
            dyn tddy_session_lifecycle::host_desktop_targets::HostDesktopTargetStore,
        > = Arc::new(
            tddy_session_lifecycle::host_desktop_targets::FileHostDesktopTargetStore::new(
                tddy_session_lifecycle::host_registry::host_registry_dir(tddy_data_dir),
            ),
        );
        let ss_host_keypair: Arc<dyn tddy_session_lifecycle::host_keypair::HostKeypair> =
            Arc::new(tddy_session_lifecycle::host_keypair::FileHostKeypair::new(
                tddy_session_lifecycle::host_registry::host_registry_dir(tddy_data_dir),
            ));
        let ss_svc =
            tddy_screen_sharing::ScreenSharingServiceImpl::new(ss_user_resolver, ss_sessions_base)
                .with_config(Arc::clone(config_arc))
                .with_room_tokens(Arc::clone(room_tokens))
                .with_host_scope(ss_host_targets, ss_host_keypair, Arc::clone(host_prompts))
                .with_credential_vaults(self.identity.credential_vaults.clone());
        // The entry comes from `tddy-screen-sharing` rather than being assembled here: the
        // subsystem's whole contract with this wiring layer is the `ServiceEntry` it returns.
        self.identity
            .rpc_entries
            .push(tddy_screen_sharing::build_screen_sharing_entry(ss_svc));
    }

    pub(super) fn mount_accounts_entry(&mut self, m: &Mounted) {
        let accounts_user_resolver = m.mount.user_resolver.clone();
        // AccountsService — what the caller's credential vault holds, never a secret. The session
        // token resolves to the GitHub login, which is the vault's subject. Not registered without
        // `auth_storage`: there is no vault to show, and nothing stands in for one.
        if let Some(vaults) = self.identity.credential_vaults.clone() {
            let mut accounts_service = crate::account_linking::accounts_service_over(
                &vaults,
                accounts_user_resolver,
                &self.identity.keys.auth_config,
            );
            accounts_service = self.wire_credential_sync(&vaults, accounts_service);
            self.identity
                .rpc_entries
                .push(tddy_accounts::build_accounts_entry(accounts_service));
        }
    }

    pub(super) fn wire_credential_sync(
        &mut self,
        vaults: &Arc<SessionVaults>,
        mut accounts_service: AccountsService,
    ) -> AccountsService {
        let config = &self.config;
        // `#keyring` 6/9's sync engine: journaled credential propagation to this daemon's
        // admitted peers, over the same common room as every other LiveKit-dependent service
        // above. Only when a room exists at all — regardless of whether `keyring.group_secret`
        // is set, see `credential_sync`'s module doc — so a daemon with LiveKit switched off
        // constructs no engine, and a misconfigured one still gets a real, journaling engine
        // rather than none.
        if let Some((registry, room_slot)) = self.credential_sync_room.take() {
            let local_instance_id =
                tddy_daemon_kernel::daemon_identity::local_instance_id_for_config(config);
            let common_room = tddy_daemon_kernel::peer_forwarding::CommonRoom::new(
                tddy_daemon_livekit::LiveKitPeerForwarder::new(room_slot.clone()),
                config,
            );
            let daemon_signing_key = Arc::new(self.identity.keys.signing_key.take().expect(
                "signing_key is set whenever session_tokens is set (both come from the \
                 same `Some(key)` branch above)",
            ));
            match crate::credential_sync::build(
                config,
                daemon_signing_key,
                Arc::clone(&registry),
                room_slot,
                common_room,
                &local_instance_id,
            ) {
                Ok(wiring) => {
                    accounts_service = accounts_service.with_sync_status(wiring.sync_status_source);
                    self.tasks.credential_sync_publish_watch = Some(CredentialSyncPublishWatch {
                        registry,
                        vaults: Arc::clone(vaults),
                        config: config.clone(),
                        engine: wiring.engine,
                    });
                }
                Err(e) => {
                    log::warn!(
                        target: "tddy_daemon::credential_sync",
                        "could not assemble #keyring 6/9's sync engine: {e:#}"
                    );
                }
            }
        }
        accounts_service
    }
}
