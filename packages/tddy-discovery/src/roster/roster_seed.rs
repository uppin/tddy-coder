//! The roster a spawning daemon already read, handed to the agent it spawns so the agent's first
//! `tools/list` is right before any stream has delivered a frame.
//!
//! Distinct from `TDDY_SUBAGENTS_JSON` on purpose. That variable carries **defs** — endpoint,
//! model, credential — and a seeded def is what this process runs an agent's turn loop from
//! in-process (`LiveAgentRoster::local_def_for`). A host-run agent's conversations go through the
//! daemon, and its host may not reach the model at all, so seeding it with defs would reroute
//! them and export a credential into its environment. What it lacks is only the roster's
//! **entries** — who is attached, and what each took over — which is exactly a
//! `SessionAgentRoster` snapshot, applied the way a stream frame is.
//!
//! The snapshot travels as the hex of its protobuf encoding: the message has no serde form, and a
//! roster is a few hundred bytes, so a base64 dependency would buy nothing hex does not.

use prost::Message;
use tddy_core::spawn_env::env_non_empty;
use tddy_service::proto::session_agents_svc::SessionAgentRoster;

/// The variable the roster seed travels in.
pub const ROSTER_SEED_ENV: &str = "TDDY_SESSION_AGENT_ROSTER_SEED";

/// The env pair that hands `roster` to a spawned agent.
pub fn roster_seed_env(roster: &SessionAgentRoster) -> (String, String) {
    let hex = roster
        .encode_to_vec()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    (ROSTER_SEED_ENV.to_string(), hex)
}

/// The roster the spawn handed over, or `None` when it handed none.
///
/// A value that is set and does not decode is an error, never "no seed": the daemon that spawned
/// this process read a roster and meant it to be in force from the first answer, and silently
/// starting without it is the failure this seed exists to remove.
pub fn roster_seed_from_env() -> Result<Option<SessionAgentRoster>, String> {
    env_non_empty(ROSTER_SEED_ENV)
        .map(|value| roster_seed_from_value(&value))
        .transpose()
}

/// The roster a [`ROSTER_SEED_ENV`] value carries — the inverse of [`roster_seed_env`].
pub fn roster_seed_from_value(value: &str) -> Result<SessionAgentRoster, String> {
    let bytes = decode_hex(value)
        .ok_or_else(|| format!("{ROSTER_SEED_ENV} is set but is not a hex-encoded roster"))?;
    SessionAgentRoster::decode(bytes.as_slice())
        .map_err(|e| format!("{ROSTER_SEED_ENV} is set but does not decode as a roster: {e}"))
}

fn decode_hex(hex: &str) -> Option<Vec<u8>> {
    if !hex.len().is_multiple_of(2) {
        return None;
    }
    (0..hex.len())
        .step_by(2)
        .map(|at| u8::from_str_radix(hex.get(at..at + 2)?, 16).ok())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tddy_service::proto::session_agents_svc::SessionAgentEntry;

    fn a_roster() -> SessionAgentRoster {
        SessionAgentRoster {
            rev: 3,
            agents: vec![SessionAgentEntry {
                agent_id: "Gemma Local Coder@a-daemon".to_string(),
                replaces: vec!["Grep".to_string()],
                ..Default::default()
            }],
            ..Default::default()
        }
    }

    #[test]
    fn a_roster_survives_the_trip_through_its_env_value() {
        let (key, value) = roster_seed_env(&a_roster());
        assert_eq!(key, ROSTER_SEED_ENV);
        assert_eq!(roster_seed_from_value(&value), Ok(a_roster()));
    }

    #[test]
    fn a_value_that_is_hex_but_not_a_roster_is_refused_naming_the_variable() {
        let refusal = roster_seed_from_value("ff").expect_err("0xff is not a roster");
        assert!(
            refusal.contains(ROSTER_SEED_ENV),
            "names the variable: {refusal}"
        );
    }

    #[test]
    fn a_value_that_is_not_hex_is_refused() {
        assert_eq!(decode_hex("zz"), None);
        assert_eq!(
            decode_hex("abc"),
            None,
            "an odd length cannot be whole bytes"
        );
    }
}
