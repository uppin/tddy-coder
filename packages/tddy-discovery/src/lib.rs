//! Discovery agent — OpenAI-compatible multi-turn codebase exploration.

pub mod agent_def;
pub mod backend;
pub mod discovery;
pub mod openai;
pub mod subagent;
pub mod tools;
pub mod warmup;

/// The live roster of agents attached to a session, and the conversation runtime over them.
///
/// Moved here from `tddy-tools` by `#unbundle` node 5, and this crate is the right home because it
/// already owns every type the runtime manipulates — `subagent::{SubagentSession, SubagentRegistry,
/// PromptOutcome, CodebaseAccess, StopReason}`, `agent_def::SpecializedAgentDef` and
/// `openai::TokenUsage`. What lived in `tddy-tools` was a *stateful layer over this crate's traits*
/// that happened to be compiled into a binary.
///
/// **Node 7 serves `session_agents.SessionAgentService` in front of this**, so its shape is fixed
/// here rather than in the node that consumes it.
pub mod roster {
    /// Whether the roster's view is current enough to answer with.
    ///
    /// A roster read from a stream that has since dropped is *stale*, not empty — and answering
    /// "no agents" from a stale roster is how a picker offers nothing while an agent is attached.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum RosterCurrency {
        Current,
        Stale,
    }

    /// Whether a session's exec tools are the host's or an agent's.
    ///
    /// A takeover has to be visible: two things claiming to serve `Read` resolve by ordering, and
    /// an agent that thinks it owns a tool it does not will report work it never did.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub enum CatalogVisibility {
        HostTools,
        TakenOverBy { agent_id: String },
    }

    /// The agents attached to a session, and what they have taken over.
    #[derive(Debug, Default)]
    pub struct LiveAgentRoster {
        // TODO(tools-thinning): implement
    }

    impl LiveAgentRoster {
        /// Whether this roster's view is current.
        pub fn currency(&self) -> RosterCurrency {
            // TODO(tools-thinning): implement
            unimplemented!("LiveAgentRoster::currency")
        }

        /// Which agent, if any, may be addressed for this session.
        pub fn addressable(&self) -> Option<String> {
            // TODO(tools-thinning): implement
            unimplemented!("LiveAgentRoster::addressable")
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        /// A roster that has never been populated is current and empty. That is different from a
        /// roster whose stream dropped, and conflating the two is how a picker offers no agents
        /// while one is attached.
        #[test]
        fn a_fresh_roster_is_current_rather_than_stale() {
            // Given
            let roster = LiveAgentRoster::default();

            // When
            let currency = roster.currency();

            // Then
            assert_eq!(currency, RosterCurrency::Current);
        }

        #[test]
        fn has_no_addressable_agent_before_one_attaches() {
            // Given
            let roster = LiveAgentRoster::default();

            // When
            let addressable = roster.addressable();

            // Then
            assert_eq!(addressable, None);
        }

        /// A takeover names the agent, because "the host no longer serves Read" is not actionable
        /// without knowing who does.
        #[test]
        fn a_takeover_names_the_agent_that_owns_the_tool() {
            let taken = CatalogVisibility::TakenOverBy {
                agent_id: "reviewer@daemon-7".to_string(),
            };
            assert_ne!(taken, CatalogVisibility::HostTools);
        }
    }
}
