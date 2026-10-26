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
            let response = self
                .0
                .get(url)
                .send()
                .await
                .map_err(|_| RoutingError::RpcUnavailable)?;
            if !response.status().is_success() {
                return Err(RoutingError::RpcError);
            }
            response.json().await.map_err(|_| RoutingError::Incomplete)
        })
    }
}
