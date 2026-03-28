use crate::HelixError;
use deadpool_redis::{Connection, Pool};
use redis::AsyncCommands;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Serialize, Deserialize)]
struct StoredRecord {
    content_hash: String,
    fingerprint: u64,
    first_seen_at: u64,
    last_seen_at: u64,
    crawl_count: u32,
}

pub enum FreshnessResult {
    New,
    Unchanged {
        last_seen_at: u64,
    },
    Changed {
        old_hash: String,
        new_hash: String,
        last_seen_at: u64,
    },
}

pub struct FreshnessStore {
    pool: Pool,
}

impl FreshnessStore {
    #[must_use]
    pub fn new(pool: Pool) -> Self {
        Self { pool }
    }

    /// SHA-256 of content → hex string
    #[must_use]
    pub fn content_hash(content: &[u8]) -> String {
        let mut hasher = Sha256::new();
        hasher.update(content);
        hex::encode(hasher.finalize())
    }

    fn url_key(url: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(url.as_bytes());
        hex::encode(hasher.finalize())
    }

    /// Internal helper to make Redis `SET` type-safe
    async fn redis_set(conn: &mut Connection, key: &str, value: String) -> Result<(), HelixError> {
        let _: () = conn.set(key, value).await?;
        Ok(())
    }

    /// Checks freshness and updates stored record.
    ///
    /// # Errors
    /// Returns an error if:
    /// - Redis connection fails
    /// - Redis command fails
    /// - Stored data cannot be deserialized
    /// - System time retrieval fails
    pub async fn check_and_update(
        &self,
        url: &str,
        content: &[u8],
        fingerprint: u64,
    ) -> Result<FreshnessResult, HelixError> {
        let content_hash = Self::content_hash(content);
        let key = format!("helix:url:{}", Self::url_key(url));

        let mut conn = self.pool.get().await?;

        let stored: Option<String> = conn.get(&key).await?;

        let (result, first_seen_at, crawl_count) = if let Some(json) = stored {
            let record: StoredRecord = serde_json::from_str(&json)?;

            if record.content_hash == content_hash {
                (
                    FreshnessResult::Unchanged {
                        last_seen_at: record.last_seen_at,
                    },
                    record.first_seen_at,
                    record.crawl_count + 1,
                )
            } else {
                (
                    FreshnessResult::Changed {
                        old_hash: record.content_hash,
                        new_hash: content_hash.clone(),
                        last_seen_at: record.last_seen_at,
                    },
                    record.first_seen_at, // preserve original
                    record.crawl_count + 1,
                )
            }
        } else {
            (FreshnessResult::New, current_timestamp()?, 1)
        };

        // Only update Redis for New or Changed
        if matches!(
            result,
            FreshnessResult::New | FreshnessResult::Changed { .. }
        ) {
            let now = current_timestamp()?;

            let record = StoredRecord {
                content_hash,
                fingerprint,
                first_seen_at,
                last_seen_at: now,
                crawl_count,
            };

            let json = serde_json::to_string(&record)?;
            Self::redis_set(&mut conn, &key, json).await?;
        }

        Ok(result)
    }
    /// Pings Redis to verify connectivity.
    ///
    /// # Errors
    /// Returns an error if Redis connection or command fails.
    pub async fn ping(&self) -> Result<(), HelixError> {
        let mut conn = self.pool.get().await?;
        let _: String = conn.ping().await?;
        Ok(())
    }
}

/// Safe timestamp helper
fn current_timestamp() -> Result<u64, HelixError> {
    Ok(std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|_| HelixError::InvalidConfig("system time error".into()))?
        .as_secs())
}
