use crate::spec::CodingAgentSpec;

/// Agent-env entries (`coding_agent.pass_env`, an agent-env base set) refused when one would let an
/// agent inherit a `TDDY_*` variable — `TDDY_X`, `TDDY_*`, or a prefix pattern every `TDDY_*` name
/// matches (`TDDY*`, `T*`, `*`) — or is not a variable name nor a `PREFIX_*` pattern. The message
/// names the entry.
pub(crate) fn validate_env_entries(entries: &[String]) -> Result<(), String> {
    for entry in entries {
        if admits_a_tddy_variable(entry) {
            return Err(format!(
                "env entry '{entry}' would admit a TDDY_* variable, which no agent inherits"
            ));
        }
        if !is_an_env_entry(entry) {
            return Err(format!(
                "env entry '{entry}' is neither a variable name nor a PREFIX_* pattern"
            ));
        }
    }
    Ok(())
}

/// Whether `entry` (a name or a pattern) could name a `TDDY_*` variable.
pub(super) fn admits_a_tddy_variable(entry: &str) -> bool {
    const TDDY: &str = tddy_agent_backend::agent_env::TDDY_ENV_PREFIX;
    match entry.split_once('*') {
        // A prefix pattern admits every name starting with its prefix.
        Some((prefix, _)) => TDDY.starts_with(prefix) || prefix.starts_with(TDDY),
        None => entry == TDDY.trim_end_matches('_') || entry.starts_with(TDDY),
    }
}

/// Whether `entry` is a variable name (`PATH`) or a `PREFIX_*` pattern (`LC_*`).
fn is_an_env_entry(entry: &str) -> bool {
    let is_name = |text: &str| {
        text.chars()
            .next()
            .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
            && text.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
    };
    match entry.strip_suffix("_*") {
        Some(prefix) => is_name(&format!("{prefix}_")),
        None => is_name(entry),
    }
}

impl CodingAgentSpec {
    /// `coding_agent.pass_env`: the names and `PREFIX_*` patterns of the daemon variables the
    /// agent's process is also started with.
    pub fn pass_env(&self) -> &[String] {
        &self.agent.pass_env
    }

    /// The variables of `parent` (the daemon's environment) that [`Self::pass_env`] names, in
    /// `parent`'s order — what every host launch of this agent adds to its environment. A variable
    /// whose name or value is not UTF-8 is not passed.
    pub fn passed_env(
        &self,
        parent: impl IntoIterator<Item = (std::ffi::OsString, std::ffi::OsString)>,
    ) -> Vec<(String, String)> {
        parent
            .into_iter()
            .filter(|(name, _)| {
                self.agent.pass_env.iter().any(|pattern| {
                    tddy_agent_backend::agent_env::matches_env_pattern(pattern, name)
                })
            })
            .filter_map(|(name, value)| Some((name.into_string().ok()?, value.into_string().ok()?)))
            .collect()
    }
}
