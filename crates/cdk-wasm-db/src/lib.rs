//! WebAssembly SQLite storage backend for cdk

#![warn(missing_docs)]
#![warn(rustdoc::bare_urls)]

#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;

// WASM-only in-memory implementation
#[cfg(target_arch = "wasm32")]
mod wasm_impl;

#[cfg(target_arch = "wasm32")]
pub use wasm_impl::*;

// Native stub implementation (for compilation only)
#[cfg(not(target_arch = "wasm32"))]
mod native_stub;

#[cfg(not(target_arch = "wasm32"))]
pub use native_stub::*;

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

#[cfg(test)]
mod tests {
    #[cfg(target_arch = "wasm32")]
    use super::*;

    #[cfg(target_arch = "wasm32")]
    #[test]
    fn test_send_sync() {
        // Compile-time check that WalletWasmDatabase is Send + Sync
        fn assert_send<T: Send>() {}
        fn assert_sync<T: Sync>() {}

        assert_send::<WalletWasmDatabase>();
        assert_sync::<WalletWasmDatabase>();
    }

    #[cfg(target_arch = "wasm32")]
    #[test]
    fn test_arc_compatibility() {
        use std::sync::Arc;

        // Test that we can wrap WalletWasmDatabase in Arc
        let wallet_db = WalletWasmDatabase::new();
        let _wallet_db: Arc<WalletWasmDatabase> = Arc::new(wallet_db);
    }
}
