# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

A high-performance Rust-based blockchain address query tool that queries balance and transaction information across multiple EVM-compatible blockchain networks using Ankr RPC API.

## Build Commands

```bash
# Development build
cargo build

# Release build (optimized, LTO enabled)
cargo build --release

# Binary location: target/release/blockchain-query
```

## Architecture

### Query Flow
1. **CLI** (`cli.rs`) parses arguments → **Config** (`config.rs`) loads chain settings
2. **QueryEngine** (`query.rs`) manages concurrent execution across chains
3. **EthereumClient** (`blockchain/ethereum.rs`) fetches data via RPC
4. **EnhancedRpcClient** (`blockchain/enhanced_rpc.rs`) uses Ankr's `ankr_getTransactionsByAddress` API for efficient transaction lookup
5. Results exported via **CsvExporter** (`export.rs`)

### Key Design Patterns
- **Async/Await**: Uses Tokio runtime with configurable concurrency
- **Rate Limiting**: Per-chain semaphore controls concurrent requests (default: 50 per chain)
- **Fallback Strategy**: Ankr enhanced API → Standard RPC → None if no transactions
- **Parallel Arrays**: `addresses.txt` and `privateKeys.txt` use matching line indices

### Critical Files
- `src/query.rs:24`: `concurrent_per_chain` - controls per-chain concurrency (50 default)
- `src/cli.rs:24-32`: Default values for `-j`, `-r`, `-d` flags
- `src/blockchain/enhanced_rpc.rs:33-41`: HTTP client connection pool settings
- `src/config.rs:160-177`: Chain name → Ankr RPC endpoint mapping

## Supported Chains

Chain config mapping (`config.rs`):
| Input | Ankr RPC | Chain ID |
|-------|----------|----------|
| ethereum | eth | 1 |
| bsc/bnb | bsc | 56 |
| polygon | polygon | 137 |
| arbitrum | arbitrum | 42161 |
| optimism | optimism | 10 |
| base | base | 8453 |
| linea | linea | 59144 |
| zksync | zksync_era | 324 |
| avalanche | avalanche | 43114 |
| fantom | fantom | 250 |
| gnosis | gnosis | 100 |
| scroll | scroll | 534352 |

## Common Tasks

**Query with higher concurrency for speed:**
```bash
./blockchain-query query -j 100 -r 5
```

**Query specific chains:**
```bash
./blockchain-query query --chains ethereum,zksync,arbitrum
```

**Test RPC connections:**
```bash
./blockchain-query test --chains ethereum,bsc,polygon
```

## Configuration

`config.json` supports two formats:
- **Simple**: `{"ankr_api_key": "...", "chains": ["ethereum", "zksync"]}`
- **Advanced**: `{"chains": {"ethereum": {"rpc_url": "...", "chain_id": 1}}}`

Ankr API key required from https://www.ankr.com/rpc/

## Performance Tuning

Key parameters in `cli.rs`:
- `-j, --max-concurrent`: Total concurrent requests (default: 50)
- `-r, --retry-attempts`: Retry count on failure (default: 3)
- `-d, --retry-delay`: Initial delay before retry in ms (default: 500)

Connection pool tuning in `enhanced_rpc.rs:33-41` affects HTTP throughput.
