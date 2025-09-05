//! WASM implementation with in-memory storage

use js_sys::Promise;
use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;
use tokio::sync::Mutex;
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::future_to_promise;

/// Simple error type for WASM database operations
#[wasm_bindgen]
#[derive(Debug, Clone)]
pub struct WasmDbError {
    message: String,
}

#[wasm_bindgen]
impl WasmDbError {
    #[wasm_bindgen(constructor)]
    pub fn new(message: String) -> WasmDbError {
        WasmDbError { message }
    }

    #[wasm_bindgen(getter)]
    pub fn message(&self) -> String {
        self.message.clone()
    }

    #[wasm_bindgen(setter)]
    pub fn set_message(&mut self, message: String) {
        self.message = message;
    }
}

impl fmt::Display for WasmDbError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "WASM DB error: {}", self.message)
    }
}

impl std::error::Error for WasmDbError {}

impl From<String> for WasmDbError {
    fn from(message: String) -> Self {
        WasmDbError { message }
    }
}

impl From<&str> for WasmDbError {
    fn from(message: &str) -> Self {
        WasmDbError {
            message: message.to_string(),
        }
    }
}

/// WASM-compatible in-memory mint database
#[wasm_bindgen]
#[derive(Debug, Clone)]
pub struct MintWasmDatabase {
    inner: Arc<Mutex<HashMap<String, String>>>,
}

/// WASM-compatible in-memory wallet database  
#[wasm_bindgen]
#[derive(Debug, Clone)]
pub struct WalletWasmDatabase {
    inner: Arc<Mutex<HashMap<String, String>>>,
}

#[cfg(feature = "auth")]
/// WASM-compatible in-memory mint auth database
pub type MintWasmAuthDatabase = MintWasmDatabase;

#[wasm_bindgen]
impl MintWasmDatabase {
    /// Create a new in-memory mint database
    #[wasm_bindgen(constructor)]
    pub fn new(_path: String) -> Promise {
        future_to_promise(async move {
            let db = Self {
                inner: Arc::new(Mutex::new(HashMap::new())),
            };
            Ok(JsValue::from(db))
        })
    }

    /// Store a key-value pair
    pub fn set(&self, key: String, value: String) -> Promise {
        let inner = self.inner.clone();
        future_to_promise(async move {
            let mut storage = inner.lock().await;
            storage.insert(key, value);
            Ok(JsValue::undefined())
        })
    }

    /// Get a value by key
    pub fn get(&self, key: String) -> Promise {
        let inner = self.inner.clone();
        future_to_promise(async move {
            let storage = inner.lock().await;
            let result = storage.get(&key).cloned();
            match result {
                Some(value) => Ok(JsValue::from_str(&value)),
                None => Ok(JsValue::null()),
            }
        })
    }

    /// Remove a key-value pair
    pub fn remove(&self, key: String) -> Promise {
        let inner = self.inner.clone();
        future_to_promise(async move {
            let mut storage = inner.lock().await;
            storage.remove(&key);
            Ok(JsValue::undefined())
        })
    }

    /// List all keys
    pub fn keys(&self) -> Promise {
        let inner = self.inner.clone();
        future_to_promise(async move {
            let storage = inner.lock().await;
            let keys: Vec<JsValue> = storage.keys().map(|k| JsValue::from_str(k)).collect();
            let js_array = js_sys::Array::new();
            for key in keys {
                js_array.push(&key);
            }
            Ok(JsValue::from(js_array))
        })
    }
}

// Keep the original implementation for internal Rust use
impl MintWasmDatabase {
    /// Create a new in-memory mint database (internal use)
    pub async fn new_internal<T: Into<String>>(_path: T) -> Result<Self, WasmDbError> {
        Ok(Self {
            inner: Arc::new(Mutex::new(HashMap::new())),
        })
    }

    /// Store a key-value pair (internal use)
    pub async fn set_internal(&self, key: String, value: String) -> Result<(), WasmDbError> {
        let mut storage = self.inner.lock().await;
        storage.insert(key, value);
        Ok(())
    }

    /// Get a value by key (internal use)
    pub async fn get_internal(&self, key: &str) -> Result<Option<String>, WasmDbError> {
        let storage = self.inner.lock().await;
        Ok(storage.get(key).cloned())
    }

    /// Remove a key-value pair (internal use)
    pub async fn remove_internal(&self, key: &str) -> Result<(), WasmDbError> {
        let mut storage = self.inner.lock().await;
        storage.remove(key);
        Ok(())
    }

    /// List all keys (internal use)
    pub async fn keys_internal(&self) -> Result<Vec<String>, WasmDbError> {
        let storage = self.inner.lock().await;
        Ok(storage.keys().cloned().collect())
    }
}

#[wasm_bindgen]
impl WalletWasmDatabase {
    /// Create a new in-memory wallet database
    #[wasm_bindgen(constructor)]
    pub fn new(_path: String) -> Promise {
        future_to_promise(async move {
            let db = Self {
                inner: Arc::new(Mutex::new(HashMap::new())),
            };
            Ok(JsValue::from(db))
        })
    }

    /// Store a key-value pair
    pub fn set(&self, key: String, value: String) -> Promise {
        let inner = self.inner.clone();
        future_to_promise(async move {
            let mut storage = inner.lock().await;
            storage.insert(key, value);
            Ok(JsValue::undefined())
        })
    }

    /// Get a value by key
    pub fn get(&self, key: String) -> Promise {
        let inner = self.inner.clone();
        future_to_promise(async move {
            let storage = inner.lock().await;
            let result = storage.get(&key).cloned();
            match result {
                Some(value) => Ok(JsValue::from_str(&value)),
                None => Ok(JsValue::null()),
            }
        })
    }

    /// Remove a key-value pair
    pub fn remove(&self, key: String) -> Promise {
        let inner = self.inner.clone();
        future_to_promise(async move {
            let mut storage = inner.lock().await;
            storage.remove(&key);
            Ok(JsValue::undefined())
        })
    }

    /// List all keys
    pub fn keys(&self) -> Promise {
        let inner = self.inner.clone();
        future_to_promise(async move {
            let storage = inner.lock().await;
            let keys: Vec<JsValue> = storage.keys().map(|k| JsValue::from_str(k)).collect();
            let js_array = js_sys::Array::new();
            for key in keys {
                js_array.push(&key);
            }
            Ok(JsValue::from(js_array))
        })
    }
}

// Keep the original implementation for internal Rust use
impl WalletWasmDatabase {
    /// Create a new in-memory wallet database (internal use)
    pub async fn new_internal<T: Into<String>>(_path: T) -> Result<Self, WasmDbError> {
        Ok(Self {
            inner: Arc::new(Mutex::new(HashMap::new())),
        })
    }

    /// Store a key-value pair (internal use)
    pub async fn set_internal(&self, key: String, value: String) -> Result<(), WasmDbError> {
        let mut storage = self.inner.lock().await;
        storage.insert(key, value);
        Ok(())
    }

    /// Get a value by key (internal use)
    pub async fn get_internal(&self, key: &str) -> Result<Option<String>, WasmDbError> {
        let storage = self.inner.lock().await;
        Ok(storage.get(key).cloned())
    }

    /// Remove a key-value pair (internal use)
    pub async fn remove_internal(&self, key: &str) -> Result<(), WasmDbError> {
        let mut storage = self.inner.lock().await;
        storage.remove(key);
        Ok(())
    }

    /// List all keys (internal use)
    pub async fn keys_internal(&self) -> Result<Vec<String>, WasmDbError> {
        let storage = self.inner.lock().await;
        Ok(storage.keys().cloned().collect())
    }
}
