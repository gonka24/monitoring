//! Deliberately bounded diagnostics: never log URLs, queries, bodies or error text.
use serde_json::{Value, json};

fn record(operation: &str, reason: &str, status: Option<u16>) -> Value {
    json!({"severity": "ERROR", "component": "monitoring", "operation": operation,
        "reason": reason, "upstream_status": status})
}

pub(crate) fn failure(operation: &str, reason: &str) {
    eprintln!("{}", record(operation, reason, None));
}

pub(crate) fn http_status(operation: &str, status: u16) {
    eprintln!("{}", record(operation, "http_status", Some(status)));
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

#[cfg(test)]
mod tests {
    use super::*;

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
