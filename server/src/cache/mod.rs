mod memory;
mod persistent;

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use dashmap::DashMap;

use crate::common::error::AppError;
use crate::common::types::SizeResult;
use self::memory::MemoryCache;
use self::persistent::PersistentCache;

pub struct CacheService {
    l1: MemoryCache,
    l2: PersistentCache,
    /// Failed analyses cached for 1 hour to avoid re-enqueuing known-broken packages.
    negative: moka::future::Cache<String, String>,
    /// Per-package request counts, flushed to Postgres every 5 minutes.
    /// Note: counts swapped to zero at flush time; requests arriving during
    /// the swap are counted in the next flush window (acceptable for metrics).
    request_counts: Arc<DashMap<(String, String), AtomicU64>>,
    pool: sqlx::PgPool,
}

impl CacheService {
    pub fn new(pool: sqlx::PgPool) -> Self {
        Self {
            l1: MemoryCache::new(100_000),
            l2: PersistentCache::new(pool.clone()),
            negative: moka::future::Cache::builder()
                .max_capacity(10_000)
                .time_to_live(Duration::from_secs(3600))
                .build(),
            request_counts: Arc::new(DashMap::new()),
            pool,
        }
    }

    /// L1 → L2 lookup with L1 promotion on L2 hit.
    pub async fn get(
        &self,
        ecosystem: &str,
        package: &str,
        version: &str,
    ) -> Result<Option<SizeResult>, AppError> {
        let key = cache_key(ecosystem, package, version);

        if let Some(result) = self.l1.get(&key).await {
            return Ok(Some(result));
        }

        if let Some(result) = self.l2.get(ecosystem, package, version).await? {
            self.l1.insert(&key, result.clone()).await;
            return Ok(Some(result));
        }

        Ok(None)
    }

    pub async fn get_latest(
        &self,
        ecosystem: &str,
        package: &str,
    ) -> Result<Option<SizeResult>, AppError> {
        self.l2.get_latest(ecosystem, package).await
    }

    pub async fn search(
        &self,
        ecosystem: &str,
        query: Option<&str>,
        keyword: Option<&str>,
        treeshakeable: Option<bool>,
        has_types: Option<bool>,
        sort: &str,
        order: &str,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<SizeResult>, AppError> {
        self.l2.search(ecosystem, query, keyword, treeshakeable, has_types, sort, order, limit, offset).await
    }

    pub async fn top(
        &self,
        ecosystem: &str,
        sort: &str,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<SizeResult>, AppError> {
        self.l2.top(ecosystem, sort, limit, offset).await
    }

    pub async fn alternatives(
        &self,
        ecosystem: &str,
        package: &str,
        limit: i64,
    ) -> Result<Vec<SizeResult>, AppError> {
        self.l2.alternatives(ecosystem, package, limit).await
    }

    pub async fn get_failure(&self, ecosystem: &str, package: &str, version: &str) -> Option<String> {
        self.negative.get(&cache_key(ecosystem, package, version)).await
    }

    pub async fn insert_failure(&self, ecosystem: &str, package: &str, version: &str, error: String) {
        self.negative.insert(cache_key(ecosystem, package, version), error).await;
    }

    /// Enqueue a hot-priority job. Idempotent — no-op if already pending or processing.
    pub async fn enqueue_job(
        &self,
        ecosystem: &str,
        package: &str,
        version: &str,
    ) -> Result<(), AppError> {
        sqlx::query(
            r#"
            INSERT INTO job_queue (ecosystem, name, version, priority)
            VALUES ($1, $2, $3, 'hot')
            ON CONFLICT ON CONSTRAINT uq_job_active DO NOTHING
            "#,
        )
        .bind(ecosystem)
        .bind(package)
        .bind(version)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Reset jobs stuck in 'processing' for >5 minutes back to 'pending',
    /// unless they have exhausted retries.
    pub async fn reset_stale_jobs(&self) {
        let result = sqlx::query(
            r#"
            UPDATE job_queue
            SET status = 'pending', started_at = NULL, worker_id = NULL
            WHERE status = 'processing'
              AND started_at < now() - INTERVAL '5 minutes'
              AND retry_count < max_retries
            "#,
        )
        .execute(&self.pool)
        .await;

        match result {
            Ok(r) if r.rows_affected() > 0 => {
                tracing::warn!(count = r.rows_affected(), "reset stale jobs");
            }
            Err(e) => tracing::warn!(error = %e, "failed to reset stale jobs"),
            _ => {}
        }
    }

    pub fn track_request(&self, ecosystem: &str, package: &str) {
        self.request_counts
            .entry((ecosystem.to_string(), package.to_string()))
            .or_insert_with(|| AtomicU64::new(0))
            .fetch_add(1, Ordering::Relaxed);
    }

    pub fn stats(&self) -> CacheStats {
        CacheStats { l1_entries: self.l1.entry_count() }
    }

    pub async fn flush_request_stats(&self) {
        let entries: Vec<((String, String), u64)> = self
            .request_counts
            .iter()
            .map(|e| (e.key().clone(), e.value().swap(0, Ordering::Relaxed)))
            .filter(|(_, count)| *count > 0)
            .collect();

        for ((ecosystem, name), count) in entries {
            let result = sqlx::query(
                r#"
                INSERT INTO npm_request_stats (name, requests, last_hit)
                VALUES ($1, $2, now())
                ON CONFLICT (name)
                DO UPDATE SET requests = npm_request_stats.requests + $2, last_hit = now()
                "#,
            )
            .bind(&name)
            .bind(count as i64)
            .execute(&self.pool)
            .await;

            if let Err(e) = result {
                tracing::warn!(error = %e, ecosystem = %ecosystem, name = %name, "failed to flush request stats");
            }
        }
    }
}

pub struct CacheStats {
    pub l1_entries: u64,
}

fn cache_key(ecosystem: &str, package: &str, version: &str) -> String {
    format!("{ecosystem}:{package}:{version}")
}
