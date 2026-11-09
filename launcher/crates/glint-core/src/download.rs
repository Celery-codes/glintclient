use std::{
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};

use futures::StreamExt;
use serde::Serialize;
use sha1::Digest;
use tokio::io::AsyncWriteExt;

use crate::{Error, Result, fs_util};

const MAX_ATTEMPTS: u32 = 4;
const PROGRESS_INTERVAL: Duration = Duration::from_millis(80);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Hash {
    Sha1(String),
    Sha256(String),
}

#[derive(Debug, Clone)]
pub struct Download {
    pub url: String,
    pub path: PathBuf,
    pub hash: Option<Hash>,
    pub size: Option<u64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Progress {
    /// What is being worked on, e.g. `libraries`, `assets`, `java`.
    pub stage: &'static str,
    pub done: u64,
    pub total: u64,
}

pub type ProgressFn = Arc<dyn Fn(Progress) + Send + Sync>;

pub fn no_progress() -> ProgressFn {
    Arc::new(|_| {})
}

/// Whether the file on disk can be trusted without downloading it again.
///
/// With a known size this is an existence and size check, which keeps launching an
/// already-installed game fast (hashing ~500 MB of assets every launch is not). Every
/// file is hash-verified when it is downloaded. `deep` forces a full hash check.
pub async fn is_valid(item: &Download, deep: bool) -> bool {
    let Ok(meta) = tokio::fs::metadata(&item.path).await else {
        return false;
    };
    if !meta.is_file() {
        return false;
    }
    if let Some(size) = item.size {
        if meta.len() != size {
            return false;
        }
        if !deep {
            return true;
        }
    }
    match &item.hash {
        Some(hash) => hash_file(&item.path, hash)
            .await
            .is_ok_and(|actual| actual == hash_hex(hash)),
        None => true,
    }
}

fn hash_hex(hash: &Hash) -> &str {
    match hash {
        Hash::Sha1(h) | Hash::Sha256(h) => h,
    }
}

enum Hasher {
    Sha1(sha1::Sha1),
    Sha256(sha2::Sha256),
}

impl Hasher {
    fn for_hash(hash: &Hash) -> Self {
        match hash {
            Hash::Sha1(_) => Hasher::Sha1(sha1::Sha1::new()),
            Hash::Sha256(_) => Hasher::Sha256(sha2::Sha256::new()),
        }
    }

    fn update(&mut self, data: &[u8]) {
        match self {
            Hasher::Sha1(h) => h.update(data),
            Hasher::Sha256(h) => h.update(data),
        }
    }

    fn finish(self) -> String {
        match self {
            Hasher::Sha1(h) => hex::encode(h.finalize()),
            Hasher::Sha256(h) => hex::encode(h.finalize()),
        }
    }
}

/// Hex digest of a file, using the algorithm of `like`.
pub async fn hash_file(path: &Path, like: &Hash) -> Result<String> {
    let path = path.to_owned();
    let mut hasher = Hasher::for_hash(like);
    tokio::task::spawn_blocking(move || -> Result<String> {
        use std::io::Read;
        let mut file = std::fs::File::open(path)?;
        let mut buf = vec![0u8; 1 << 16];
        loop {
            let n = file.read(&mut buf)?;
            if n == 0 {
                break;
            }
            hasher.update(&buf[..n]);
        }
        Ok(hasher.finish())
    })
    .await
    .map_err(|e| Error::Other(e.to_string()))?
}

/// Download one file to its destination, verifying the hash, with retries.
pub async fn fetch(client: &reqwest::Client, item: &Download) -> Result<()> {
    let mut attempt = 1;
    loop {
        match fetch_once(client, item).await {
            Ok(()) => return Ok(()),
            Err(e) if attempt < MAX_ATTEMPTS && is_retryable(&e) => {
                tokio::time::sleep(Duration::from_millis(300 * u64::from(attempt * attempt))).await;
                attempt += 1;
            }
            Err(e) => return Err(e),
        }
    }
}

fn is_retryable(e: &Error) -> bool {
    match e {
        // 4xx (other than timeouts and rate limits) won't fix itself.
        Error::Http(e) => e
            .status()
            .is_none_or(|s| s.is_server_error() || s.as_u16() == 408 || s.as_u16() == 429),
        Error::Checksum { .. } => true,
        _ => false,
    }
}

async fn fetch_once(client: &reqwest::Client, item: &Download) -> Result<()> {
    if let Some(parent) = item.path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    let tmp = fs_util::tmp_path(&item.path);
    let response = client.get(&item.url).send().await?.error_for_status()?;
    let mut hasher = item.hash.as_ref().map(Hasher::for_hash);
    let mut file = tokio::fs::File::create(&tmp).await?;
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        if let Some(h) = hasher.as_mut() {
            h.update(&chunk);
        }
        file.write_all(&chunk).await?;
    }
    file.flush().await?;
    drop(file);

    if let (Some(hasher), Some(expected)) = (hasher, item.hash.as_ref()) {
        let actual = hasher.finish();
        if !actual.eq_ignore_ascii_case(hash_hex(expected)) {
            let _ = tokio::fs::remove_file(&tmp).await;
            return Err(Error::Checksum {
                file: item.url.clone(),
                expected: hash_hex(expected).to_string(),
                actual,
            });
        }
    }
    tokio::fs::rename(&tmp, &item.path).await?;
    Ok(())
}

/// Bring every item up to date, downloading only what is missing or the wrong size.
///
/// Progress counts files and is throttled, so thousands of small assets don't flood the UI.
pub async fn fetch_all(
    client: &reqwest::Client,
    items: Vec<Download>,
    concurrency: usize,
    stage: &'static str,
    progress: &ProgressFn,
) -> Result<()> {
    let total = items.len() as u64;
    let done = AtomicU64::new(0);
    let last_report = Mutex::new(Instant::now() - PROGRESS_INTERVAL);
    let report = |force: bool| {
        let mut last = last_report.lock().unwrap();
        if force || last.elapsed() >= PROGRESS_INTERVAL {
            *last = Instant::now();
            progress(Progress {
                stage,
                done: done.load(Ordering::Relaxed),
                total,
            });
        }
    };
    report(true);

    // Items are moved into their futures: a closure over `&Download` makes the resulting
    // future impossible to prove `Send` when this runs inside a spawned task.
    let mut results = futures::stream::iter(items.into_iter().map(|item| {
        let done = &done;
        let report = &report;
        async move {
            if !is_valid(&item, false).await {
                fetch(client, &item).await?;
            }
            done.fetch_add(1, Ordering::Relaxed);
            report(false);
            Ok::<(), Error>(())
        }
    }))
    .buffer_unordered(concurrency.max(1));

    while let Some(result) = results.next().await {
        result?;
    }
    report(true);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn validity_checks_size_then_hash() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.txt");
        tokio::fs::write(&path, b"hello").await.unwrap();
        let sha1 = "aaf4c61ddcc5e8a2dabede0f3b482cd9aea9434d".to_string();
        let mut item = Download {
            url: String::new(),
            path: path.clone(),
            hash: Some(Hash::Sha1(sha1)),
            size: Some(5),
        };
        assert!(is_valid(&item, false).await);
        assert!(is_valid(&item, true).await);

        item.size = Some(6);
        assert!(!is_valid(&item, false).await);

        // Right size, wrong content: the quick check passes, the deep check doesn't.
        tokio::fs::write(&path, b"jello").await.unwrap();
        item.size = Some(5);
        assert!(is_valid(&item, false).await);
        assert!(!is_valid(&item, true).await);

        // No size known: fall back to hashing.
        item.size = None;
        assert!(!is_valid(&item, false).await);

        item.path = dir.path().join("missing");
        assert!(!is_valid(&item, false).await);
    }
}
