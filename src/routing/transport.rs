use super::RoutingError;
use serde_json::Value;
use std::{future::Future, pin::Pin};

/// Injectable read-only transport. Futures must release work when dropped.
/// No background tasks, retries, or independent timeout may outlive the future.
pub trait RoutingTransport: Send + Sync {
    fn get<'a>(
        &'a self,
        url: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<Value, RoutingError>> + Send + 'a>>;
}

pub struct HttpRoutingTransport(reqwest::Client);
impl HttpRoutingTransport {
    /// Builds a transport with redirects disabled; the caller owns the total deadline.
    /// # Errors
    /// Returns client initialization errors.
    pub fn new() -> Result<Self, reqwest::Error> {
        Ok(Self(
            reqwest::Client::builder()
                .redirect(reqwest::redirect::Policy::none())
                .build()?,
        ))
    }
}
impl RoutingTransport for HttpRoutingTransport {
    fn get<'a>(
        &'a self,
        url: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<Value, RoutingError>> + Send + 'a>> {
        Box::pin(async move {
            let started = std::time::Instant::now();
            let response = self.0.get(url).send().await.map_err(|error| {
                crate::diagnostics::routing_request_error(
                    url,
                    started.elapsed().as_millis(),
                    &error,
                );
                RoutingError::RpcUnavailable
            })?;
            if !response.status().is_success() {
                crate::diagnostics::routing_http_status(
                    url,
                    started.elapsed().as_millis(),
                    response.status().as_u16(),
                );
                return Err(RoutingError::RpcError);
            }
            response.json().await.map_err(|error| {
                crate::diagnostics::routing_request_error(
                    url,
                    started.elapsed().as_millis(),
                    &error,
                );
                RoutingError::Incomplete
            })
        })
    }
}
