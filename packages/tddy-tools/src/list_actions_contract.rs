//! Contract metadata for `list-actions` JSON (acceptance-tests session actions automation).

use log::debug;

/// JSON field `acceptance_tests_session_actions_contract_version` for automation that pairs
/// `list-actions` output with acceptance-tests session action deliverables (PRD).
pub const ACCEPTANCE_TESTS_SESSION_ACTIONS_CONTRACT_VERSION: u64 = 1;

/// Value merged into [`crate::session_actions_cli::ListActionsResponse`].
pub fn acceptance_tests_session_actions_contract_version() -> Option<u64> {
    debug!(
        target: "tddy_tools::list_actions_contract",
        "acceptance_tests_session_actions_contract_version version={}",
        ACCEPTANCE_TESTS_SESSION_ACTIONS_CONTRACT_VERSION
    );
    Some(ACCEPTANCE_TESTS_SESSION_ACTIONS_CONTRACT_VERSION)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contract_version_is_one_for_list_actions_automation() {
        assert_eq!(
            acceptance_tests_session_actions_contract_version(),
            Some(1u64),
            "contract version must be Some(1) for list-actions automation envelope"
        );
    }
}
