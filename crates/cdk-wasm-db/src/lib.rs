//! WebAssembly SQLite storage backend for cdk

#![warn(missing_docs)]
#![warn(rustdoc::bare_urls)]

// For WASM targets, provide a basic in-memory implementation
#[cfg(target_arch = "wasm32")]
mod wasm_impl;

#[cfg(target_arch = "wasm32")]
pub use wasm_impl::*;

// For non-WASM targets, re-export cdk-sqlite for compatibility
#[cfg(not(target_arch = "wasm32"))]
mod native_impl;

#[cfg(not(target_arch = "wasm32"))]
pub use native_impl::*;

/// Initialize the WASM SQLite environment
///
/// This function must be called before using any database functionality
/// in a WebAssembly environment. It sets up the necessary SQLite WASM
/// bindings and initializes the database engine.
#[cfg(target_arch = "wasm32")]
pub async fn init() {
    // For now, just a placeholder - sqlite-wasm-rs doesn't export init()
    // In a real implementation, this would initialize the WASM SQLite environment
}

/// Initialize the WASM SQLite environment
///
/// This is a no-op function for non-WASM targets.
#[cfg(not(target_arch = "wasm32"))]
pub async fn init() {
    // No-op for non-WASM targets
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_init() {
        // Test that init doesn't panic
        init().await;
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[tokio::test]
    async fn test_database_creation() {
        // Initialize first
        init().await;

        // Test wallet database creation
        let wallet_db = WalletWasmDatabase::new(":memory:").await;
        assert!(wallet_db.is_ok());

        // Test mint database creation
        let mint_db = MintWasmDatabase::new(":memory:").await;
        assert!(mint_db.is_ok());
    }
}
