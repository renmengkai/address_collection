use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use crate::error::Result;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransactionInfo {
    pub hash: String,
    pub timestamp: DateTime<Utc>,
    pub block_number: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddressInfo {
    pub address: String,
    pub balance: Option<String>,
    pub transaction_count: u64,
    pub last_transaction: Option<TransactionInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum QueryStatus {
    Success,
    NoTransactions,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryResult {
    pub address: String,
    pub chain_name: String,
    pub chain_display_name: String,
    pub balance: Option<String>,
    pub transaction_count: u64,
    pub last_transaction_time: Option<DateTime<Utc>>,
    pub last_transaction_hash: Option<String>,
    pub status: QueryStatus,
    pub error_message: Option<String>,
}

#[async_trait]
pub trait BlockchainClient: Send + Sync {
    async fn get_last_transaction(&self, address: &str) -> Result<Option<TransactionInfo>>;
    
    async fn get_address_info(&self, address: &str) -> Result<AddressInfo>;
    
    async fn validate_address(&self, address: &str) -> Result<bool>;
    
    fn get_chain_name(&self) -> &str;
    
    fn get_display_name(&self) -> &str;
    
    fn get_chain_id(&self) -> u64;
}

pub trait BlockchainClientFactory: Send + Sync {
    fn create_client(&self, rpc_url: &str, chain_name: &str, display_name: &str, chain_id: u64) -> Result<Box<dyn BlockchainClient>>;
}