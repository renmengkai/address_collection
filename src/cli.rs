use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(
    name = "blockchain-query",
    about = "A high-performance blockchain address query tool",
    version = "1.0.0",
    author = "Blockchain Query Tool"
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,

    /// Configuration file path
    #[arg(short, long, default_value = "config.json")]
    pub config: PathBuf,

    /// Enable verbose logging
    #[arg(short, long)]
    pub verbose: bool,

    /// Maximum concurrent requests
    #[arg(short = 'j', long, default_value = "50")]
    pub max_concurrent: usize,

    /// Number of retry attempts for failed requests
    #[arg(short = 'r', long, default_value = "3")]
    pub retry_attempts: u32,

    /// Delay between retries in milliseconds
    #[arg(short = 'd', long, default_value = "500")]
    pub retry_delay: u64,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Query addresses from a file
    Query {
        /// Path to file containing addresses (one per line)
        #[arg(short, long)]
        input: Option<PathBuf>,

        /// Blockchain chains to query (ethereum, bsc, polygon, etc.). If not specified, queries all chains in config.
        #[arg(short = 'C', long, value_delimiter = ',')]
        chains: Vec<String>,

        /// Output CSV file path
        #[arg(short, long, default_value = "results.csv")]
        output: PathBuf,
    },

    /// Validate addresses without querying blockchain
    Validate {
        /// Path to file containing addresses (one per line)
        #[arg(short, long)]
        input: PathBuf,

        /// Output validation results to file
        #[arg(short, long)]
        output: Option<PathBuf>,
    },

    /// List supported blockchain chains
    Chains,

    /// Test RPC connection to specified chains
    Test {
        /// Blockchain chains to test (ethereum, bsc, polygon)
        #[arg(short = 'C', long, value_delimiter = ',')]
        chains: Vec<String>,
    },

    /// Show configuration information
    Config {
        /// Show detailed configuration for specific chain
        #[arg(short, long)]
        chain: Option<String>,
    },
}

impl Cli {
    pub fn validate(&self) -> Result<(), String> {
        match &self.command {
            Commands::Query { chains, .. } => {
                // 空的 chains 表示查询所有链，这是允许的
                for chain in chains {
                    let chain_lower = chain.to_lowercase();
                    if !Self::is_supported_chain(&chain_lower) {
                        return Err(format!(
                            "Unsupported chain: {}. Supported chains: ethereum, bsc, bnb, polygon, arbitrum, optimism, avalanche, base, linea, fantom, gnosis, scroll, zksync", 
                            chain
                        ));
                    }
                }
            }
            Commands::Test { chains, .. } => {
                if chains.is_empty() {
                    return Err("At least one chain must be specified for testing".to_string());
                }
                
                for chain in chains {
                    let chain_lower = chain.to_lowercase();
                    if !Self::is_supported_chain(&chain_lower) {
                        return Err(format!(
                            "Unsupported chain: {}. Supported chains: ethereum, bsc, bnb, polygon, arbitrum, optimism, avalanche, base, linea, fantom, gnosis, scroll, zksync", 
                            chain
                        ));
                    }
                }
            }
            _ => {}
        }
        
        Ok(())
    }
    
    fn is_supported_chain(chain: &str) -> bool {
        matches!(chain, 
            "ethereum" | "bsc" | "bnb" | "polygon" | "arbitrum" | 
            "optimism" | "avalanche" | "base" | "linea" | "fantom" | 
            "gnosis" | "scroll" | "zksync"
        )
    }
}