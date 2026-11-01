//! Deliberately bounded diagnostics: never log URLs, queries, bodies or error text.
use serde_json::{Value, json};

fn record(operation: &str, reason: &str, status: Option<u16>) -> Value {
    json!({"severity": "ERROR", "component": "monitoring", "operation": operation,
        "reason": reason, "upstream_status": status})
}

pub(crate) fn failure(operation: &str, reason: &str) {
    eprintln!("{}", record(operation, reason, None));
}

pub(crate) fn pool_failure(index: usize) {
    let mut entry = record("pool.node", "cooldown", None);
    entry["node_index"] = json!(index);
    eprintln!("{entry}");
}

fn request_record(operation: &str, error: &reqwest::Error) -> Value {
    let reason = if error.is_timeout() {
        "timeout"
    } else if error.is_connect() {
        "connect"
    } else if error.is_status() {
        "http_status"
    } else if error.is_decode() {
        "decode"
    } else {
        "transport"
    };
    record(operation, reason, error.status().map(|s| s.as_u16()))
}

pub(crate) fn request_error(operation: &str, error: &reqwest::Error) {
    eprintln!("{}", request_record(operation, error));
}

/// Only fixed endpoint labels and the hostname survive; never log a raw URL.
fn routing_record(url: &str, elapsed_ms: u128, mut entry: Value) -> Value {
    if let Ok(url) = reqwest::Url::parse(url) {
        entry["upstream_host"] = json!(url.host_str());
        entry["upstream_port"] = json!(url.port_or_known_default());
        let path = url.path();
        entry["endpoint"] = json!(match path {
            "/chain-rpc/status" => "status",
            "/chain-api/productscience/inference/inference/get_current_epoch" => "epoch",
            _ if path.starts_with("/chain-api/cosmwasm/wasm/v1/contract/") => {
                if path.contains("/smart/") {
                    "contract_smart"
                } else {
                    "contract_info"
                }
            }
            _ if path.starts_with(
                "/chain-api/productscience/inference/inference/list_claim_recipients/"
            ) =>
                "claim_recipients",
            _ => "other",
        });
    }
    entry["elapsed_ms"] = json!(elapsed_ms);
    entry
}

pub(crate) fn routing_http_status(url: &str, elapsed_ms: u128, status: u16) {
    eprintln!(
        "{}",
        routing_record(
            url,
            elapsed_ms,
            record("routing.http", "http_status", Some(status))
        )
    );
}

pub(crate) fn routing_request_error(url: &str, elapsed_ms: u128, error: &reqwest::Error) {
    eprintln!(
        "{}",
        routing_record(url, elapsed_ms, request_record("routing.http", error))
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn routing_context_redacts_credentials_queries_and_dynamic_paths() {
        let entry = routing_record(
            "https://user:secret@node1.gonka.ai:8443/chain-api/cosmwasm/wasm/v1/contract/private-address/smart/private-message?token=private-token",
            250,
            record("routing.http", "http_status", Some(503)),
        );
        assert_eq!(entry["upstream_host"], "node1.gonka.ai");
        assert_eq!(entry["upstream_port"], 8443);
        assert_eq!(entry["endpoint"], "contract_smart");
        assert_eq!(entry["elapsed_ms"], 250);
        let encoded = entry.to_string();
        for sensitive in ["user", "secret", "private", "token"] {
            assert!(!encoded.contains(sensitive));
        }
    }

    #[tokio::test]
    async fn classifies_http_and_decode_errors_without_leaking_request_data()
    -> Result<(), Box<dyn std::error::Error>> {
        use axum::{Router, http::StatusCode, routing::get};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
        let base = format!("http://{}", listener.local_addr()?);
        let app = Router::new()
            .route(
                "/status",
                get(|| async { (StatusCode::SERVICE_UNAVAILABLE, "private body") }),
            )
            .route("/decode", get(|| async { "private invalid JSON" }));
        let server = tokio::spawn(async move { axum::serve(listener, app).await });
        let client = reqwest::Client::new();
        for (path, reason, status) in [
            ("status", "http_status", Some(503)),
            ("decode", "decode", None),
        ] {
            let response = client
                .get(format!("{base}/{path}?token=private-query"))
                .send()
                .await?;
            let error = if path == "status" {
                response
                    .error_for_status()
                    .err()
                    .ok_or("expected HTTP error")?
            } else {
                response
                    .json::<Value>()
                    .await
                    .err()
                    .ok_or("expected decode error")?
            };
            let actual = request_record("test.request", &error);
            assert_eq!(actual["reason"], reason);
            assert_eq!(actual["upstream_status"], json!(status));
            assert_eq!(actual["severity"], "ERROR");
            let encoded = actual.to_string();
            assert!(!encoded.contains("private"));
            assert!(!encoded.contains(&base));
            assert!(!encoded.contains("token"));
        }
        server.abort();
        Ok(())
    }
}
