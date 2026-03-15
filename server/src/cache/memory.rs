use moka::future::Cache;

use crate::common::types::SizeResult;

#[derive(Clone)]
pub struct MemoryCache {
    cache: Cache<String, SizeResult>,
}

impl MemoryCache {
    pub fn new(max_capacity: u64) -> Self {
        Self {
            cache: Cache::builder().max_capacity(max_capacity).build(),
        }
    }

    pub async fn get(&self, key: &str) -> Option<SizeResult> {
        self.cache.get(key).await
    }

    pub async fn insert(&self, key: &str, value: SizeResult) {
        self.cache.insert(key.to_string(), value).await;
    }

    pub fn entry_count(&self) -> u64 {
        self.cache.entry_count()
    }
}
