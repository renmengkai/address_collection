use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fs;
use std::path::Path;
use crate::error::{BlockchainError, Result};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChainConfig {
    pub name: String,
    pub display_name: String,
    pub rpc_url: String,
    pub chain_id: u64,
    #[serde(rename = "type")]
    pub chain_type: ChainType,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ChainType {
    Evm,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub ankr_api_key: String,
    pub chains: Vec<ChainConfig>,
}

impl Config {
    pub fn from_file<P: AsRef<Path>>(path: P) -> Result<Self> {
        let content = fs::read_to_string(path)
            .map_err(|e| BlockchainError::ConfigError(format!("Failed to read config file: {}", e)))?;

        // 支持新格式：{"ankr_api_key": "...", "chains": ["ethereum", "polygon", ...]}
        let json: Value = serde_json::from_str(&content)
            .map_err(|e| BlockchainError::ConfigError(format!("Failed to parse config JSON: {}", e)))?;

        let mut chains: Vec<ChainConfig> = Vec::new();
        let mut ankr_api_key = String::new();

        match &json {
            Value::Object(map) => {
                // 读取 ankr_api_key
                if let Some(api_key_val) = map.get("ankr_api_key") {
                    if let Value::String(key) = api_key_val {
                        ankr_api_key = key.clone();
                    }
                }

                if let Some(chains_val) = map.get("chains") {
                    match chains_val {
                        // 新格式：chains 为字符串数组
                        Value::Array(chain_names) => {
                            for name_val in chain_names {
                                if let Value::String(name) = name_val {
                                    let chain_config = Self::build_chain_config_with_key(name, &ankr_api_key);
                                    chains.push(chain_config);
                                }
                            }
                        }
                        // 兼容旧格式：chains 为对象
                        Value::Object(chain_map) => {
                            for (name, val) in chain_map {
                                match val {
                                    Value::String(rpc) => {
                                        chains.push(Self::build_chain_config_simple(name, rpc));
                                    }
                                    Value::Object(obj) => {
                                        let rpc_url = obj.get("rpc_url").and_then(|v| v.as_str()).unwrap_or("").to_string();
                                        let chain_id = obj.get("chain_id").and_then(|v| v.as_u64()).unwrap_or_else(|| Self::default_chain_id(name));
                                        let display_name = obj.get("name").and_then(|v| v.as_str()).map(|s| s.to_string()).unwrap_or_else(|| Self::default_display_name(name));
                                        let chain_type = ChainType::Evm;

                                        chains.push(ChainConfig {
                                            name: name.to_string(),
                                            display_name,
                                            rpc_url,
                                            chain_id,
                                            chain_type,
                                        });
                                    }
                                    _ => {}
                                }
                            }
                        }
                        _ => {}
                    }
                } else {
                    // 兼容旧格式：顶层对象为 链名 -> RPC URL（无 chains 字段）
                    for (name, val) in map {
                        if name == "ankr_api_key" {
                            continue;
                        }
                        if let Value::String(rpc) = val {
                            chains.push(Self::build_chain_config_simple(name, rpc));
                        }
                    }
                }
            }
            _ => {}
        }

        let config = Config { ankr_api_key, chains };
        config.validate()?;
        Ok(config)
    }
    
    pub fn validate(&self) -> Result<()> {
        if self.chains.is_empty() {
            return Err(BlockchainError::ConfigError("No chains configured".to_string()));
        }
        
        for chain in &self.chains {
            if chain.name.is_empty() {
                return Err(BlockchainError::ConfigError("Chain name cannot be empty".to_string()));
            }
            
            if chain.display_name.is_empty() {
                return Err(BlockchainError::ConfigError("Chain display name cannot be empty".to_string()));
            }
            
            if chain.rpc_url.is_empty() {
                return Err(BlockchainError::ConfigError(format!("RPC URL cannot be empty for chain {}", chain.name)));
            }
            
            // 对 EVM 链要求 chain_id 非 0；非 EVM（如 Bitcoin）可以为 0
            if matches!(chain.chain_type, ChainType::Evm) && chain.chain_id == 0 {
                return Err(BlockchainError::ConfigError(format!("Chain ID cannot be 0 for chain {}", chain.name)));
            }
        }
        
        Ok(())
    }
    
    pub fn get_chain(&self, name: &str) -> Option<&ChainConfig> {
        self.chains.iter().find(|chain| chain.name == name)
    }

    fn build_chain_config_simple(name: &str, rpc_url: &str) -> ChainConfig {
        ChainConfig {
            name: name.to_string(),
            display_name: Self::default_display_name(name),
            rpc_url: rpc_url.to_string(),
            chain_id: Self::default_chain_id(name),
            chain_type: ChainType::Evm,
        }
    }

    fn build_chain_config_with_key(name: &str, api_key: &str) -> ChainConfig {
        let rpc_url = Self::build_ankr_rpc_url(name, api_key);
        ChainConfig {
            name: name.to_string(),
            display_name: Self::default_display_name(name),
            rpc_url,
            chain_id: Self::default_chain_id(name),
            chain_type: ChainType::Evm,
        }
    }

    fn build_ankr_rpc_url(chain_name: &str, api_key: &str) -> String {
        let chain_lower = chain_name.to_lowercase();
        let ankr_chain = match chain_lower.as_str() {
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
            other => other,
        };
        format!("https://rpc.ankr.com/{}/{}", ankr_chain, api_key)
    }

    fn default_display_name(name: &str) -> String {
        match name.to_lowercase().as_str() {
            "ethereum" => "Ethereum Mainnet".to_string(),
            "bsc" | "bnb" => "BNB Smart Chain".to_string(),
            "polygon" => "Polygon Mainnet".to_string(),
            "arbitrum" => "Arbitrum One".to_string(),
            "optimism" => "Optimism Mainnet".to_string(),
            "avalanche" => "Avalanche C-Chain".to_string(),
            "base" => "Base Mainnet".to_string(),
            "linea" => "Linea Mainnet".to_string(),
            "fantom" => "Fantom Opera".to_string(),
            "gnosis" => "Gnosis Chain".to_string(),
            "scroll" => "Scroll Mainnet".to_string(),
            "zksync" => "zkSync Era".to_string(),
            other => {
                // 首字母大写
                let mut chars = other.chars();
                match chars.next() {
                    None => String::new(),
                    Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                }
            }
        }
    }

    fn default_chain_id(name: &str) -> u64 {
        match name.to_lowercase().as_str() {
            "ethereum" => 1,
            "bsc" | "bnb" => 56,
            "polygon" => 137,
            "arbitrum" => 42161,
            "optimism" => 10,
            "avalanche" => 43114,
            "base" => 8453,
            "linea" => 59144,
            "fantom" => 250,
            "gnosis" => 100,
            "scroll" => 534352,
            "zksync" => 324,
            _ => 1,
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Config {
            ankr_api_key: "YOUR_ANKR_API_KEY_HERE".to_string(),
            chains: vec![
                ChainConfig {
                    name: "ethereum".to_string(),
                    display_name: "Ethereum Mainnet".to_string(),
                    rpc_url: "https://rpc.ankr.com/eth/YOUR_ANKR_API_KEY_HERE".to_string(),
                    chain_id: 1,
                    chain_type: ChainType::Evm,
                },
                ChainConfig {
                    name: "bsc".to_string(),
                    display_name: "BNB Smart Chain".to_string(),
                    rpc_url: "https://rpc.ankr.com/bsc/YOUR_ANKR_API_KEY_HERE".to_string(),
                    chain_id: 56,
                    chain_type: ChainType::Evm,
                },
                ChainConfig {
                    name: "polygon".to_string(),
                    display_name: "Polygon".to_string(),
                    rpc_url: "https://rpc.ankr.com/polygon/YOUR_ANKR_API_KEY_HERE".to_string(),
                    chain_id: 137,
                    chain_type: ChainType::Evm,
                },
            ],
        }
    }
}