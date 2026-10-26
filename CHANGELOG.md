# Changelog

## 0.2.0

- Add live read-only routing observations at `GET /v1/deals/{deal}/routing`.
- Export Rust client, four-state result, pure wire parser, deployment profile and stable errors.
- Verify full deal identity, Factory registration, network freshness and exact target epoch.
- Bound the full operation to 12 seconds by default; support cancellation, one full retry on epoch change and isolated concurrent calls.
- Add deterministic transport/HTTP tests and Dashboard migration documentation.
