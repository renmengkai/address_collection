use crate::blockchain::{BlockchainClient, QueryResult, QueryStatus};
use crate::error::{BlockchainError, Result};
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Semaphore;
use tokio::time::{sleep, Duration};

pub struct QueryEngine {
    clients: HashMap<String, Arc<dyn BlockchainClient>>,
    rate_limiter: Arc<Semaphore>,
    retry_attempts: u32,
    retry_delay: Duration,
    concurrent_per_chain: usize,
}

impl QueryEngine {
    pub fn new(max_concurrent: usize, retry_attempts: u32, retry_delay_ms: u64) -> Self {
        Self {
            clients: HashMap::new(),
            rate_limiter: Arc::new(Semaphore::new(max_concurrent)),
            retry_attempts,
            retry_delay: Duration::from_millis(retry_delay_ms),
            concurrent_per_chain: 50, // 提高并发数：平衡速度和稳定性
        }
    }

    pub fn with_concurrent_per_chain(mut self, concurrent_per_chain: usize) -> Self {
        self.concurrent_per_chain = concurrent_per_chain;
        self
    }

    pub fn add_client(&mut self, chain: String, client: Arc<dyn BlockchainClient>) {
        self.clients.insert(chain, client);
    }

    pub async fn query_addresses(
        &self,
        addresses: &[String],
        chain: &str,
    ) -> Result<Vec<QueryResult>> {
        self.query_addresses_with_concurrency(addresses, chain, self.concurrent_per_chain).await
    }

    pub async fn query_addresses_with_concurrency(
        &self,
        addresses: &[String],
        chain: &str,
        concurrent: usize,
    ) -> Result<Vec<QueryResult>> {
        let client = self
            .clients
            .get(chain)
            .ok_or_else(|| BlockchainError::ConfigError(format!("Chain {} not supported", chain)))?;

        let progress = ProgressBar::new(addresses.len() as u64);
        progress.set_style(
            ProgressStyle::default_bar()
                .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} ({eta})")
                .unwrap()
                .progress_chars("#>-"),
        );
        progress.set_message(format!("[{}] {} addresses ({}线程)", chain, addresses.len(), concurrent));

        // 为每条链创建独立的并发控制
        let chain_limiter = Arc::new(Semaphore::new(concurrent));
        let mut results = Vec::new();
        let mut tasks = Vec::new();

        for address in addresses {
            let client = Arc::clone(client);
            let address = address.clone();
            let chain_limiter = Arc::clone(&chain_limiter);
            let retry_attempts = self.retry_attempts;
            let retry_delay = self.retry_delay;

            let task = tokio::spawn(async move {
                let _permit = chain_limiter.acquire().await.unwrap();
                Self::query_address_with_retry(
                    client,
                    address,
                    retry_attempts,
                    retry_delay,
                )
                .await
            });

            tasks.push(task);
        }

        for task in tasks {
            match task.await {
                Ok(result) => {
                    results.push(result);
                    progress.inc(1);
                }
                Err(e) => {
                    results.push(QueryResult {
                        address: "Unknown".to_string(),
                        chain_name: chain.to_string(),
                        chain_display_name: chain.to_string(),
                        balance: None,
                        transaction_count: 0,
                        last_transaction_time: None,
                        last_transaction_hash: None,
                        status: QueryStatus::Error,
                        error_message: Some(format!("Task join error: {}", e)),
                    });
                    progress.inc(1);
                }
            }
        }

        progress.finish_with_message(format!("[{}] 完成 {} 个地址", chain, addresses.len()));
        Ok(results)
    }

    async fn query_address_with_retry(
        client: Arc<dyn BlockchainClient>,
        address: String,
        max_attempts: u32,
        retry_delay: Duration,
    ) -> QueryResult {
        let mut attempts = 0;
        
        loop {
            attempts += 1;
            
            match client.get_address_info(&address).await {
                Ok(info) => {
                    return QueryResult {
                        address: address.clone(),
                        chain_name: client.get_chain_name().to_string(),
                        chain_display_name: client.get_display_name().to_string(),
                        balance: info.balance,
                        transaction_count: info.transaction_count,
                        last_transaction_time: info.last_transaction.as_ref().map(|t| t.timestamp),
                        last_transaction_hash: info.last_transaction.as_ref().map(|t| t.hash.clone()),
                        status: QueryStatus::Success,
                        error_message: None,
                    };
                }
                Err(e) => {
                    if attempts >= max_attempts {
                        return QueryResult {
                            address: address.clone(),
                            chain_name: client.get_chain_name().to_string(),
                            chain_display_name: client.get_display_name().to_string(),
                            balance: None,
                            transaction_count: 0,
                            last_transaction_time: None,
                            last_transaction_hash: None,
                            status: QueryStatus::Error,
                            error_message: Some(format!("Failed after {} attempts: {}", attempts, e)),
                        };
                    }
                    
                    sleep(retry_delay * attempts as u32).await;
                }
            }
        }
    }

    pub async fn query_multiple_chains(
        &self,
        addresses: &[String],
        chains: &[String],
    ) -> Result<HashMap<String, Vec<QueryResult>>> {
        println!("\n开始并发查询 {} 条链，每条链 {} 个并发线程\n", chains.len(), self.concurrent_per_chain);
        
        // 创建多进度条管理器
        let multi_progress = Arc::new(MultiProgress::new());
        
        let addresses = Arc::new(addresses.to_vec());
        let mut chain_tasks = Vec::new();
        
        // 并发查询所有链
        for chain in chains {
            let chain = chain.clone();
            let addresses = Arc::clone(&addresses);
            let client = match self.clients.get(&chain) {
                Some(c) => Arc::clone(c),
                None => {
                    eprintln!("链 {} 未配置，跳过", chain);
                    continue;
                }
            };
            
            let concurrent = self.concurrent_per_chain;
            let retry_attempts = self.retry_attempts;
            let retry_delay = self.retry_delay;
            let multi_progress = Arc::clone(&multi_progress);
            
            let task = tokio::spawn(async move {
                let result = Self::query_chain_internal(
                    client,
                    &addresses,
                    &chain,
                    concurrent,
                    retry_attempts,
                    retry_delay,
                    &multi_progress,
                ).await;
                (chain, result)
            });
            
            chain_tasks.push(task);
        }
        
        // 收集所有链的结果
        let mut all_results = HashMap::new();
        for task in chain_tasks {
            match task.await {
                Ok((chain, Ok(results))) => {
                    all_results.insert(chain, results);
                }
                Ok((chain, Err(e))) => {
                    eprintln!("查询链 {} 失败: {}", chain, e);
                    all_results.insert(chain, Vec::new());
                }
                Err(e) => {
                    eprintln!("任务执行失败: {}", e);
                }
            }
        }
        
        Ok(all_results)
    }
    
    async fn query_chain_internal(
        client: Arc<dyn BlockchainClient>,
        addresses: &[String],
        chain: &str,
        concurrent: usize,
        retry_attempts: u32,
        retry_delay: Duration,
        multi_progress: &MultiProgress,
    ) -> Result<Vec<QueryResult>> {
        let progress = multi_progress.add(ProgressBar::new(addresses.len() as u64));
        progress.set_style(
            ProgressStyle::default_bar()
                .template("[{msg}] [{bar:40.cyan/blue}] {pos}/{len} ({eta})")
                .unwrap()
                .progress_chars("#>-"),
        );
        progress.set_message(format!("{:<10}", chain));

        // 为每条链创建独立的并发控制
        let chain_limiter = Arc::new(Semaphore::new(concurrent));
        let mut tasks = Vec::new();

        for (idx, address) in addresses.iter().enumerate() {
            let client = Arc::clone(&client);
            let address = address.clone();
            let chain_limiter = Arc::clone(&chain_limiter);

            let task = tokio::spawn(async move {
                let _permit = chain_limiter.acquire().await.unwrap();
                Self::query_address_with_retry(
                    client,
                    address,
                    retry_attempts,
                    retry_delay,
                )
                .await
            });

            tasks.push(task);
        }

        let mut results = Vec::new();
        for task in tasks {
            match task.await {
                Ok(result) => {
                    results.push(result);
                    progress.inc(1);
                }
                Err(e) => {
                    results.push(QueryResult {
                        address: "Unknown".to_string(),
                        chain_name: chain.to_string(),
                        chain_display_name: chain.to_string(),
                        balance: None,
                        transaction_count: 0,
                        last_transaction_time: None,
                        last_transaction_hash: None,
                        status: QueryStatus::Error,
                        error_message: Some(format!("Task join error: {}", e)),
                    });
                    progress.inc(1);
                }
            }
        }

        progress.finish_with_message(format!("{:<10} ✓", chain));
        Ok(results)
    }

    pub fn supported_chains(&self) -> Vec<String> {
        self.clients.keys().cloned().collect()
    }
}