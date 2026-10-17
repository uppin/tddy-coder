# 2026-10-17 — A clone asks for an SSH key passphrase

**Type:** Feature

`stream_relay::Relay` carries a forwarded stream's frames with a per-frame idle timeout, separated from `CommonRoom` so the policy runs without a LiveKit room. Behaviour of the forwarded stream is unchanged. See [the kernel documentation](../daemon-kernel.md).
