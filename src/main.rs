use std::{env, error::Error, net::Ipv4Addr};
use tokio::net::TcpListener;

fn main() -> Result<(), Box<dyn Error>> {
    match dotenvy::from_path(".env") {
        Ok(()) => {}
        Err(error) if error.not_found() => {}
        Err(_) => return Err("Failed to load .env".into()),
    }
    run()
}

#[tokio::main]
async fn run() -> Result<(), Box<dyn Error>> {
    let port = match env::var("PORT") {
        Ok(value) => value.parse::<u16>()?,
        Err(env::VarError::NotPresent) => 8080,
        Err(error) => return Err(error.into()),
    };
    let listener = TcpListener::bind((Ipv4Addr::UNSPECIFIED, port)).await?;
    eprintln!("Listening on {}", listener.local_addr()?);
    let endpoint = match env::var("GONKA_REST_URL") {
        Ok(value) => value,
        Err(env::VarError::NotPresent) => "https://node1.gonka.ai:8443/chain-api".to_owned(),
        Err(error) => return Err(error.into()),
    };
    let client = monitoring::chain::ChainClient::new(&endpoint)?;
    let cache = monitoring::cache::Cache::default();
    let worker = tokio::spawn(monitoring::refresh::run(client.clone(), cache.clone()));
    let result = axum::serve(listener, monitoring::create_app(client, cache))
        .with_graceful_shutdown(shutdown())
        .await;
    worker.abort();
    result?;
    Ok(())
}

async fn shutdown() {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{SignalKind, signal};
        if let Ok(mut terminate) = signal(SignalKind::terminate()) {
            tokio::select! {
                _ = tokio::signal::ctrl_c() => {},
                _ = terminate.recv() => {},
            }
            return;
        }
    }
    let _ = tokio::signal::ctrl_c().await;
}
