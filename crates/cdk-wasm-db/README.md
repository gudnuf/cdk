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

// Create databases using new async constructors
let mint_db = MintWasmDatabase::new_async(":memory:").await.unwrap();
let wallet_db = WalletWasmDatabase::new_async(":memory:").await.unwrap();

// Alternative: synchronous constructor if you don't need async creation
let wallet_db2 = WalletWasmDatabase::new_sync(":memory:").unwrap();

// The databases implement the CDK WalletDatabase trait
use cdk_common::database::WalletDatabase;
use cdk_common::mint_url::MintUrl;
use std::str::FromStr;

let mint_url = MintUrl::from_str("https://mint.example.com").unwrap();
wallet_db.add_mint(mint_url, None).await.unwrap();
```

### In-Memory Database

```rust
// Both targets support in-memory mode
let db = WalletWasmDatabase::new_async(":memory:").await.unwrap();
```

### Thread Safety (Optional)

By default, the WASM implementation uses single-threaded storage (`Rc<RefCell<_>>`) which is suitable for most WASM environments. If you need thread-safe storage (e.g., for tests or specific multi-threaded WASM runtimes), enable the `threadsafe` feature:

```toml
[dependencies]
cdk-wasm-db = { version = "0.11.0", features = ["threadsafe"] }
```

With this feature enabled, the storage will use `Arc<Mutex<_>>` for thread safety.

### Constructor API Reference

The crate provides multiple ways to create database instances:

#### For Rust Code (Recommended)

```rust
use cdk_wasm_db::WalletWasmDatabase;

// Async constructor - use when you need async initialization
let db = WalletWasmDatabase::new_async(":memory:").await.unwrap();

// Sync constructor - use when you don't need async initialization
let db = WalletWasmDatabase::new_sync(":memory:").unwrap();
```

#### For JavaScript/TypeScript (via wasm-bindgen)

```javascript
// This returns a Promise in JavaScript
const db = new WalletWasmDatabase(":memory:");
```

#### Deprecated API (Internal Use)

```rust
// These are deprecated but still work
let db = WalletWasmDatabase::new_internal(":memory:".to_string()).await.unwrap();
```

### JavaScript/TypeScript Usage (via wasm-pack)

After building with wasm-pack, you can use the generated bindings in your web project:

```typescript
import init, { MintWasmDatabase, WalletWasmDatabase, init_db } from './pkg/cdk_wasm_db.js';

// Initialize the WASM module
async function setup() {
    await init();
    await init_db(); // Optional explicit initialization
}

// Use the wallet database
async function useWalletDb() {
    const db = new WalletWasmDatabase(":memory:");
    
    // Store and retrieve data
    await db.set("key", "value");
    const value = await db.get("key"); // Returns "value" or null
    
    // List all keys
    const keys = await db.keys(); // Returns array of strings
    
    // Remove a key
    await db.remove("key");
    
    // Clean up
    db.free();
}
```

## Building for WebAssembly

### Using wasm-pack (Recommended for Web Projects)

The crate is designed to work with `wasm-pack` for generating JavaScript bindings:

```bash
# Install wasm-pack if you haven't already
cargo install wasm-pack

# Build the crate for web use
wasm-pack build --target web --out-dir pkg

# The generated files will be in the pkg/ directory:
# - cdk_wasm_db.js        - JavaScript bindings
# - cdk_wasm_db.d.ts      - TypeScript definitions  
# - cdk_wasm_db_bg.wasm   - Compiled WebAssembly module
# - package.json          - NPM package metadata
```

### Manual Cargo Build

You can also build manually using cargo:

```bash
# Build for WASM (uses lightweight implementation)
cargo build --target wasm32-unknown-unknown

# Build for native (uses cdk-sqlite)
cargo build
```

### Integration with Build Scripts

You can integrate wasm-pack builds into shell scripts like this:

```bash
#!/bin/bash
set -e

# Set environment variables to fix Nix hardening issues with WASM compilation
export NIX_HARDENING_ENABLE=""
export CC_wasm32_unknown_unknown=/usr/bin/clang

echo "Building WASM module..."
wasm-pack build --target web --out-dir pkg

echo "Building TypeScript..."
bun run build:ts

echo "Copying WASM files to dist..."
mkdir -p dist/wasm
cp pkg/*.wasm dist/wasm/
cp pkg/*.js dist/wasm/

echo "Build complete!"
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

### Rust API (Internal/Native)

For Rust code and internal usage:

```rust
// Internal WASM methods (not exposed to JavaScript)
impl MintWasmDatabase {
    pub async fn new_internal<T: Into<String>>(path: T) -> Result<Self, WasmDbError>;
    pub async fn set_internal(&self, key: String, value: String) -> Result<(), WasmDbError>;
    pub async fn get_internal(&self, key: &str) -> Result<Option<String>, WasmDbError>;
    pub async fn remove_internal(&self, key: &str) -> Result<(), WasmDbError>;
    pub async fn keys_internal(&self) -> Result<Vec<String>, WasmDbError>;
}

// Native targets use cdk-sqlite (full CDK database functionality)
impl WalletWasmDatabase {
    pub async fn new(path: &str) -> Result<Self, Error>; // Full cdk-sqlite API
    // ... all other cdk-sqlite methods
}
```

### JavaScript API (via wasm-pack)

For web projects using the wasm-pack generated bindings:

```typescript
class MintWasmDatabase {
    constructor(path: string);
    set(key: string, value: string): Promise<void>;
    get(key: string): Promise<string | null>;
    remove(key: string): Promise<void>;
    keys(): Promise<string[]>;
    free(): void; // Clean up WASM memory
}

class WalletWasmDatabase {
    // Same methods as MintWasmDatabase
}

class WasmDbError {
    constructor(message: string);
    message: string;
}

// Initialization functions
function init(): void; // Called automatically
function init_db(): Promise<void>; // Optional explicit init
```

### Cross-Platform

```rust
pub async fn init(); // Safe to call on any target (Rust)
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