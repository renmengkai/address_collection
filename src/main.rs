mod address;
mod blockchain;
mod cli;
mod config;
mod error;
mod export;
mod query;

use clap::Parser;
use color_eyre::eyre::Result;
use std::sync::Arc;
use tracing::{info, warn};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use crate::blockchain::create_client_from_config;
use crate::cli::{Cli, Commands};
use crate::config::Config;
use crate::export::ExcelExporter;
use crate::query::QueryEngine;
use crate::address::AddressParser;

#[tokio::main]
async fn main() -> Result<()> {
    // 初始化错误处理
    color_eyre::install()?;

    // 解析命令行参数
    let cli = Cli::parse();

    // 验证参数
    if let Err(e) = cli.validate() {
        eprintln!("Error: {}", e);
        std::process::exit(1);
    }

    // 初始化日志
    let log_level = if cli.verbose { "debug" } else { "info" };
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| format!("blockchain_query={}", log_level).into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    info!("Blockchain Query Tool v1.0.0");
    info!("Configuration file: {:?}", cli.config);

    // 加载配置
    let config = match Config::from_file(&cli.config) {
        Ok(config) => {
            info!("Configuration loaded successfully");
            config
        }
        Err(e) => {
            warn!("Failed to load configuration file: {}. Using default configuration.", e);
            Config::default()
        }
    };

    // 执行命令
    match &cli.command {
        Commands::Query { input, chains, output } => {
            handle_query(&cli, &config, input, chains, output).await?;
        }
        Commands::Validate { input, output } => {
            handle_validate(input, output.as_ref()).await?;
        }
        Commands::Chains => {
            handle_chains(&config);
        }
        Commands::Test { chains } => {
            handle_test(&config, chains).await?;
        }
        Commands::Config { chain } => {
            handle_config(&config, chain.as_ref());
        }
    }

    Ok(())
}

async fn handle_query(
    cli: &Cli,
    config: &Config,
    input: &Option<std::path::PathBuf>,
    chains: &[String],
    output: &std::path::PathBuf,
) -> Result<()> {
    info!("Starting query operation");
    info!("Input file arg: {:?}", input);
    
    // 如果未指定 chains，使用配置文件中的所有链
    let target_chains: Vec<String> = if chains.is_empty() {
        let all_chains: Vec<String> = config.chains.iter()
            .map(|c| c.name.clone())
            .collect();
        info!("No chains specified, querying all chains from config: {:?}", all_chains);
        all_chains
    } else {
        info!("Chains: {:?}", chains);
        chains.to_vec()
    };
    
    info!("Output file: {:?}", output);

    // 读取输入文件（支持默认 addresses.txt；不存在则尝试 address.txt）
    let (input_path, input_content) = if let Some(provided) = input.as_ref() {
        match tokio::fs::read_to_string(provided).await {
            Ok(content) => (provided.clone(), content),
            Err(e) => {
                eprintln!("Error reading input file {:?}: {}", provided, e);
                std::process::exit(1);
            }
        }
    } else {
        // 优先使用 address.txt，若不存在则回退到 addresses.txt
        let default1 = std::path::PathBuf::from("address.txt");
        match tokio::fs::read_to_string(&default1).await {
            Ok(content) => (default1, content),
            Err(_) => {
                let default2 = std::path::PathBuf::from("addresses.txt");
                match tokio::fs::read_to_string(&default2).await {
                    Ok(content) => (default2, content),
                    Err(e2) => {
                        eprintln!("Error reading default input files: address.txt/addresses.txt: {}", e2);
                        std::process::exit(1);
                    }
                }
            }
        }
    };
    info!("Resolved input file: {:?}", input_path);

    // 解析地址
    let parser = AddressParser::new();
    let parsed_addresses = match parser.parse_addresses_from_file(&input_content) {
        Ok(addresses) => addresses,
        Err(e) => {
            eprintln!("Error parsing addresses: {}", e);
            std::process::exit(1);
        }
    };

    let addresses: Vec<String> = parsed_addresses.iter()
        .map(|addr| addr.normalized_address.clone())
        .collect();

    if addresses.is_empty() {
        eprintln!("No valid addresses found in input file");
        std::process::exit(1);
    }

    // 创建查询引擎
    let mut engine = QueryEngine::new(
        cli.max_concurrent,
        cli.retry_attempts,
        cli.retry_delay,
    );

    // 为每个链添加客户端
    for chain_name in &target_chains {
        let chain_lower = chain_name.to_lowercase();
        match config.get_chain(&chain_lower) {
            Some(chain_config) => {
                match create_client_from_config(chain_config, &config.ankr_api_key) {
                    Ok(client) => {
                        engine.add_client(chain_lower.clone(), Arc::from(client));
                        info!("Added client for chain: {}", chain_name);
                    }
                    Err(e) => {
                        warn!("Failed to create client for chain {}: {}", chain_name, e);
                    }
                }
            }
            None => {
                warn!("No configuration found for chain: {}", chain_name);
            }
        }
    }

    if engine.supported_chains().is_empty() {
        eprintln!("No valid blockchain clients could be created");
        std::process::exit(1);
    }

    // 执行查询
    println!("\nQuerying {} addresses on {} chains...", addresses.len(), engine.supported_chains().len());
    
    let results = match engine.query_multiple_chains(&addresses, &target_chains).await {
        Ok(results) => results,
        Err(e) => {
            eprintln!("Query failed: {}", e);
            std::process::exit(1);
        }
    };

    // 导出结果到Excel
    let exporter = ExcelExporter::new();
    
    // 修改输出文件扩展名为.xlsx
    let excel_output = output.with_extension("xlsx");
    
    match exporter.export_to_file(&results, &excel_output) {
        Ok(_) => {
            println!("\nResults exported to: {:?}", excel_output);
        }
        Err(e) => {
            eprintln!("Failed to export results: {}", e);
            std::process::exit(1);
        }
    }

    Ok(())
}

async fn handle_validate(
    input: &std::path::PathBuf,
    output: Option<&std::path::PathBuf>,
) -> Result<()> {
    info!("Starting address validation");
    info!("Input file: {:?}", input);

    let parser = AddressParser::new();
    let input_content = match tokio::fs::read_to_string(input).await {
        Ok(content) => content,
        Err(e) => {
            eprintln!("Error reading input file: {}", e);
            std::process::exit(1);
        }
    };

    let parsed_addresses = match parser.parse_addresses_from_file(&input_content) {
        Ok(addresses) => addresses,
        Err(e) => {
            eprintln!("Error parsing addresses: {}", e);
            std::process::exit(1);
        }
    };

    let addresses: Vec<String> = parsed_addresses.iter()
        .map(|addr| addr.normalized_address.clone())
        .collect();

    println!("\nAddress Validation Results:");
    println!("{:<50} {:<10} {:<20}", "Address", "Valid", "Type");
    println!("{}", "-".repeat(80));

    let mut valid_count = 0;
    let mut invalid_count = 0;

    for address in &addresses {
        let (is_valid, addr_type) = if address.starts_with("0x") && address.len() == 42 {
            (true, "Ethereum")
        } else {
            (false, "Unknown")
        };

        if is_valid {
            valid_count += 1;
            println!("{:<50} {:<10} {:<20}", address, "✓", addr_type);
        } else {
            invalid_count += 1;
            println!("{:<50} {:<10} {:<20}", address, "✗", addr_type);
        }
    }

    println!("\nValidation Summary:");
    println!("  Total addresses: {}", addresses.len());
    println!("  Valid addresses: {}", valid_count);
    println!("  Invalid addresses: {}", invalid_count);

    if let Some(output_path) = output {
        use std::fs::File;
        use std::io::Write;
        
        let mut file = File::create(output_path)?;
        writeln!(file, "Address,Valid,Type")?;
        
        for address in &addresses {
            let (is_valid, addr_type) = if address.starts_with("0x") && address.len() == 42 {
                ("true", "Ethereum")
            } else {
                ("false", "Unknown")
            };
            writeln!(file, "{},{},{}", address, is_valid, addr_type)?;
        }
        
        println!("\nValidation results saved to: {:?}", output_path);
    }

    Ok(())
}

fn handle_chains(config: &Config) {
    println!("Configured blockchain chains:");
    println!("{:<15} {:<25} {:<10}", "Chain", "Display Name", "Chain ID");
    println!("{}", "-".repeat(80));

    for chain_config in &config.chains {
        println!(
            "{:<15} {:<25} {:<10}",
            chain_config.name,
            chain_config.display_name,
            chain_config.chain_id
        );
    }
    
    println!("\nTotal chains configured: {}", config.chains.len());
}

async fn handle_test(config: &Config, chains: &[String]) -> Result<()> {
    println!("Testing RPC connections...\n");

    for chain_name in chains {
        let chain_lower = chain_name.to_lowercase();
        
        match config.get_chain(&chain_lower) {
            Some(chain_config) => {
                print!("Testing {}... ", chain_name);
                
                match create_client_from_config(chain_config, &config.ankr_api_key) {
                    Ok(client) => {
                        // 使用未调用的方法以消除编译警告
                        let _is_valid = client.validate_address("0x0000000000000000000000000000000000000000").await.unwrap_or(false);
                        let _cid = client.get_chain_id();
                        // 测试连接 - 尝试获取最后一个交易
                        match client.get_last_transaction("0x0000000000000000000000000000000000000000").await {
                            Ok(_) => {
                                println!("✓ Connection successful");
                            }
                            Err(e) => {
                                println!("✗ Connection failed: {}", e);
                            }
                        }
                    }
                    Err(e) => {
                        println!("✗ Client creation failed: {}", e);
                    }
                }
            }
            None => {
                println!("{}: ✗ No configuration found", chain_name);
            }
        }
    }

    Ok(())
}

fn handle_config(config: &Config, chain: Option<&String>) {
    if let Some(chain_name) = chain {
        let chain_lower = chain_name.to_lowercase();
        
        if let Some(chain_config) = config.get_chain(&chain_lower) {
            println!("Configuration for chain: {}", chain_name);
            println!("  Type: {:?}", chain_config.chain_type);
            println!("  RPC URL: {}", chain_config.rpc_url);
            println!("  Chain ID: {}", chain_config.chain_id);
        } else {
            println!("No configuration found for chain: {}", chain_name);
        }
    } else {
        println!("Current configuration:");
        println!("{}", serde_json::to_string_pretty(config).unwrap_or_else(|_| "Failed to serialize config".to_string()));
    }
}
