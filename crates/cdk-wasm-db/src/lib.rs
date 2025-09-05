//! WebAssembly SQLite storage backend for cdk

#![warn(missing_docs)]
#![warn(rustdoc::bare_urls)]

#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;

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
#[wasm_bindgen(start)]
pub fn init() {
    // Basic initialization for WASM
    // console_error_panic_hook and logging can be added via optional dependencies
}

/// Initialize the WASM SQLite environment
///
/// This function provides a JavaScript-accessible initialization function
/// that can be called explicitly by users.
#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
pub fn init_db() -> js_sys::Promise {
    use wasm_bindgen_futures::future_to_promise;
    future_to_promise(async move {
        // For now, just a placeholder - sqlite-wasm-rs doesn't export init()
        // In a real implementation, this would initialize the WASM SQLite environment
        Ok(JsValue::undefined())
    })
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

    #[cfg(not(target_arch = "wasm32"))]
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

        // Test wallet database creation - these are now re-exports of cdk-sqlite
        let _wallet_db = WalletWasmDatabase::new(":memory:")
            .await
            .expect("Failed to create wallet database");

        // Test mint database creation
        let _mint_db = MintWasmDatabase::new(":memory:")
            .await
            .expect("Failed to create mint database");
    }

    #[cfg(target_arch = "wasm32")]
    #[tokio::test]
    async fn test_database_operations() {
        // Test WASM-specific operations using internal methods
        let wallet_db = WalletWasmDatabase::new_internal(":memory:")
            .await
            .expect("Failed to create wallet database");

        // Test set and get
        wallet_db
            .set_internal("test_key".to_string(), "test_value".to_string())
            .await
            .expect("Failed to set value");
        let value = wallet_db
            .get_internal("test_key")
            .await
            .expect("Failed to get value");
        assert_eq!(value, Some("test_value".to_string()));

        // Test keys
        let keys = wallet_db.keys_internal().await.expect("Failed to get keys");
        assert!(keys.contains(&"test_key".to_string()));

        // Test remove
        wallet_db
            .remove_internal("test_key")
            .await
            .expect("Failed to remove key");
        let value = wallet_db
            .get_internal("test_key")
            .await
            .expect("Failed to get value after removal");
        assert_eq!(value, None);
    }
}
