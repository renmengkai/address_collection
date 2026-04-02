#[derive(Debug, thiserror::Error)]
pub enum BlockchainError {
    #[error("Invalid address format: {0}")]
    InvalidAddress(String),
    
    #[error("Invalid private key format")]
    InvalidPrivateKey,
    
    #[error("RPC connection failed: {0}")]
    RpcConnection(String),
    
    
    #[error("Configuration error: {0}")]
    ConfigError(String),
    
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
    
    #[error("CSV error: {0}")]
    CsvError(#[from] csv::Error),
    
    #[error("JSON error: {0}")]
    JsonError(#[from] serde_json::Error),
    
    #[error("URL parse error: {0}")]
    UrlError(#[from] url::ParseError),
    
    
    #[error("Hex decode error: {0}")]
    HexError(#[from] hex::FromHexError),
    
    #[error("Secp256k1 error: {0}")]
    Secp256k1Error(String),
    
    #[error("Join error: {0}")]
    JoinError(#[from] tokio::task::JoinError),
    
    #[error("Timeout error: {0}")]
    TimeoutError(String),
    
    // 移除未使用的枚举变体以清除警告
}

pub type Result<T> = std::result::Result<T, BlockchainError>;