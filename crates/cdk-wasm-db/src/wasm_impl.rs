//! WASM implementation with in-memory storage

use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;
use tokio::sync::Mutex;

/// Simple error type for WASM database operations
#[derive(Debug, Clone)]
pub struct WasmDbError {
    /// Error message
    pub message: String,
}

impl fmt::Display for WasmDbError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "WASM DB error: {}", self.message)
    }
}

impl std::error::Error for WasmDbError {}

/// WASM-compatible in-memory mint database
#[derive(Debug, Clone)]
pub struct MintWasmDatabase {
    inner: Arc<Mutex<HashMap<String, String>>>,
}

/// WASM-compatible in-memory wallet database  
#[derive(Debug, Clone)]
pub struct WalletWasmDatabase {
    inner: Arc<Mutex<HashMap<String, String>>>,
}

#[cfg(feature = "auth")]
/// WASM-compatible in-memory mint auth database
pub type MintWasmAuthDatabase = MintWasmDatabase;

impl MintWasmDatabase {
    /// Create a new in-memory mint database
    pub async fn new<T: Into<String>>(_path: T) -> Result<Self, WasmDbError> {
        Ok(Self {
            inner: Arc::new(Mutex::new(HashMap::new())),
        })
    }

    /// Store a key-value pair
    pub async fn set(&self, key: String, value: String) -> Result<(), WasmDbError> {
        let mut storage = self.inner.lock().await;
        storage.insert(key, value);
        Ok(())
    }

    /// Get a value by key
    pub async fn get(&self, key: &str) -> Result<Option<String>, WasmDbError> {
        let storage = self.inner.lock().await;
        Ok(storage.get(key).cloned())
    }

    /// Remove a key-value pair
    pub async fn remove(&self, key: &str) -> Result<(), WasmDbError> {
        let mut storage = self.inner.lock().await;
        storage.remove(key);
        Ok(())
    }

    /// List all keys
    pub async fn keys(&self) -> Result<Vec<String>, WasmDbError> {
        let storage = self.inner.lock().await;
        Ok(storage.keys().cloned().collect())
    }
}

impl WalletWasmDatabase {
    /// Create a new in-memory wallet database
    pub async fn new<T: Into<String>>(_path: T) -> Result<Self, WasmDbError> {
        Ok(Self {
            inner: Arc::new(Mutex::new(HashMap::new())),
        })
    }

    /// Store a key-value pair
    pub async fn set(&self, key: String, value: String) -> Result<(), WasmDbError> {
        let mut storage = self.inner.lock().await;
        storage.insert(key, value);
        Ok(())
    }

    /// Get a value by key
    pub async fn get(&self, key: &str) -> Result<Option<String>, WasmDbError> {
        let storage = self.inner.lock().await;
        Ok(storage.get(key).cloned())
    }

    /// Remove a key-value pair
    pub async fn remove(&self, key: &str) -> Result<(), WasmDbError> {
        let mut storage = self.inner.lock().await;
        storage.remove(key);
        Ok(())
    }

    /// List all keys
    pub async fn keys(&self) -> Result<Vec<String>, WasmDbError> {
        let storage = self.inner.lock().await;
        Ok(storage.keys().cloned().collect())
    }
}
