# 2026-03-14 — TokenService RPC

**Type:** Feature

token.proto, TokenServiceImpl, TokenProvider trait. TokenService allows callers to generate and refresh LiveKit access tokens without holding API credentials. Delegates to TokenGenerator via TokenProvider. Integration tests: token_service_acceptance. (tddy-service)
