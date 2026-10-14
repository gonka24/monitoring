use axum::{Router, http::StatusCode, routing::get};
use monitoring::{
    chain::{ChainClient, ChainError},
    deal::DealStatus,
};

#[tokio::test]
async fn reads_state_and_handles_failures() -> Result<(), Box<dyn std::error::Error>> {
    let address = "gonka1kxlsujae2mgahx2grd8hq8clhsk2zwsqk8q8esx6n2cpj426sdts9cdejs";
    let path = format!("/cosmwasm/wasm/v1/contract/{address}/smart/eyJzdGF0ZSI6e319");
    let fixture = include_str!("fixtures/deal-state.json");
    for (status, body, expected) in [
        (StatusCode::OK, fixture, Ok(DealStatus::Releasing)),
        (
            StatusCode::SERVICE_UNAVAILABLE,
            "",
            Err(ChainError::Upstream),
        ),
        (StatusCode::OK, "{}", Err(ChainError::Upstream)),
    ] {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
        let client = ChainClient::new(&format!("http://{}", listener.local_addr()?))?;
        let app = Router::new().route(&path, get(move || async move { (status, body) }));
        let server = tokio::spawn(async move { axum::serve(listener, app).await });
        let result = client.state(address).await.map(|state| state.status);
        server.abort();
        assert_eq!(result, expected);
        assert_eq!(
            client.state("../status").await,
            Err(ChainError::InvalidAddress)
        );
    }
    Ok(())
}
