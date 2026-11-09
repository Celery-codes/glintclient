pub mod fabric;
pub mod mojang;
pub mod rules;

use std::path::Path;

use serde::de::DeserializeOwned;

use crate::{Result, fs_util};

/// Fetch JSON that changes over time (version lists). The network copy wins and
/// refreshes the cache; the cache is the fallback so the launcher still works offline.
pub(crate) async fn fetch_json_cached<T: DeserializeOwned>(
    client: &reqwest::Client,
    url: &str,
    cache: &Path,
) -> Result<T> {
    let fresh = async {
        let bytes = client
            .get(url)
            .send()
            .await?
            .error_for_status()?
            .bytes()
            .await?;
        let value: T = serde_json::from_slice(&bytes)?;
        Ok::<_, crate::Error>((value, bytes))
    };
    match fresh.await {
        Ok((value, bytes)) => {
            // A cache write failure shouldn't fail the request that just succeeded.
            let _ = fs_util::write_atomic(cache, &bytes).await;
            Ok(value)
        }
        Err(network_error) => match fs_util::read_json_opt(cache).await {
            Ok(Some(value)) => Ok(value),
            _ => Err(network_error),
        },
    }
}
