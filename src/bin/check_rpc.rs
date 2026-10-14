use std::{env, error::Error, time::Duration};

fn main() -> Result<(), Box<dyn Error>> {
    let mut args = env::args().skip(1);
    let endpoint = args.next().ok_or("Usage: check_rpc RPC_URL CHAIN_ID")?;
    let chain = args
        .next()
        .filter(|s| !s.is_empty())
        .ok_or("Missing chain ID")?;
    if args.next().is_some() {
        return Err("Unexpected argument".into());
    }
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(10))
        .redirect(reqwest::redirect::Policy::none())
        .build()?;
    let status: serde_json::Value = client
        .get(format!("{}/status", endpoint.trim_end_matches('/')))
        .send()
        .and_then(reqwest::blocking::Response::error_for_status)
        .and_then(reqwest::blocking::Response::json)
        .map_err(|_| "RPC request failed (transport, HTTP or JSON)")?;
    let height = verified_height(&status, &chain)?;
    println!("chain_id={chain} height={height} catching_up=false");
    Ok(())
}

fn verified_height(status: &serde_json::Value, chain: &str) -> Result<u64, &'static str> {
    if status.get("error").is_some()
        || status["result"]["node_info"]["network"].as_str() != Some(chain)
        || status["result"]["sync_info"]["catching_up"].as_bool() != Some(false)
    {
        return Err("RPC error, wrong chain or node not synchronized");
    }
    status["result"]["sync_info"]["latest_block_height"]
        .as_str()
        .and_then(|s| s.parse().ok())
        .filter(|height| *height > 0)
        .ok_or("Missing or invalid block height")
}

#[cfg(test)]
mod tests {
    use super::verified_height;
    use serde_json::json;

    #[test]
    fn validates_node_identity_and_sync() {
        let status = json!({"result": {"node_info": {"network": "test"},
            "sync_info": {"catching_up": false, "latest_block_height": "42"}}});
        assert_eq!(verified_height(&status, "test"), Ok(42));
        assert!(verified_height(&status, "other").is_err());
        for (pointer, value) in [
            ("/result/sync_info/catching_up", json!(true)),
            ("/result/sync_info/catching_up", json!(null)),
            ("/result/sync_info/latest_block_height", json!("0")),
            ("/result/sync_info/latest_block_height", json!("invalid")),
        ] {
            let mut invalid = status.clone();
            if let Some(field) = invalid.pointer_mut(pointer) {
                *field = value;
            }
            assert!(verified_height(&invalid, "test").is_err());
        }
        assert!(verified_height(&json!({"error": {"code": -1}}), "test").is_err());
    }
}
