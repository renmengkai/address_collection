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
                .timeout(Duration::from_secs(20))
                .pool_idle_timeout(Duration::from_secs(30))
                .pool_max_idle_per_host(20)  // 增加每个主机的最大空闲连接
                .tcp_keepalive(Duration::from_secs(30))
                .http2_keep_alive_interval(Duration::from_secs(15))
                .http2_keep_alive_timeout(Duration::from_secs(5))
                .http2_keep_alive_while_idle(true)
                .build()
                .unwrap(),
        }
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
        for attempt in 1..=3 {
            tracing::debug!("[{}] Attempt {} for address {}", chain_name, attempt, address);

            // 添加 15 秒超时保护整个请求
            let request = self.client
                .post(rpc_url)
                .json(&payload);

            match timeout(Duration::from_secs(15), request.send()).await {
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
                        let delay_ms = 200 * (2_u64.pow(attempt - 1));
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
                        let delay_ms = 200 * (2_u64.pow(attempt - 1));
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

    /// 检测 RPC 是否支持 Ankr 增强方法
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
