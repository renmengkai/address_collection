use crate::error::{BlockchainError, Result};
use crate::config::{ChainConfig, ChainType};
use super::traits::{BlockchainClient, BlockchainClientFactory};

pub struct EvmClientFactory {
    multichain_url: String,
}

impl EvmClientFactory {
    pub fn new(multichain_url: String) -> Self {
        Self { multichain_url }
    }
}

impl BlockchainClientFactory for EvmClientFactory {
    fn create_client(&self, rpc_url: &str, chain_name: &str, display_name: &str, chain_id: u64) -> Result<Box<dyn BlockchainClient>> {
        match chain_name {
            "ethereum" | "bsc" | "bnb" | "polygon" | "arbitrum" | "optimism" | 
            "avalanche" | "base" | "linea" | "fantom" | "gnosis" | "scroll" | "zksync" => {
                Ok(Box::new(super::ethereum::EthereumClient::new(
                    rpc_url.to_string(),
                    chain_name.to_string(),
                    display_name.to_string(),
                    chain_id,
                    self.multichain_url.clone(),
                )?))
            }
            _ => Err(BlockchainError::ConfigError(format!("Unsupported EVM chain: {}", chain_name))),
        }
    }
}

pub fn create_client_factory(chain_type: &ChainType, multichain_url: String) -> Result<Box<dyn BlockchainClientFactory>> {
    match chain_type {
        ChainType::Evm => Ok(Box::new(EvmClientFactory::new(multichain_url))),
    }
}

pub fn create_client_from_config(chain_config: &ChainConfig, ankr_api_key: &str) -> Result<Box<dyn BlockchainClient>> {
    let multichain_url = format!("https://rpc.ankr.com/multichain/{}", ankr_api_key);
    let factory = create_client_factory(&chain_config.chain_type, multichain_url)?;
    factory.create_client(
        &chain_config.rpc_url,
        &chain_config.name,
        &chain_config.display_name,
        chain_config.chain_id,
    )
}