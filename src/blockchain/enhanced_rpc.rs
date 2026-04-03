use crate::error::{BlockchainError, Result};
use chrono::{DateTime, Utc, TimeZone};
use serde::Deserialize;
use serde_json::json;
use std::time::Duration;
use tokio::time::timeout;

#[derive(Debug, Deserialize)]
struct AnkrTransaction {
    #[serde(rename = "blockNumber")]
    block_number: String,
    timestamp: String,
    hash: String,
}

#[derive(Debug, Deserialize)]
struct AnkrResult {
    transactions: Vec<AnkrTransaction>,
    #[serde(rename = "nextPageToken")]
    #[allow(dead_code)]
    next_page_token: Option<String>,
}

#[derive(Debug, Deserialize)]
struct AnkrResponse {
    result: Option<AnkrResult>,
}

pub struct EnhancedRpcClient {
    client: reqwest::Client,
}

impl EnhancedRpcClient {
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::builder()
                .timeout(Duration::from_secs(30))
                .pool_idle_timeout(Duration::from_secs(60))
                .pool_max_idle_per_host(50)  // 增加每个主机的最大空闲连接
                .tcp_keepalive(Duration::from_secs(30))
                .http2_keep_alive_interval(Duration::from_secs(15))
                .http2_keep_alive_timeout(Duration::from_secs(5))
                .http2_keep_alive_while_idle(true)
                .build()
                .unwrap(),
        }
    }

    /// 判断是否为热门链（更容易受速率限制影响）
    fn is_popular_chain(&self, chain_name: &str) -> bool {
        matches!(chain_name.to_lowercase().as_str(), "ethereum" | "base")
    }

    /// 获取链特定的超时时间（秒）
    fn get_timeout_for_chain(&self, chain_name: &str) -> u64 {
        if self.is_popular_chain(chain_name) {
            25 // 热门链给更长的超时时间
        } else {
            15
        }
    }

    /// 获取链特定的重试延迟（毫秒）
    fn get_retry_delay_for_chain(&self, chain_name: &str, attempt: u32) -> u64 {
        let base_delay = if self.is_popular_chain(chain_name) {
            500 // 热门链基础延迟更长，避免触发速率限制
        } else {
            200
        };
        base_delay * (2_u64.pow(attempt - 1))
    }

    /// 使用 Ankr 的增强型 RPC 方法获取最后交易
    pub async fn get_last_transaction_ankr(
        &self,
        address: &str,
        rpc_url: &str,
        chain_name: &str,
    ) -> Result<Option<(DateTime<Utc>, String, u64)>> {
        // 将链名转换为 Ankr 的 blockchain 参数
        let blockchain = match chain_name.to_lowercase().as_str() {
            "ethereum" => "eth",
            "polygon" => "polygon",
            "bsc" | "bnb" => "bsc",
            "arbitrum" => "arbitrum",
            "optimism" => "optimism",
            "avalanche" => "avalanche",
            "base" => "base",
            "linea" => "linea",
            "fantom" => "fantom",
            "gnosis" => "gnosis",
            "scroll" => "scroll",
            "zksync" => "zksync_era",
            _ => return Err(BlockchainError::ConfigError(format!("Unsupported chain: {}", chain_name))),
        };

        // 使用 ankr_getTransactionsByAddress 方法
        let payload = json!({
            "jsonrpc": "2.0",
            "method": "ankr_getTransactionsByAddress",
            "params": {
                "blockchain": blockchain,
                "address": address,
                "pageSize": 1,
                "descOrder": true
            },
            "id": 1
        });

        // 尝试最多 3 次，每次失败后指数退避
        let mut last_error = None;
        let timeout_secs = self.get_timeout_for_chain(chain_name);

        for attempt in 1..=3 {
            tracing::debug!("[{}] Attempt {} for address {}", chain_name, attempt, address);

            // 使用链特定的超时时间
            let request = self.client
                .post(rpc_url)
                .json(&payload);

            match timeout(Duration::from_secs(timeout_secs), request.send()).await {
                Ok(Ok(response)) => {
                    tracing::debug!("[{}] {} - Response received", chain_name, address);
                    match response.text().await {
                        Ok(response_text) => {
                            match serde_json::from_str::<AnkrResponse>(&response_text) {
                                Ok(ankr_response) => {
                                    tracing::debug!("Ankr API response for {}: has result={}", address, ankr_response.result.is_some());

                                    if let Some(result) = ankr_response.result {
                                        tracing::debug!("Transactions count: {}", result.transactions.len());

                                        if let Some(tx) = result.transactions.first() {
                                            // 解析时间戳（可能是十六进制字符串或十进制数字）
                                            let timestamp = if tx.timestamp.starts_with("0x") {
                                                u64::from_str_radix(tx.timestamp.trim_start_matches("0x"), 16)
                                                    .map_err(|e| BlockchainError::RpcConnection(format!("Invalid hex timestamp: {}", e)))?
                                            } else {
                                                tx.timestamp.parse::<u64>()
                                                    .map_err(|e| BlockchainError::RpcConnection(format!("Invalid timestamp: {}", e)))?
                                            };

                                            let datetime = Utc.timestamp_opt(timestamp as i64, 0)
                                                .single()
                                                .ok_or_else(|| BlockchainError::RpcConnection("Invalid timestamp".to_string()))?;

                                            let block_number = u64::from_str_radix(
                                                tx.block_number.trim_start_matches("0x"),
                                                16
                                            ).unwrap_or(0);

                                            return Ok(Some((datetime, tx.hash.clone(), block_number)));
                                        }
                                    }

                                    return Ok(None);
                                }
                                Err(e) => {
                                    last_error = Some(BlockchainError::RpcConnection(format!("Failed to parse Ankr response: {}", e)));
                                }
                            }
                        }
                        Err(e) => {
                            last_error = Some(BlockchainError::RpcConnection(format!("Failed to read response: {}", e)));
                        }
                    }
                }
                Ok(Err(e)) => {
                    // 连接错误
                    let msg = format!("Ankr RPC request failed: {}", e);
                    if attempt < 3 {
                        tracing::debug!("Attempt {} failed for {}: {}", attempt, address, e);
                    } else {
                        tracing::warn!("All attempts failed for {}: {}", address, e);
                    }
                    last_error = Some(BlockchainError::RpcConnection(msg));

                    if attempt < 3 {
                        let delay_ms = self.get_retry_delay_for_chain(chain_name, attempt);
                        tracing::debug!("Retrying after {} ms...", delay_ms);
                        tokio::time::sleep(Duration::from_millis(delay_ms)).await;
                    }
                }
                Err(_) => {
                    // 超时
                    let msg = format!("Ankr RPC timeout (attempt {}/3)", attempt);
                    tracing::warn!("[{}] {} for {}", chain_name, msg, address);
                    last_error = Some(BlockchainError::TimeoutError(msg));

                    if attempt < 3 {
                        let delay_ms = self.get_retry_delay_for_chain(chain_name, attempt);
                        tracing::debug!("Retrying after {} ms...", delay_ms);
                        tokio::time::sleep(Duration::from_millis(delay_ms)).await;
                    }
                }
            }
        }

        // 对于 zksync，如果 Ankr API 持续超时，直接返回 None 而非错误
        // zksync_era 在 Ankr 上可能不稳定
        if chain_name.to_lowercase() == "zksync" {
            tracing::warn!("Ankr API failed for zksync - returning no transaction data");
            return Ok(None);
        }

        // 所有尝试都失败，返回错误
        Err(last_error.unwrap_or_else(|| BlockchainError::RpcConnection("Unknown error".to_string())))
    }

    /// 使用 Ankr API 查询历史交易，可指定返回的最大交易数量
    /// 通过增加 pageSize 可以查询更久远的历史交易
    #[allow(dead_code)]
    pub async fn get_last_transaction_ankr_with_limit(
        &self,
        address: &str,
        rpc_url: &str,
        chain_name: &str,
        page_size: u32,
    ) -> Result<Option<(DateTime<Utc>, String, u64)>> {
        // 将链名转换为 Ankr 的 blockchain 参数
        let blockchain = match chain_name.to_lowercase().as_str() {
            "ethereum" => "eth",
            "polygon" => "polygon",
            "bsc" | "bnb" => "bsc",
            "arbitrum" => "arbitrum",
            "optimism" => "optimism",
            "avalanche" => "avalanche",
            "base" => "base",
            "linea" => "linea",
            "fantom" => "fantom",
            "gnosis" => "gnosis",
            "scroll" => "scroll",
            "zksync" => "zksync_era",
            _ => return Err(BlockchainError::ConfigError(format!("Unsupported chain: {}", chain_name))),
        };

        // 使用更大的 pageSize 查询更多历史交易
        let payload = json!({
            "jsonrpc": "2.0",
            "method": "ankr_getTransactionsByAddress",
            "params": {
                "blockchain": blockchain,
                "address": address,
                "pageSize": page_size,
                "descOrder": true
            },
            "id": 1
        });

        let timeout_secs = self.get_timeout_for_chain(chain_name);

        let request = self.client
            .post(rpc_url)
            .json(&payload);

        match timeout(Duration::from_secs(timeout_secs), request.send()).await {
            Ok(Ok(response)) => {
                match response.text().await {
                    Ok(response_text) => {
                        match serde_json::from_str::<AnkrResponse>(&response_text) {
                            Ok(ankr_response) => {
                                if let Some(result) = ankr_response.result {
                                    // 返回最后一笔交易（因为 descOrder=true，最后一笔就是最旧的）
                                    if let Some(tx) = result.transactions.last() {
                                        let timestamp = if tx.timestamp.starts_with("0x") {
                                            u64::from_str_radix(tx.timestamp.trim_start_matches("0x"), 16)
                                                .map_err(|e| BlockchainError::RpcConnection(format!("Invalid hex timestamp: {}", e)))?
                                        } else {
                                            tx.timestamp.parse::<u64>()
                                                .map_err(|e| BlockchainError::RpcConnection(format!("Invalid timestamp: {}", e)))?
                                        };

                                        let datetime = Utc.timestamp_opt(timestamp as i64, 0)
                                            .single()
                                            .ok_or_else(|| BlockchainError::RpcConnection("Invalid timestamp".to_string()))?;

                                        let block_number = u64::from_str_radix(
                                            tx.block_number.trim_start_matches("0x"),
                                            16
                                        ).unwrap_or(0);

                                        tracing::debug!("[{}] Found oldest transaction in {} records for {}: block={}",
                                            chain_name, result.transactions.len(), address, block_number);

                                        return Ok(Some((datetime, tx.hash.clone(), block_number)));
                                    }
                                }
                                Ok(None)
                            }
                            Err(e) => Err(BlockchainError::RpcConnection(format!("Failed to parse Ankr response: {}", e))),
                        }
                    }
                    Err(e) => Err(BlockchainError::RpcConnection(format!("Failed to read response: {}", e))),
                }
            }
            Ok(Err(e)) => Err(BlockchainError::RpcConnection(format!("Ankr RPC request failed: {}", e))),
            Err(_) => Err(BlockchainError::TimeoutError("Ankr RPC timeout".to_string())),
        }
    }

    /// 使用 Ankr API 查询历史交易，追溯直到找到最后一笔交易
    /// 适用于需要查询多年前历史交易的场景
    /// 策略：使用 fromTimestamp 参数，逐年向前查询，直到找到交易
    pub async fn get_last_transaction_ankr_paginated(
        &self,
        address: &str,
        rpc_url: &str,
        chain_name: &str,
        max_years: u32, // 最大查询年数
    ) -> Result<Option<(DateTime<Utc>, String, u64)>> {
        // 将链名转换为 Ankr 的 blockchain 参数
        let blockchain = match chain_name.to_lowercase().as_str() {
            "ethereum" => "eth",
            "polygon" => "polygon",
            "bsc" | "bnb" => "bsc",
            "arbitrum" => "arbitrum",
            "optimism" => "optimism",
            "avalanche" => "avalanche",
            "base" => "base",
            "linea" => "linea",
            "fantom" => "fantom",
            "gnosis" => "gnosis",
            "scroll" => "scroll",
            "zksync" => "zksync_era",
            _ => return Err(BlockchainError::ConfigError(format!("Unsupported chain: {}", chain_name))),
        };

        let timeout_secs = self.get_timeout_for_chain(chain_name);
        let now = Utc::now().timestamp() as u64;
        let one_year_secs: u64 = 365 * 24 * 60 * 60;

        // 从当前时间开始，逐年向前查询
        for year_offset in 0..max_years {
            let to_timestamp = now - (year_offset as u64 * one_year_secs);
            let from_timestamp = to_timestamp - one_year_secs;

            tracing::info!("[{}] Querying year {} for {} (from {} to {})",
                chain_name, year_offset + 1, address, from_timestamp, to_timestamp);

            let payload = json!({
                "jsonrpc": "2.0",
                "method": "ankr_getTransactionsByAddress",
                "params": {
                    "blockchain": blockchain,
                    "address": address,
                    "pageSize": 1,
                    "descOrder": true,
                    "fromTimestamp": from_timestamp,
                    "toTimestamp": to_timestamp
                },
                "id": year_offset + 1
            });

            let request = self.client
                .post(rpc_url)
                .json(&payload);

            match timeout(Duration::from_secs(timeout_secs), request.send()).await {
                Ok(Ok(response)) => {
                    match response.text().await {
                        Ok(response_text) => {
                            match serde_json::from_str::<AnkrResponse>(&response_text) {
                                Ok(ankr_response) => {
                                    if let Some(result) = ankr_response.result {
                                        if let Some(tx) = result.transactions.first() {
                                            let timestamp = if tx.timestamp.starts_with("0x") {
                                                u64::from_str_radix(tx.timestamp.trim_start_matches("0x"), 16)
                                                    .map_err(|e| BlockchainError::RpcConnection(format!("Invalid hex timestamp: {}", e)))?
                                            } else {
                                                tx.timestamp.parse::<u64>()
                                                    .map_err(|e| BlockchainError::RpcConnection(format!("Invalid timestamp: {}", e)))?
                                            };

                                            let datetime = Utc.timestamp_opt(timestamp as i64, 0)
                                                .single()
                                                .ok_or_else(|| BlockchainError::RpcConnection("Invalid timestamp".to_string()))?;

                                            let block_number = u64::from_str_radix(
                                                tx.block_number.trim_start_matches("0x"),
                                                16
                                            ).unwrap_or(0);

                                            tracing::info!("[{}] Found transaction in year {} for {}: block={}, time={}",
                                                chain_name, year_offset + 1, address, block_number, datetime);

                                            return Ok(Some((datetime, tx.hash.clone(), block_number)));
                                        }
                                    }
                                    tracing::debug!("[{}] No transactions in year {} for {}",
                                        chain_name, year_offset + 1, address);
                                }
                                Err(e) => {
                                    tracing::warn!("[{}] Failed to parse response for {} in year {}: {}",
                                        chain_name, address, year_offset + 1, e);
                                }
                            }
                        }
                        Err(e) => {
                            tracing::warn!("[{}] Failed to read response for {} in year {}: {}",
                                chain_name, address, year_offset + 1, e);
                        }
                    }
                }
                Ok(Err(e)) => {
                    tracing::warn!("[{}] Request failed for {} in year {}: {}",
                        chain_name, address, year_offset + 1, e);
                }
                Err(_) => {
                    tracing::warn!("[{}] Timeout for {} in year {}",
                        chain_name, address, year_offset + 1);
                }
            }

            // 在请求之间添加短暂延迟
            if year_offset < max_years - 1 {
                tokio::time::sleep(Duration::from_millis(200)).await;
            }
        }

        tracing::warn!("[{}] No transaction found in last {} years for {}",
            chain_name, max_years, address);
        Ok(None)
    }

    /// 检测 RPC 是否支持 Ankr 增强方法
    #[allow(dead_code)]
    pub async fn supports_ankr_methods(&self, rpc_url: &str) -> bool {
        // 尝试调用 Ankr 方法，如果失败则不支持
        let payload = json!({
            "jsonrpc": "2.0",
            "method": "ankr_getTransactionsByAddress",
            "params": {
                "address": "0x0000000000000000000000000000000000000000",
                "pageSize": 1,
                "pageToken": "",
                "descOrder": true
            },
            "id": 1
        });

        if let Ok(response) = self.client.post(rpc_url).json(&payload).send().await {
            if let Ok(text) = response.text().await {
                // 如果返回包含 result 而不是 error，则支持
                return !text.contains("method not found") && !text.contains("\"error\"");
            }
        }
        false
    }
}
