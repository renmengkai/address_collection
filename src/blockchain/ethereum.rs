use async_trait::async_trait;
use ethers::prelude::*;
use ethers::providers::{Http, Provider};
use chrono::{TimeZone, Utc};
use url::Url;
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::timeout;
use crate::error::{BlockchainError, Result};
use super::traits::{BlockchainClient, AddressInfo, TransactionInfo};
use super::enhanced_rpc::EnhancedRpcClient;

pub struct EthereumClient {
    provider: Arc<Provider<Http>>,
    chain_name: String,
    display_name: String,
    #[allow(dead_code)]
    chain_id: u64,
    multichain_url: String,
    enhanced_rpc: Arc<EnhancedRpcClient>,
}

impl EthereumClient {
    pub fn new(rpc_url: String, chain_name: String, display_name: String, chain_id: u64, multichain_url: String) -> Result<Self> {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .map_err(|e| BlockchainError::RpcConnection(format!("Failed to create HTTP client: {}", e)))?;

        let rpc_url_parsed: Url = rpc_url.parse().map_err(|e: url::ParseError| BlockchainError::UrlError(e))?;
        let http = Http::new_with_client(rpc_url_parsed, client);
        let provider = Provider::new(http);

        Ok(Self {
            provider: Arc::new(provider),
            chain_name,
            display_name,
            chain_id,
            multichain_url,
            enhanced_rpc: Arc::new(EnhancedRpcClient::new()),
        })
    }

    async fn get_last_transaction_with_retry(&self, address: &str, max_retries: u32) -> Result<Option<TransactionInfo>> {
        let mut retries = 0;
        let mut last_error = None;

        while retries < max_retries {
            match self.get_last_transaction_internal(address).await {
                Ok(result) => return Ok(result),
                Err(e) => {
                    last_error = Some(e);
                    retries += 1;
                    if retries < max_retries {
                        let delay = Duration::from_secs(2u64.pow(retries - 1));
                        tracing::warn!("Retry {} after {:?} due to error: {}", retries, delay, last_error.as_ref().unwrap());
                        tokio::time::sleep(delay).await;
                    }
                }
            }
        }

        Err(last_error.unwrap_or_else(|| BlockchainError::RpcConnection("Max retries exceeded".to_string())))
    }

    async fn get_last_transaction_internal(&self, address: &str) -> Result<Option<TransactionInfo>> {
        let address = Address::from_str(address)
            .map_err(|e| BlockchainError::InvalidAddress(format!("Invalid Ethereum address: {}", e)))?;

        // 获取地址的交易数量
        let tx_count = timeout(Duration::from_secs(30), self.provider.get_transaction_count(address, None))
            .await
            .map_err(|_| BlockchainError::TimeoutError("Timeout getting transaction count".to_string()))?
            .map_err(|e| BlockchainError::RpcConnection(format!("Failed to get transaction count: {}", e)))?;

        if tx_count == 0u64.into() {
            return Ok(None);
        }

        // 获取最新的区块号
        let latest_block = timeout(Duration::from_secs(30), self.provider.get_block_number())
            .await
            .map_err(|_| BlockchainError::TimeoutError("Timeout getting latest block".to_string()))?
            .map_err(|e| BlockchainError::RpcConnection(format!("Failed to get latest block: {}", e)))?;

        // 从最新的区块开始向前搜索，查找该地址的交易
        let mut current_block = latest_block;
        let start_block = if latest_block > 1000u64.into() {
            latest_block - U64::from(1000u64)
        } else {
            0u64.into()
        };

        while current_block >= start_block {
            let block = timeout(Duration::from_secs(30), self.provider.get_block_with_txs(current_block))
                .await
                .map_err(|_| BlockchainError::TimeoutError("Timeout getting block".to_string()))?
                .map_err(|e| BlockchainError::RpcConnection(format!("Failed to get block: {}", e)))?;

            if let Some(block) = block {
                // 查找包含该地址的交易
                for tx in block.transactions {
                    if tx.from == address || tx.to == Some(address) {
                        let timestamp = Utc.timestamp_opt(block.timestamp.as_u64() as i64, 0)
                            .single()
                            .ok_or_else(|| BlockchainError::RpcConnection("Invalid timestamp".to_string()))?;

                        return Ok(Some(TransactionInfo {
                            hash: format!("{:x}", tx.hash),
                            timestamp,
                            block_number: block.number.unwrap_or_default().as_u64(),
                        }));
                    }
                }
            }

            if current_block == 0u64.into() {
                break;
            }
            current_block = current_block - U64::from(1u64);
        }

        Ok(None)
    }

    /// 降级方案：使用标准 RPC 获取最后一笔交易
    async fn get_last_transaction_fallback(&self, address: &str) -> Result<Option<TransactionInfo>> {
        let eth_address = Address::from_str(address)
            .map_err(|e| BlockchainError::InvalidAddress(format!("Invalid address: {}", e)))?;

        // 从最新区块向回查找最多 50 个区块（减少查询量）
        let latest_block = self.provider.get_block_number().await
            .map_err(|e| BlockchainError::RpcConnection(format!("Failed to get block number: {}", e)))?;

        let start_block = latest_block.as_u64().saturating_sub(50);

        // 查找最近的交易（从最新到最旧）
        for block_num in (start_block..=latest_block.as_u64()).rev() {
            if let Ok(Some(block)) = self.provider.get_block_with_txs(block_num).await {
                // 检查该区块中是否有该地址的交易
                for tx in block.transactions.iter().rev() {
                    if tx.from == eth_address || tx.to == Some(eth_address) {
                        // 找到最后一笔交易
                        let timestamp = Utc.timestamp_opt(block.timestamp.as_u64() as i64, 0)
                            .single()
                            .ok_or_else(|| BlockchainError::RpcConnection("Invalid timestamp".to_string()))?;

                        return Ok(Some(TransactionInfo {
                            timestamp,
                            hash: format!("{:x}", tx.hash),
                            block_number: block_num,
                        }));
                    }
                }
            }
        }

        // 在最近 50 个区块中没找到
        Ok(None)
    }
}

#[async_trait]
impl BlockchainClient for EthereumClient {
    async fn get_last_transaction(&self, address: &str) -> Result<Option<TransactionInfo>> {
        self.get_last_transaction_with_retry(address, 3).await
    }

    async fn get_address_info(&self, address: &str) -> Result<AddressInfo> {
        let eth_address = Address::from_str(address)
            .map_err(|e| BlockchainError::InvalidAddress(format!("Invalid Ethereum address: {}", e)))?;

        let address_str = format!("{:x}", eth_address);
        let address_with_prefix = format!("0x{}", address_str);

        tracing::debug!("[{}] Querying address {}", self.chain_name, address_str);

        // 并发执行：余额、交易数量、最后交易时间（3个请求同时进行）
        let balance_future = timeout(Duration::from_secs(30), self.provider.get_balance(eth_address, None));
        let tx_count_future = timeout(Duration::from_secs(30), self.provider.get_transaction_count(eth_address, None));
        let last_tx_future = self.enhanced_rpc.get_last_transaction_ankr(
            &address_with_prefix,
            &self.multichain_url,
            &self.chain_name
        );

        tracing::debug!("[{}] {} - Waiting for balance, tx_count, last_tx...", self.chain_name, address_str);

        let (balance_result, tx_count_result, last_tx_result) = tokio::join!(
            balance_future,
            tx_count_future,
            last_tx_future
        );

        tracing::debug!("[{}] {} - All 3 requests completed", self.chain_name, address_str);

        let balance = balance_result
            .map_err(|_| BlockchainError::TimeoutError("Timeout getting balance".to_string()))?
            .map_err(|e| BlockchainError::RpcConnection(format!("Failed to get balance: {}", e)))?;

        let tx_count = tx_count_result
            .map_err(|_| BlockchainError::TimeoutError("Timeout getting transaction count".to_string()))?
            .map_err(|e| BlockchainError::RpcConnection(format!("Failed to get transaction count: {}", e)))?;

        tracing::debug!("[{}] {} - balance={}, tx_count={}", self.chain_name, address_str, balance, tx_count);

        // 处理交易时间查询结果
        let last_transaction = match last_tx_result {
            Ok(Some((timestamp, hash, block_number))) => {
                tracing::info!("Got transaction for {} on {}: hash={}, timestamp={}",
                    address_str, self.chain_name, hash, timestamp);
                Some(TransactionInfo {
                    timestamp,
                    hash,
                    block_number,
                })
            }
            Ok(None) => {
                // Ankr API 返回空，zksync 直接返回 None，其他链尝试回退
                tracing::debug!("Ankr API returned no transactions for {} on {}", address_str, self.chain_name);

                // 如果是 zksync，不尝试回退（回退方案对 zksync 很慢且不可靠）
                if self.chain_name.to_lowercase() == "zksync" {
                    None
                } else if tx_count.as_u64() > 0 {
                    match self.get_last_transaction_fallback(&address_with_prefix).await {
                        Ok(Some(tx_info)) => {
                            tracing::info!("Fallback succeeded for {} on {}", address_str, self.chain_name);
                            Some(tx_info)
                        }
                        Ok(None) => {
                            tracing::warn!("Fallback returned no transaction for {} on {} (tx_count={})",
                                address_str, self.chain_name, tx_count.as_u64());
                            None
                        }
                        Err(e) => {
                            tracing::warn!("Fallback failed for {} on {}: {}", address_str, self.chain_name, e);
                            None
                        }
                    }
                } else {
                    tracing::info!("No transactions found for {} on {}", address_str, self.chain_name);
                    None
                }
            }
            Err(e) => {
                tracing::warn!("Failed to get last transaction for {} on {}: {}",
                    address_str, self.chain_name, e);

                // 如果是 zksync，直接返回 None（Ankr 不稳定，回退太慢）
                if self.chain_name.to_lowercase() == "zksync" {
                    None
                } else if tx_count.as_u64() > 0 {
                    match self.get_last_transaction_fallback(&address_with_prefix).await {
                        Ok(Some(tx_info)) => {
                            tracing::info!("Fallback succeeded after Ankr error for {} on {}", address_str, self.chain_name);
                            Some(tx_info)
                        }
                        _ => None
                    }
                } else {
                    None
                }
            }
        };

        // 将 Wei 转换为 ETH (1 ETH = 10^18 Wei)
        let balance_eth = balance.as_u128() as f64 / 1_000_000_000_000_000_000.0;

        Ok(AddressInfo {
            address: address_str,
            balance: Some(format!("{:.18}", balance_eth)),
            transaction_count: tx_count.as_u64(),
            last_transaction,
        })
    }

    async fn validate_address(&self, address: &str) -> Result<bool> {
        match Address::from_str(address) {
            Ok(_) => Ok(true),
            Err(_) => Ok(false),
        }
    }

    fn get_chain_name(&self) -> &str {
        &self.chain_name
    }

    fn get_display_name(&self) -> &str {
        &self.display_name
    }

    fn get_chain_id(&self) -> u64 {
        self.chain_id
    }
}
