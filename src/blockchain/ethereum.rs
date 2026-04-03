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

        // 判断是否为热门链，限制扫描范围
        let is_popular_chain = matches!(self.chain_name.to_lowercase().as_str(), "ethereum" | "base");
        let max_scan_blocks = if is_popular_chain { 100 } else { 1000 };

        // 从最新的区块开始向前搜索，查找该地址的交易
        let mut current_block = latest_block;
        let start_block = if latest_block > max_scan_blocks.into() {
            latest_block - U64::from(max_scan_blocks as u64)
        } else {
            0u64.into()
        };

        tracing::debug!("[{}] Scanning last {} blocks for transaction of {}",
            self.chain_name, max_scan_blocks, address);

        while current_block >= start_block {
            let block = timeout(Duration::from_secs(10), self.provider.get_block_with_txs(current_block))
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

    /// 降级方案：使用 Ankr API 分页查询历史交易
    /// 通过分页查询追溯直到找到最后一笔交易（支持查询多年数据）
    /// 限制：最多只查询1年，减少API调用
    async fn get_last_transaction_fallback(&self, address: &str) -> Result<Option<TransactionInfo>> {
        tracing::info!("[{}] Trying Ankr API paginated fallback for {}", self.chain_name, address);

        // 使用分页查询追溯历史交易，最多查询1年（减少API调用）
        // 如果1年内没有交易，说明地址可能不活跃
        match self.enhanced_rpc.get_last_transaction_ankr_paginated(
            address,
            &self.multichain_url,
            &self.chain_name,
            1, // 限制为1年，减少API调用
        ).await {
            Ok(Some((timestamp, hash, block_number))) => {
                tracing::info!("[{}] Ankr API paginated fallback succeeded for {}: found tx at block {}",
                    self.chain_name, address, block_number);
                return Ok(Some(TransactionInfo {
                    timestamp,
                    hash,
                    block_number,
                }));
            }
            Ok(None) => {
                tracing::warn!("[{}] Ankr API paginated fallback returned no transactions for {} (address may have no recent transactions)",
                    self.chain_name, address);
            }
            Err(e) => {
                tracing::warn!("[{}] Ankr API paginated fallback failed for {}: {}",
                    self.chain_name, address, e);
            }
        }

        // 不再扫描区块，因为区块扫描会发起大量API请求
        // 如果分页查询失败，直接返回None
        tracing::info!("[{}] Skipping block scanning for {} to reduce API calls", self.chain_name, address);
        Ok(None)
    }

    /// 通过扫描区块获取最后一笔交易（仅用于极端情况）
    #[allow(dead_code)]
    async fn get_last_transaction_by_scanning(&self, address: &str) -> Result<Option<TransactionInfo>> {
        let eth_address = Address::from_str(address)
            .map_err(|e| BlockchainError::InvalidAddress(format!("Invalid address: {}", e)))?;

        // 获取最新区块号
        let latest_block = timeout(
            Duration::from_secs(10),
            self.provider.get_block_number()
        ).await
            .map_err(|_| BlockchainError::TimeoutError("Timeout getting block number".to_string()))?
            .map_err(|e| BlockchainError::RpcConnection(format!("Failed to get block number: {}", e)))?;

        // 根据链类型设置不同的扫描范围
        // 热门链区块产生快，扫描更多区块；其他链扫描适中范围
        let scan_blocks = match self.chain_name.to_lowercase().as_str() {
            "ethereum" => 5000,   // 约17小时（12秒/区块）
            "base" => 5000,       // 约2小时（2秒/区块）
            "polygon" => 10000,   // 约5小时（2秒/区块）
            "bsc" | "bnb" => 5000, // 约4小时（3秒/区块）
            "arbitrum" => 50000,  // 约14小时（1秒/区块）
            "optimism" => 50000,  // 约14小时（1秒/区块）
            "avalanche" => 5000,  // 约7小时（5秒/区块）
            _ => 2000,            // 其他链默认扫描2000个区块
        };

        let start_block = latest_block.as_u64().saturating_sub(scan_blocks);

        tracing::info!("[{}] Block scanning {} blocks ({} to {}) for {}",
            self.chain_name, scan_blocks, start_block, latest_block, address);

        for block_num in (start_block..=latest_block.as_u64()).rev() {
            match timeout(
                Duration::from_secs(5),
                self.provider.get_block_with_txs(block_num)
            ).await {
                Ok(Ok(Some(block))) => {
                    for tx in block.transactions.iter().rev() {
                        if tx.from == eth_address || tx.to == Some(eth_address) {
                            let timestamp = Utc.timestamp_opt(block.timestamp.as_u64() as i64, 0)
                                .single()
                                .ok_or_else(|| BlockchainError::RpcConnection("Invalid timestamp".to_string()))?;

                            tracing::info!("[{}] Found transaction in block {} for {}",
                                self.chain_name, block_num, address);

                            return Ok(Some(TransactionInfo {
                                timestamp,
                                hash: format!("{:x}", tx.hash),
                                block_number: block_num,
                            }));
                        }
                    }
                }
                Ok(Ok(None)) => continue,
                Ok(Err(e)) => {
                    tracing::debug!("[{}] Error getting block {}: {}", self.chain_name, block_num, e);
                    continue;
                }
                Err(_) => {
                    tracing::debug!("[{}] Timeout getting block {}", self.chain_name, block_num);
                    continue;
                }
            }
        }

        tracing::warn!("[{}] No transaction found in last {} blocks for {} (tx_count > 0 but no tx found)",
            self.chain_name, scan_blocks, address);
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

        // 先查询交易数量（最轻量的请求）
        let tx_count = timeout(Duration::from_secs(30), self.provider.get_transaction_count(eth_address, None))
            .await
            .map_err(|_| BlockchainError::TimeoutError("Timeout getting transaction count".to_string()))?
            .map_err(|e| BlockchainError::RpcConnection(format!("Failed to get transaction count: {}", e)))?;

        tracing::debug!("[{}] {} - tx_count={}", self.chain_name, address_str, tx_count);

        // 如果没有交易，直接返回结果，跳过所有昂贵的API调用
        if tx_count.as_u64() == 0 {
            tracing::info!("[{}] {} - No transactions, skipping balance and tx query", self.chain_name, address_str);
            return Ok(AddressInfo {
                address: address_str,
                balance: Some("0".to_string()),
                transaction_count: 0,
                last_transaction: None,
            });
        }

        // 有交易时才查询余额和最后交易
        let balance_future = timeout(Duration::from_secs(30), self.provider.get_balance(eth_address, None));
        let last_tx_future = self.enhanced_rpc.get_last_transaction_ankr(
            &address_with_prefix,
            &self.multichain_url,
            &self.chain_name
        );

        tracing::debug!("[{}] {} - Waiting for balance, last_tx...", self.chain_name, address_str);

        let (balance_result, last_tx_result) = tokio::join!(
            balance_future,
            last_tx_future
        );

        tracing::debug!("[{}] {} - Balance and tx requests completed", self.chain_name, address_str);

        let balance = balance_result
            .map_err(|_| BlockchainError::TimeoutError("Timeout getting balance".to_string()))?
            .map_err(|e| BlockchainError::RpcConnection(format!("Failed to get balance: {}", e)))?;

        tracing::debug!("[{}] {} - balance={}", self.chain_name, address_str, balance);

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
                tracing::debug!("Ankr API returned no transactions for {} on {}", address_str, self.chain_name);

                if self.chain_name.to_lowercase() == "zksync" {
                    None
                } else {
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
                }
            }
            Err(e) => {
                tracing::warn!("Failed to get last transaction for {} on {}: {}",
                    address_str, self.chain_name, e);

                if self.chain_name.to_lowercase() == "zksync" {
                    None
                } else {
                    match self.get_last_transaction_fallback(&address_with_prefix).await {
                        Ok(Some(tx_info)) => {
                            tracing::info!("Fallback succeeded after Ankr error for {} on {}", address_str, self.chain_name);
                            Some(tx_info)
                        }
                        _ => None
                    }
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
