use ethers::types::Address;
use ethers::utils::keccak256;
use secp256k1::{SecretKey, PublicKey};
use std::str::FromStr;
use crate::error::{BlockchainError, Result};

#[derive(Debug, Clone)]
pub struct ParsedAddress {
    pub normalized_address: String,
}

pub struct AddressParser;

impl AddressParser {
    pub fn new() -> Self {
        Self
    }
    
    pub fn parse_address(&self, input: &str) -> Result<ParsedAddress> {
        let trimmed = input.trim();
        
        if trimmed.is_empty() {
            return Err(BlockchainError::InvalidAddress("Empty input".to_string()));
        }
        
        // 检查是否为以太坊地址格式
        if Self::is_ethereum_address(trimmed) {
            let normalized = Self::normalize_ethereum_address(trimmed)?;
            return Ok(ParsedAddress { normalized_address: normalized });
        }
        
        // 检查是否为私钥格式 (64或66字符的十六进制字符串)
        if Self::is_private_key_format(trimmed) {
            let address = Self::derive_address_from_private_key(trimmed)?;
            return Ok(ParsedAddress { normalized_address: address });
        }
        
        Err(BlockchainError::InvalidAddress(format!("Invalid address or private key format: {}", trimmed)))
    }
    
    pub fn parse_addresses_from_file(&self, content: &str) -> Result<Vec<ParsedAddress>> {
        let mut addresses = Vec::new();
        
        for line in content.lines() {
            let trimmed = line.trim();
            if !trimmed.is_empty() && !trimmed.starts_with('#') {
                match self.parse_address(trimmed) {
                    Ok(address) => addresses.push(address),
                    Err(e) => {
                        tracing::warn!("Failed to parse address '{}': {}", trimmed, e);
                    }
                }
            }
        }
        
        if addresses.is_empty() {
            return Err(BlockchainError::InvalidAddress("No valid addresses found in file".to_string()));
        }
        
        Ok(addresses)
    }
    
    fn is_ethereum_address(address: &str) -> bool {
        // 检查是否为0x开头的42字符十六进制地址
        if address.len() != 42 {
            return false;
        }
        
        if !address.starts_with("0x") && !address.starts_with("0X") {
            return false;
        }
        
        // 检查剩余部分是否为有效的十六进制
        hex::decode(&address[2..]).is_ok()
    }
    
    fn normalize_ethereum_address(address: &str) -> Result<String> {
        // 转换为小写并验证格式
        let lower = address.to_lowercase();
        let addr = Address::from_str(&lower)
            .map_err(|e| BlockchainError::InvalidAddress(format!("Invalid Ethereum address: {}", e)))?;
        
        Ok(format!("0x{:x}", addr))
    }
    
    fn is_private_key_format(key: &str) -> bool {
        // 检查是否为64字符的十六进制私钥，或66字符的0x前缀私钥
        let clean_key = if key.starts_with("0x") || key.starts_with("0X") {
            &key[2..]
        } else {
            key
        };
        
        clean_key.len() == 64 && hex::decode(clean_key).is_ok()
    }
    
    fn derive_address_from_private_key(private_key: &str) -> Result<String> {
        let clean_key = if private_key.starts_with("0x") || private_key.starts_with("0X") {
            &private_key[2..]
        } else {
            private_key
        };
        
        let secret_bytes = hex::decode(clean_key)
            .map_err(|_e| BlockchainError::InvalidPrivateKey)?;
        
        let secret_key = SecretKey::from_slice(&secret_bytes)
            .map_err(|e| BlockchainError::Secp256k1Error(format!("Invalid private key: {}", e)))?;
        
        let secp = secp256k1::Secp256k1::new();
        let public_key = PublicKey::from_secret_key(&secp, &secret_key);
        
        // 将公钥转换为以太坊地址
        let public_key_bytes = public_key.serialize_uncompressed();
        let hash = keccak256(&public_key_bytes[1..]); // 跳过0x04前缀
        let address_bytes = &hash[12..]; // 取后20字节
        
        Ok(format!("0x{}", hex::encode(address_bytes)))
    }
}

impl Default for AddressParser {
    fn default() -> Self {
        Self::new()
    }
}