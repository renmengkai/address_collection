use crate::blockchain::{AddressInfo, QueryResult};
use moka::future::Cache;
use std::sync::Arc;
use std::time::Duration;

/// 地址查询结果缓存（预留，用于未来扩展）
#[allow(dead_code)]
pub struct AddressCache {
    /// 缓存: (链名, 小写地址) -> AddressInfo
    inner: Cache<(String, String), AddressInfo>,
}

impl AddressCache {
    /// 创建新的缓存实例
    /// max_capacity: 最大缓存条目数
    /// ttl_secs: 缓存过期时间（秒）
    pub fn new(max_capacity: u64, ttl_secs: u64) -> Self {
        let cache = Cache::builder()
            .max_capacity(max_capacity)
            .time_to_live(Duration::from_secs(ttl_secs))
            .build();

        Self { inner: cache }
    }

    /// 获取缓存的地址信息
    pub async fn get(&self, chain: &str, address: &str) -> Option<AddressInfo> {
        let key = (chain.to_lowercase(), address.to_lowercase());
        self.inner.get(&key).await
    }

    /// 设置缓存的地址信息
    pub async fn set(&self, chain: &str, address: &str, info: AddressInfo) {
        let key = (chain.to_lowercase(), address.to_lowercase());
        self.inner.insert(key, info).await;
    }

    /// 批量获取缓存的地址信息
    /// 返回 (已缓存的地址列表, 未缓存的地址列表)
    pub async fn get_batch(
        &self,
        chain: &str,
        addresses: &[String],
    ) -> (Vec<(String, AddressInfo)>, Vec<String>) {
        let mut cached = Vec::new();
        let mut uncached = Vec::new();

        for address in addresses {
            if let Some(info) = self.get(chain, address).await {
                cached.push((address.clone(), info));
            } else {
                uncached.push(address.clone());
            }
        }

        (cached, uncached)
    }

    /// 获取缓存统计信息
    pub fn get_stats(&self) -> CacheStats {
        CacheStats {
            entry_count: self.inner.entry_count(),
        }
    }
}

/// 缓存统计信息（预留，用于未来扩展）
#[allow(dead_code)]
pub struct CacheStats {
    pub entry_count: u64,
}

/// 创建默认配置的缓存（预留，用于未来扩展）
/// 最大10000条，缓存1小时
#[allow(dead_code)]
pub fn create_default_cache() -> Arc<AddressCache> {
    Arc::new(AddressCache::new(10_000, 3600))
}

/// 创建查询结果缓存（用于去重输入地址）
pub struct QueryResultCache {
    /// 缓存: (链名, 小写地址) -> QueryResult
    inner: Cache<(String, String), QueryResult>,
}

impl QueryResultCache {
    pub fn new(max_capacity: u64) -> Self {
        let cache = Cache::builder()
            .max_capacity(max_capacity)
            .build();

        Self { inner: cache }
    }

    pub async fn get(&self, chain: &str, address: &str) -> Option<QueryResult> {
        let key = (chain.to_lowercase(), address.to_lowercase());
        self.inner.get(&key).await
    }

    pub async fn set(&self, chain: &str, address: &str, result: QueryResult) {
        let key = (chain.to_lowercase(), address.to_lowercase());
        self.inner.insert(key, result).await;
    }
}

/// 创建用于查询结果去重的缓存
pub fn create_dedup_cache() -> Arc<QueryResultCache> {
    Arc::new(QueryResultCache::new(100_000))
}
