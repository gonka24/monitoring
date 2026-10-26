# Live routing — Rust API v0.2.0

`GET /v1/deals/{deal}/routing` performs a fresh read-only observation. No query
parameters are accepted. `deal` must be a canonical lowercase Gonka Bech32
account/contract address (20 or 32 bytes); a non-contract fails identity checks.
This endpoint is independent of the background collector and its cached state.

The service pins `mainnet-v1` and its first node, using that node's `/chain-api`
and `/chain-rpc` together. Legacy `GONKA_REST_URL` / `GONKA_RPC_URL` configure the
collector/search only. Routing node selection is server-side, never an HTTP input.
Library users may construct a validated `RoutingDeployment` and select a node
from its pinned list. There is no failover in this version.

## Response

```json
{
  "schema": "gonka.forward.deal-routing.v1",
  "deployment_id": "mainnet-v1",
  "chain_id": "gonka-mainnet",
  "deal": "gonka19q5apa9m2zu4qjytr22ga4trk49szya52cepk0p5lsgt520xypsqv252hy",
  "host": "gonka1x2qapwtlprmlcxrdtvrskswuznn8v29r4se5tc",
  "target_epoch": "442",
  "current_epoch": "437",
  "recipient": "gonka19q5apa9m2zu4qjytr22ga4trk49szya52cepk0p5lsgt520xypsqv252hy",
  "state": "CONFIGURED",
  "checked_at": 1793000000,
  "source": "chain",
  "consistency": "live_state",
  "evidence": {
    "node": "https://node1.gonka.ai:8443",
    "start_height": "9000000",
    "end_height": "9000001",
    "factory": "gonka1ea2s57dvs06fr5mjzmna9w9vt0xylrna5cjtmzr5h72vkrza3xxsp7eayy",
    "registration_verified": true
  }
}
```

Illustrative JSON, not a fresh observation of that historical deal.

| state | recipient | Meaning and UI |
|---|---|---|
| `CONFIGURED` | equals deal | Current route matches; show Routing configured, including Open without funding/lock |
| `OTHER_RECIPIENT` | another valid address | Show actual recipient; action availability is separate |
| `NOT_CONFIGURED` | null | Successfully read list has no target entry, current < target; route may have existed previously |
| `HISTORY_UNKNOWN` | null | No target entry, current >= target; pruning may have removed history; do not advise rerouting into a closed deal |

An existing entry takes precedence over epoch comparisons. `null` is never an
error placeholder. `checked_at` is completion time in Unix seconds. Heights are
observation boundaries, **not an atomic snapshot**. Routes may change after reading.
The result says nothing about signing permission, funding, lock or runtime
compatibility for transactions.

Epochs and heights are canonical decimal strings. Rust supports the full `u64`
range, including when upstream JSON encodes an integer as a number; no float or
JavaScript intermediate is involved. Overflow, negative/fractional values and
noncanonical decimal strings are rejected. Clients must not coerce these strings
to JavaScript Number.

## Verification

One attempt reads tip, current epoch, contract info, config, exact Factory
`deal_by_host_epoch`, routes, current epoch and tip. It never requests state,
payments, events, `list_deals`, `tx_search`, runtime SHA or any transaction method.
The identity helper `RoutingClient::deal_identity` checks code ID, empty admin,
creator, factory, deal address, token, fee recipient/bps, protobuf pin, host and
registration. It can be reused by further read-only operations without loading
transactional state. Existing collector/search semantics remain unchanged.

Pins in `src/routing/mainnet-v1.json` come from the shipped
`@gonka24/forward-tools` archive `0.2.0-1bb6067-5946bdb` (`deployments/mainnet-v1.json`).
This profile is not a claim that the running network still matches it: each
observation checks the required identities. No runtime compatibility flag is read
or set, and transaction guards in other tools are unaffected.

Tip requires the pinned chain ID, catching_up=false, positive height and valid
RFC3339 block time with absolute clock difference <180 seconds, matching tools.
Every entry is validated, including entries for other epochs. Duplicate epochs
invalidate the entire observation. On epoch change the whole attempt repeats once
within the original deadline; repeated changes or height regression fail.

## Rust library

```rust,no_run
use monitoring::{RoutingClient, RoutingDeployment, RoutingOptions, CancellationToken,
                 parse_deal_routing_result};
use monitoring::routing::HttpRoutingTransport;
use std::sync::Arc;

# async fn example(deal: &str) -> Result<(), Box<dyn std::error::Error>> {
let deployment = RoutingDeployment::mainnet()?;
let client = RoutingClient::new(
    deployment.clone(), &deployment.nodes[0],
    Arc::new(HttpRoutingTransport::new()?),
)?;
let cancellation = CancellationToken::new();
let options = RoutingOptions { timeout_ms: 12_000, cancellation: cancellation.clone() };
// A request lifecycle owner can call cancellation.cancel().
let result = client.get_deal_routing(deal, options).await?;
let validated = parse_deal_routing_result(serde_json::to_value(&result)?)?;
assert_eq!(result, validated);
# Ok(()) }
```

`RoutingOptions::default()` sets a 12-second **total** deadline. Zero and
unrepresentable deadlines fail before any network calls. Explicit cancellation
returns `ABORTED`; dropping the operation future drops its in-flight request.
There are no spawned network tasks or independent transport timeout overriding
cancellation. HTTP client disconnect cancellation depends on the server/proxy
propagating the disconnect; the total deadline always bounds work. Direct library
callers should tie a CancellationToken to their request lifecycle.

Concurrent observations share immutable pins and a connection pool, not mutable
transport settings. `RoutingTransport` is injectable for deterministic tests; an
implementation must release work when its future is dropped. The low-level
`deal_identity` helper inherits the deadline of its caller and should be wrapped
in a timeout when used independently.

The pure `parse_deal_routing_result(Value)` validates required fields, addresses,
integers, evidence and state/recipient/epoch consistency; it rejects unknown
fields. Use it instead of unchecked serde deserialization for untrusted JSON.
This is a Rust validator, not a JavaScript package export or WASM/browser build.

## Errors and HTTP

All endpoint responses carry `Cache-Control: no-store`. Errors use
`{"error":{"code":"…","message":"Could not verify routing"}}`; raw RPC errors
are never exposed. Failed observations return no partial result.

| Codes | HTTP |
|---|---|
| `INVALID_ADDRESS`, `INVALID_ARGUMENT` | 400 |
| `RATE_LIMITED` | 429 + Retry-After: 1 |
| `INVALID_DEPLOYMENT`, `WRONG_CHAIN`, `STALE_RPC` | 503 |
| `DEAL_MISMATCH`, `DEAL_UNREGISTERED`, `ROUTING_INVALID`, `INCOMPLETE` | 503 |
| `RPC_ERROR`, `RPC_UNAVAILABLE`, `RPC_TIMEOUT`, `ABORTED`, `OBSERVATION_CHANGED` | 503 |

HTTP admits 8 concurrent observations per service instance and rejects excess
immediately. Input validation occurs before admission. Library callers manage
their own admission policy. No hidden cache or unbounded retries exist.

## Dashboard migration

BFF calls this service, keeping auth/brand guards, query validation, concurrency
policy, request cancellation, safe error mapping and `no-store`. Its service URL
is trusted application configuration.

```ts
const response = await fetch(
  `${monitoringUrl}/v1/deals/${encodeURIComponent(deal)}/routing`,
  { signal: request.signal, cache: "no-store" },
);
// Forward the documented body/status after applying the application's error policy.
```

Remove manual tip/currentEpoch/config/Factory/routes RPCs, recipient selection,
classification, and transport workarounds from
`apps/dashboard/src/app/api/forward/routing/route.ts` and `src/lib/forward-routing.ts`.
UI consumes `state` without address/epoch comparison. Keep polling every 30 seconds,
manual retry and refresh after INCLUDED + RESULT_VERIFIED. On errors show
“Could not verify routing”; never display an old observation as fresh.
`forward-lifecycle.ts`, `useDealRouting.ts`, `ForwardDeal.tsx` and `ActionReview.tsx`
should consume this response contract. Existing funding/lock history can remain
separate evidence, never a fallback for recipient.

The Dashboard lives in another repository; this release supplies the Rust
service/API and migration contract, and does not change those UI files.

## Validation and delivery

`./scripts/check` runs formatting, strict Clippy and all tests. `tests/routing.rs`
uses mock transport (plus a local HTTP fixture for transport error parsing), no
live chain, wallets, signing or broadcast. It covers classification, identity,
precision, bounded retries, deadlines, cancellation/drop at every stage,
parallel isolation, parser invariants and HTTP admission/errors.

Package: `cargo package --allow-dirty` produces
`target/package/monitoring-0.2.0.crate` and verifies that the archived sources build.
This is a Rust source archive, not an npm SDK replacement. Build the service with
`cargo build --release --locked` or the existing Docker workflow.
