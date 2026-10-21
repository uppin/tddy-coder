# 2026-10-20 — One Connect per desktop channel on a host row

**Type:** Feature

`#host-desktops` 7/8 ([#659](https://github.com/uppin/tddy-coder/pull/659)). `HostRowRemoteDesktop`
renders a `ChannelReading` per reading (X11 channels as `hosts-row-<id>-channel-<channelId>`,
labelled "Display :N"), with a Connect beside each connectable one (`data-channel-id`;
`connectableChannels`). `HostDesktopOverlay` takes `x11Display` and finds or adds the channel's
target (`targetForChannel`, `isTargetOfChannel`). Page object: `hostDesktopChannelsPage`. Spec:
`HostsScreenDesktopChannelsAcceptance.cy.tsx` (5).
