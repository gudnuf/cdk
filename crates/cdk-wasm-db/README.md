# CDK WASM DB

WebAssembly-compatible SQLite storage backend for the Cashu Development Kit (CDK).

This crate provides database functionality that works across both native and WebAssembly environments through conditional compilation:

- **Native targets**: Uses `cdk-sqlite` for full SQLite functionality and CDK database trait compatibility
- **WASM targets**: Provides a lightweight in-memory key-value storage implementation

## Features

- **mint**: Mint database functionality
- **wallet**: Wallet database functionality  
- **auth**: Authentication functionality
- Automatic target detection (native vs WASM)
- Simple key-value storage API for WASM
- Full CDK database compatibility for native targets

## Usage

### Basic Example

```rust
use cdk_wasm_db::{MintWasmDatabase, WalletWasmDatabase};

// Initialize the WASM environment (safe to call on native targets too)
cdk_wasm_db::init().await;

// Create databases
let mint_db = MintWasmDatabase::new("mint.db").await.unwrap();
let wallet_db = WalletWasmDatabase::new("wallet.db").await.unwrap();

// WASM targets: Use simple key-value operations
#[cfg(target_arch = "wasm32")]
{
    wallet_db.set("key".to_string(), "value".to_string()).await.unwrap();
    let value = wallet_db.get("key").await.unwrap();
}

// Native targets: Use full CDK database interface  
#[cfg(not(target_arch = "wasm32"))]
{
    // Full CDK database operations available
    // ...
}
```

### In-Memory Database

```rust
// Both targets support in-memory mode
let db = WalletWasmDatabase::new(":memory:").await.unwrap();
```

## Building for WebAssembly

The crate automatically detects the target and uses appropriate dependencies:

```bash
# Build for WASM (uses lightweight implementation)
cargo build --target wasm32-unknown-unknown

# Build for native (uses cdk-sqlite)
cargo build
```

## Implementation Details

### Native Targets
- Re-exports `cdk-sqlite` types for full compatibility
- Supports all CDK database operations
- Uses SQLite for persistent storage
- Full feature parity with existing CDK database backends

### WASM Targets  
- Lightweight in-memory HashMap-based storage
- Simple key-value API: `set()`, `get()`, `remove()`, `keys()`
- No native dependencies (avoids secp256k1, rusqlite, etc.)
- Future versions will add browser storage backends (localStorage, IndexedDB, OPFS)

## API Reference

### WASM-Specific Methods

```rust
impl MintWasmDatabase {
    pub async fn new<T: Into<String>>(path: T) -> Result<Self, WasmDbError>;
    pub async fn set(&self, key: String, value: String) -> Result<(), WasmDbError>;
    pub async fn get(&self, key: &str) -> Result<Option<String>, WasmDbError>;
    pub async fn remove(&self, key: &str) -> Result<(), WasmDbError>;
    pub async fn keys(&self) -> Result<Vec<String>, WasmDbError>;
}

impl WalletWasmDatabase {
    // Same methods as MintWasmDatabase
}
```

### Cross-Platform

```rust
pub async fn init(); // Safe to call on any target
```

## Requirements

- Rust 1.85.0 or later
- For WASM builds: `wasm32-unknown-unknown` target installed

## Current Limitations

### WASM Implementation
- In-memory storage only (data lost on page reload)
- Simple key-value operations (not full CDK database trait compatibility)
- No cryptographic operations (avoids native dependencies)

### Native Implementation
- Full feature support via `cdk-sqlite`

## Future Enhancements

1. **WASM Storage Backends**:
   - IndexedDB integration for persistent browser storage
   - localStorage for simple key-value persistence
   - Origin Private File System (OPFS) support

2. **Enhanced WASM Database**:
   - Full CDK database trait implementation for WASM
   - SQLite WASM backend using `sql.js` or similar
   - Cross-platform serialization for data portability

3. **Performance Optimizations**:
   - Batch operations for WASM
   - Compression for stored data
   - Background sync between memory and persistent storage

## Troubleshooting

### WASM Compilation Issues

If you encounter issues building for WASM, ensure you have the target installed:

```bash
rustup target add wasm32-unknown-unknown
```

### Missing Features

The WASM implementation currently provides basic storage functionality. For full CDK database operations in WASM environments, consider:

1. Using the native implementation with a WASM-compatible runtime
2. Implementing a bridge to IndexedDB or other browser storage APIs
3. Contributing to enhance the WASM implementation

## Contributing

Contributions to enhance WASM compatibility and add browser storage backends are welcome!