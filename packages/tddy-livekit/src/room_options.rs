//! Room options for tddy's own LiveKit connections.
//!
//! The livekit SDK default gathers ICE candidates with
//! [`ContinualGatheringPolicy::GatherContinually`]: every network change (a DHCP
//! renewal, a docker veth appearing or disappearing) re-gathers, and the superseded
//! host-candidate sockets are leaked by the gatherer — on a multi-homed host this
//! exhausted the daemon's file descriptors within two days of uptime
//! (livekit/rust-sdks#1512). Tddy rooms ride out network changes through the SDK's
//! reconnect flow instead, which builds a fresh connection that gathers fresh
//! candidates, so gathering once at connect time is enough — which is why
//! [`IceGathering::GatherOnce`] is also the config default in `tddy-daemon-kernel`.
//! An operator who needs established sessions to survive a network switch without a
//! reconnect cycle configures `gather_continually` knowingly.

use livekit::webrtc::prelude::ContinualGatheringPolicy;
use livekit::RoomOptions;

/// Room options tddy uses for its own LiveKit connections — the SDK default with the
/// ICE gathering policy set to `ice_gathering`.
pub fn tddy_room_options(ice_gathering: ContinualGatheringPolicy) -> RoomOptions {
    let mut options = RoomOptions::default();
    options.rtc_config.continual_gathering_policy = ice_gathering;
    options
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gathers_ice_candidates_once_when_configured_to() {
        // Given / When
        let options = tddy_room_options(ContinualGatheringPolicy::GatherOnce);

        // Then
        assert_eq!(
            options.rtc_config.continual_gathering_policy,
            ContinualGatheringPolicy::GatherOnce,
            "continual gathering leaks the superseded ICE sockets on every network \
             change (livekit/rust-sdks#1512) and exhausted the daemon's fd limit"
        );
    }

    #[test]
    fn carries_the_configured_continual_gathering_through() {
        // Given / When
        let options = tddy_room_options(ContinualGatheringPolicy::GatherContinually);

        // Then
        assert_eq!(
            options.rtc_config.continual_gathering_policy,
            ContinualGatheringPolicy::GatherContinually
        );
    }
}
