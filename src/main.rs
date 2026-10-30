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
    // Explicit pool configuration takes precedence over legacy endpoint variables.
    // Existing deployments retain their configured single endpoint until migrated.
    let pool_urls = match env::var("GONKA_NODE_URLS") {
        Ok(value) => Some(value),
        Err(env::VarError::NotPresent)
            if env::var_os("GONKA_REST_URL").is_none()
                && env::var_os("GONKA_RPC_URL").is_none() =>
        {
            Some("https://node1.gonka.ai:8443,https://node3.gonka.ai".into())
        }
        Err(env::VarError::NotPresent) => None,
        Err(error) => return Err(error.into()),
    };
    let pool = pool_urls
        .map(|urls| {
            monitoring::pool::NodePool::new(
                &urls
                    .split(',')
                    .map(|s| s.trim().to_owned())
                    .collect::<Vec<_>>(),
            )
        })
        .transpose()?;
    let endpoint = match env::var("GONKA_REST_URL") {
        Ok(value) => value,
        Err(env::VarError::NotPresent) => "https://node1.gonka.ai:8443/chain-api".into(),
        Err(error) => return Err(error.into()),
    };
    let rpc_endpoint = match env::var("GONKA_RPC_URL") {
        Ok(value) => value,
        Err(env::VarError::NotPresent) => "https://node1.gonka.ai:8443/chain-rpc".into(),
        Err(error) => return Err(error.into()),
    };
    let client = monitoring::chain::ChainClient::new(&endpoint)?;
    let rpc = monitoring::rpc::RpcClient::new(&rpc_endpoint)?;
    let search = if let Some(pool) = &pool {
        monitoring::search::NetworkSearch::with_pool(pool.clone())
            .map_err(|_| "Cannot initialize node pool")?
    } else {
        monitoring::search::NetworkSearch::new(client.clone(), rpc.clone())
    };
    let deployment = monitoring::routing::RoutingDeployment::mainnet()?;
    // Routing uses a single pinned base for REST and RPC. Never combine the
    // independently configurable legacy collector endpoints for an observation.
    let routing = monitoring::routing::RoutingClient::new(
        deployment.clone(),
        &deployment.nodes[0],
        std::sync::Arc::new(monitoring::routing::HttpRoutingTransport::new()?),
    )?;
    let cache = monitoring::cache::Cache::with_search(search).with_routing(routing);
    let worker = if let Some(pool) = pool {
        tokio::spawn(monitoring::refresh::run_pool(pool, cache.clone()))
    } else {
        tokio::spawn(monitoring::refresh::run(client, rpc, cache.clone()))
    };
    let result = axum::serve(listener, monitoring::create_app(cache))
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
