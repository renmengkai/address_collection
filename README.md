# Blockchain Address Query Tool

A high-performance Rust-based blockchain address query tool that queries balance and transaction information across multiple blockchain networks.

## Features

- **Multi-chain Support**: Query Ethereum, BSC, Polygon, Arbitrum, Optimism, Base, Linea, zkSync Era, and more
- **High Performance**: Concurrent request handling with configurable parallelism
- **Batch Processing**: Process thousands of addresses efficiently
- **CSV Export**: Results exported to CSV format for easy analysis
- **Flexible Configuration**: Support for both simple and advanced config formats
- **Address Validation**: Validate Ethereum addresses without querying blockchain
- **Retry Mechanism**: Automatic retry with exponential backoff for failed requests
- **Progress Tracking**: Visual progress bars for long-running operations

## Supported Chains

| Chain | Chain ID | Display Name |
|-------|----------|--------------|
| ethereum | 1 | Ethereum Mainnet |
| bsc/bnb | 56 | BNB Smart Chain |
| polygon | 137 | Polygon Mainnet |
| arbitrum | 42161 | Arbitrum One |
| optimism | 10 | Optimism Mainnet |
| base | 8453 | Base Mainnet |
| linea | 59144 | Linea Mainnet |
| zksync | 324 | zkSync Era |
| avalanche | 43114 | Avalanche C-Chain |
| fantom | 250 | Fantom Opera |
| gnosis | 100 | Gnosis Chain |
| scroll | 534352 | Scroll Mainnet |

## Installation

### Prerequisites

- Rust 1.70+ (install from [rustup.rs](https://rustup.rs/))
- Cargo (included with Rust)

### Build from Source

```bash
# Clone the repository
git clone <repository-url>
cd address_collection

# Build release version
cargo build --release

# The binary will be at target/release/address_collection
# You can copy it to your PATH for global access:
sudo cp target/release/address_collection /usr/local/bin/
```

### Development Build

```bash
# Build debug version (faster compilation, slower execution)
cargo build

# Run directly with cargo
cargo run -- query --input addresses.txt
```

## Configuration

Create a `config.json` file in the project root directory:

### Simple Format (using Ankr RPC)

```json
{
  "ankr_api_key": "your-ankr-api-key",
  "chains": ["ethereum", "arbitrum", "optimism", "base", "zksync"]
}
```

### Advanced Format (custom RPC URLs)

```json
{
  "ankr_api_key": "your-ankr-api-key",
  "chains": [
    {
      "name": "ethereum",
      "display_name": "Ethereum Mainnet",
      "rpc_url": "https://rpc.ankr.com/eth/your-api-key",
      "chain_id": 1,
      "chain_type": "ethereum"
    },
    {
      "name": "bsc",
      "display_name": "BNB Smart Chain",
      "rpc_url": "https://rpc.ankr.com/bsc/your-api-key",
      "chain_id": 56,
      "chain_type": "ethereum"
    }
  ]
}
```

### Getting Ankr API Key

1. Visit [Ankr RPC](https://www.ankr.com/rpc/)
2. Sign up for a free account
3. Create a new API key
4. Copy the API key to your `config.json`

## Usage

### Query Addresses

Query addresses from default input file on all configured chains:

```bash
# Uses address.txt (preferred) or addresses.txt (fallback)
./address_collection query
```

Query specific input file:

```bash
./address_collection query --input addresses.txt --output results.csv
```

Query specific chains:

```bash
./address_collection query --chains ethereum,arbitrum,zksync
```

Query with custom concurrency:

```bash
./address_collection query -j 20 --chains ethereum
```

### Validate Addresses

Validate addresses without querying blockchain:

```bash
./address_collection validate --input addresses.txt --output validation.csv
```

### Test RPC Connections

Test if RPC connections are working:

```bash
./address_collection test --chains ethereum,bsc,polygon
```

### List Configured Chains

```bash
./address_collection chains
```

### Show Configuration

Show current configuration:

```bash
./address_collection config
```

Show specific chain configuration:

```bash
./address_collection config --chain ethereum
```

## Command Options

### Global Options

| Option | Short | Description | Default |
|--------|-------|-------------|---------|
| `--config` | `-c` | Configuration file path | `config.json` |
| `--verbose` | `-v` | Enable verbose logging | `false` |
| `--max-concurrent` | `-j` | Maximum concurrent requests | `10` |
| `--retry-attempts` | `-r` | Number of retry attempts | `3` |
| `--retry-delay` | `-d` | Delay between retries in ms | `1000` |

### Query Command Options

| Option | Short | Description | Default |
|--------|-------|-------------|---------|
| `--input` | `-i` | Input addresses file | `address.txt` or `addresses.txt` |
| `--output` | `-o` | Output CSV file | `results.csv` |
| `--chains` |  | Comma-separated list of chains | All configured chains |

## Input File Format

### Addresses File

Create a text file with one Ethereum address per line:

```
0x3e35e74fd3d3aec484eb26d1461a074e4aca15de
0x37dbd334f44ff13345cb5f0ea5786a37bc02bc4c
0x4dd00520f4bb5b5a4664e00a3266bc74e6961ba2
```

**Supported address formats:**
- Lowercase: `0x3e35e74fd3d3aec484eb26d1461a074e4aca15de`
- Uppercase: `0x3E35E74FD3D3AEC484EB26D1461A074E4ACA15DE`
- Mixed case (EIP-55): `0x3e35e74fd3d3aEc484eb26d1461A074e4aca15de`

### Generating Addresses from Private Keys

If you have a `privateKeys.txt` file with private keys, you can generate the corresponding addresses:

```bash
# Example: Use cast (from foundry) to derive addresses
while read -r key; do
  cast wallet address "$key"
done < privateKeys.txt > addresses.txt
```

## Output Format

### CSV Output Columns

| Column | Description |
|--------|-------------|
| `address` | Ethereum address |
| `chain_name` | Chain identifier (e.g., ethereum, bsc) |
| `chain_display_name` | Human-readable chain name |
| `balance` | Account balance in ETH/native token |
| `transaction_count` | Total number of transactions |
| `last_transaction_time` | Timestamp of last transaction |
| `last_transaction_hash` | Hash of last transaction |
| `status` | Query status (success/error) |
| `error_message` | Error details if query failed |

### Example Output

```csv
address,chain_name,chain_display_name,balance,transaction_count,last_transaction_time,last_transaction_hash,status,error_message
0x3e35...a15de,ethereum,Ethereum Mainnet,1.23456789,42,2024-01-15T10:30:00Z,0xabc...def,success,
0x37db...2bc4c,bsc,BNB Smart Chain,0.0,0,,,success,
```

## Project Structure

```
address_collection/
├── src/
│   ├── main.rs              # Entry point and command handlers
│   ├── cli.rs               # CLI argument parsing
│   ├── config.rs            # Configuration loading
│   ├── address.rs           # Address parsing and validation
│   ├── query.rs             # Query engine with concurrent processing
│   ├── export.rs            # CSV export functionality
│   ├── error.rs             # Error types and handling
│   └── blockchain/          # Blockchain client implementations
│       ├── mod.rs           # Module exports
│       ├── traits.rs        # BlockchainClient trait definition
│       ├── client.rs        # EVM client factory
│       ├── ethereum.rs      # Ethereum client implementation
│       └── enhanced_rpc.rs  # Enhanced RPC features
├── config.json              # Configuration file
├── config.json.example      # Example configuration
├── addresses.txt            # Input addresses (one per line)
├── address.txt              # Alternative input file (preferred)
├── privateKeys.txt          # Private keys (for address generation)
├── Cargo.toml               # Rust project configuration
└── README.md                # This file
```

## Performance Tuning

### Concurrent Requests

Adjust concurrency based on your RPC provider's rate limits:

```bash
# High concurrency for premium RPC (e.g., Ankr paid tier)
./address_collection query -j 50

# Lower concurrency for free tier
./address_collection query -j 5
```

### Recommended Settings

| RPC Provider | Max Concurrent | Retry Attempts | Retry Delay |
|--------------|----------------|----------------|-------------|
| Ankr Free | 10-20 | 3 | 1000ms |
| Ankr Paid | 50-100 | 3 | 500ms |
| Alchemy | 10-30 | 3 | 1000ms |
| Infura | 10-20 | 3 | 1000ms |
| Custom | 5-10 | 5 | 2000ms |

## Troubleshooting

### Common Issues

#### "Error reading default input files: address.txt/addresses.txt"

**Cause**: No input file found

**Solution**: Create an `address.txt` or `addresses.txt` file with Ethereum addresses

```bash
# Create sample addresses file
echo "0x3e35e74fd3d3aec484eb26d1461a074e4aca15de" > addresses.txt
```

#### "No valid blockchain clients could be created"

**Cause**: Invalid configuration or missing API key

**Solution**: Check your `config.json` file:
- Verify `ankr_api_key` is set correctly
- Ensure chain names are correct
- Check RPC URLs are accessible

#### "Connection failed" or timeout errors

**Cause**: Network issues or RPC provider problems

**Solution**:
- Check your internet connection
- Verify RPC URLs are correct
- Increase retry attempts: `-r 5 -d 2000`
- Try different RPC provider

#### "Rate limit exceeded"

**Cause**: Too many requests to RPC provider

**Solution**:
- Reduce concurrency: `-j 5`
- Increase retry delay: `-d 2000`
- Upgrade to paid RPC tier

### Debug Mode

Enable verbose logging for troubleshooting:

```bash
./address_collection -v query --input addresses.txt
```

## Security Notes

- **Never commit private keys**: Keep `privateKeys.txt` in `.gitignore`
- **Protect your API keys**: Don't share `config.json` with real API keys
- **Use environment variables**: Consider using env vars for sensitive data
- **Validate addresses**: Always validate addresses before querying

## Development

### Running Tests

```bash
# Run all tests
cargo test

# Run with output
cargo test -- --nocapture
```

### Building Documentation

```bash
cargo doc --open
```

### Code Formatting

```bash
cargo fmt
cargo clippy
```

## Requirements

- Rust 1.70+
- Cargo
- Ankr RPC API key (free tier available)
- Internet connection for blockchain queries

## License

MIT License

## Contributing

Contributions are welcome! Please feel free to submit a Pull Request.

## Acknowledgments

- Built with [ethers-rs](https://github.com/gakonst/ethers-rs) for Ethereum interaction
- Uses [Ankr](https://www.ankr.com/) for RPC infrastructure
- CLI powered by [clap](https://github.com/clap-rs/clap)
