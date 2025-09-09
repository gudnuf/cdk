//! WASM-only in-memory wallet database implementation

use std::collections::HashMap;
use std::fmt;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use cashu::KeySet;
use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::future_to_promise;

use cdk_common::{
    common::ProofInfo,
    database::{self, WalletDatabase},
    mint_url::MintUrl,
    nuts::{CurrencyUnit, Id, KeySetInfo, Keys, MintInfo, PublicKey, SpendingConditions, State},
    wallet::{
        self, MintQuote as WalletMintQuote, Transaction, TransactionDirection, TransactionId,
    },
};

/// WASM database error type
#[wasm_bindgen]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WasmDbError {
    message: String,
}

#[wasm_bindgen]
impl WasmDbError {
    /// Create a new WASM database error with the given message
    #[wasm_bindgen(constructor)]
    pub fn new(message: String) -> WasmDbError {
        WasmDbError { message }
    }

    /// Get the error message
    #[wasm_bindgen(getter)]
    pub fn message(&self) -> String {
        self.message.clone()
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

impl From<serde_json::Error> for WasmDbError {
    fn from(err: serde_json::Error) -> Self {
        WasmDbError {
            message: format!("JSON error: {}", err),
        }
    }
}

impl From<database::Error> for WasmDbError {
    fn from(err: database::Error) -> Self {
        WasmDbError {
            message: err.to_string(),
        }
    }
}

impl From<WasmDbError> for database::Error {
    fn from(err: WasmDbError) -> Self {
        database::Error::Internal(err.message)
    }
}

/// WASM-only in-memory wallet database
#[wasm_bindgen]
#[derive(Debug, Clone)]
pub struct WalletWasmDatabase {
    storage: Arc<Mutex<HashMap<String, String>>>,
}

#[wasm_bindgen]
impl WalletWasmDatabase {
    /// Create a new in-memory wallet database for WASM
    #[wasm_bindgen(constructor)]
    pub fn new() -> WalletWasmDatabase {
        WalletWasmDatabase {
            storage: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Get a value by key (for WASM JS interop)
    pub fn get(&self, key: String) -> js_sys::Promise {
        let storage = self.storage.clone();
        future_to_promise(async move {
            let result = storage.lock().unwrap().get(&key).cloned();
            match result {
                Some(value) => Ok(JsValue::from_str(&value)),
                None => Ok(JsValue::null()),
            }
        })
    }

    /// Set a value by key (for WASM JS interop)
    pub fn set(&self, key: String, value: String) -> js_sys::Promise {
        let storage = self.storage.clone();
        future_to_promise(async move {
            storage.lock().unwrap().insert(key, value);
            Ok(JsValue::undefined())
        })
    }

    /// Remove a key (for WASM JS interop)
    pub fn remove(&self, key: String) -> js_sys::Promise {
        let storage = self.storage.clone();
        future_to_promise(async move {
            storage.lock().unwrap().remove(&key);
            Ok(JsValue::undefined())
        })
    }
}

// Internal helper methods for Rust use
impl WalletWasmDatabase {
    /// Store a JSON-serializable value
    fn set_json<T: serde::Serialize>(&self, key: &str, value: &T) -> Result<(), WasmDbError> {
        let json_str = serde_json::to_string(value)?;
        self.storage
            .lock()
            .unwrap()
            .insert(key.to_string(), json_str);
        Ok(())
    }

    /// Get and deserialize a JSON value
    fn get_json<T: serde::de::DeserializeOwned>(
        &self,
        key: &str,
    ) -> Result<Option<T>, WasmDbError> {
        match self.storage.lock().unwrap().get(key) {
            Some(json_str) => Ok(Some(serde_json::from_str(json_str)?)),
            None => Ok(None),
        }
    }

    /// Get all values with a key prefix
    fn get_all_with_prefix<T: serde::de::DeserializeOwned>(
        &self,
        prefix: &str,
    ) -> Result<Vec<T>, WasmDbError> {
        let storage = self.storage.lock().unwrap();
        let mut results = Vec::new();
        for (key, value) in storage.iter() {
            if key.starts_with(prefix) {
                let item: T = serde_json::from_str(value)?;
                results.push(item);
            }
        }
        Ok(results)
    }

    /// Remove a key from storage
    fn remove_key(&self, key: &str) {
        self.storage.lock().unwrap().remove(key);
    }
}

// Implement the WalletDatabase trait
#[async_trait(?Send)]
impl WalletDatabase for WalletWasmDatabase {
    type Err = database::Error;

    async fn add_mint(
        &self,
        mint_url: MintUrl,
        mint_info: Option<MintInfo>,
    ) -> Result<(), Self::Err> {
        let key = format!("mint:{}", mint_url);
        self.set_json(&key, &mint_info)
            .map_err(database::Error::from)
    }

    async fn remove_mint(&self, mint_url: MintUrl) -> Result<(), Self::Err> {
        let key = format!("mint:{}", mint_url);
        self.remove_key(&key);
        Ok(())
    }

    async fn get_mint(&self, mint_url: MintUrl) -> Result<Option<MintInfo>, Self::Err> {
        let key = format!("mint:{}", mint_url);
        match self.storage.lock().unwrap().get(&key) {
            Some(json_str) => {
                // Explicitly handle null JSON values
                if json_str.trim() == "null" {
                    Ok(None)
                } else {
                    // Try to deserialize as MintInfo directly
                    let mint_info: MintInfo = serde_json::from_str(json_str)
                        .map_err(|e| database::Error::from(WasmDbError::from(e)))?;
                    Ok(Some(mint_info))
                }
            }
            None => Ok(None),
        }
    }

    async fn get_mints(&self) -> Result<HashMap<MintUrl, Option<MintInfo>>, Self::Err> {
        let storage = self.storage.lock().unwrap();
        let mut results = HashMap::new();

        for (key, value) in storage.iter() {
            if key.starts_with("mint:") {
                if let Some(mint_url_str) = key.strip_prefix("mint:") {
                    if let Ok(mint_url) = mint_url_str.parse::<MintUrl>() {
                        // Explicitly handle null JSON values
                        let mint_info = if value.trim() == "null" {
                            None
                        } else {
                            let info: MintInfo = serde_json::from_str(value)
                                .map_err(|e| database::Error::from(WasmDbError::from(e)))?;
                            Some(info)
                        };
                        results.insert(mint_url, mint_info);
                    }
                }
            }
        }
        Ok(results)
    }

    async fn update_mint_url(
        &self,
        old_mint_url: MintUrl,
        new_mint_url: MintUrl,
    ) -> Result<(), Self::Err> {
        let mint_info = self.get_mint(old_mint_url.clone()).await?;
        self.add_mint(new_mint_url, mint_info).await?;
        self.remove_mint(old_mint_url).await?;
        Ok(())
    }

    async fn add_mint_keysets(
        &self,
        mint_url: MintUrl,
        keysets: Vec<KeySetInfo>,
    ) -> Result<(), Self::Err> {
        let key = format!("keysets:{}", mint_url);
        self.set_json(&key, &keysets).map_err(database::Error::from)
    }

    async fn get_mint_keysets(
        &self,
        mint_url: MintUrl,
    ) -> Result<Option<Vec<KeySetInfo>>, Self::Err> {
        let key = format!("keysets:{}", mint_url);
        self.get_json(&key).map_err(database::Error::from)
    }

    async fn get_keyset_by_id(&self, keyset_id: &Id) -> Result<Option<KeySetInfo>, Self::Err> {
        let storage = self.storage.lock().unwrap();
        for (key, value) in storage.iter() {
            if key.starts_with("keysets:") {
                let keysets: Vec<KeySetInfo> = serde_json::from_str(value)
                    .map_err(|e| database::Error::from(WasmDbError::from(e)))?;
                for keyset in keysets {
                    if keyset.id == *keyset_id {
                        return Ok(Some(keyset));
                    }
                }
            }
        }
        Ok(None)
    }

    async fn add_mint_quote(&self, quote: WalletMintQuote) -> Result<(), Self::Err> {
        let key = format!("mint_quote:{}", quote.id);
        self.set_json(&key, &quote).map_err(database::Error::from)
    }

    async fn get_mint_quote(&self, quote_id: &str) -> Result<Option<WalletMintQuote>, Self::Err> {
        let key = format!("mint_quote:{}", quote_id);
        self.get_json(&key).map_err(database::Error::from)
    }

    async fn get_mint_quotes(&self) -> Result<Vec<WalletMintQuote>, Self::Err> {
        self.get_all_with_prefix("mint_quote:")
            .map_err(database::Error::from)
    }

    async fn remove_mint_quote(&self, quote_id: &str) -> Result<(), Self::Err> {
        let key = format!("mint_quote:{}", quote_id);
        self.remove_key(&key);
        Ok(())
    }

    async fn add_melt_quote(&self, quote: wallet::MeltQuote) -> Result<(), Self::Err> {
        let key = format!("melt_quote:{}", quote.id);
        self.set_json(&key, &quote).map_err(database::Error::from)
    }

    async fn get_melt_quote(&self, quote_id: &str) -> Result<Option<wallet::MeltQuote>, Self::Err> {
        let key = format!("melt_quote:{}", quote_id);
        self.get_json(&key).map_err(database::Error::from)
    }

    async fn get_melt_quotes(&self) -> Result<Vec<wallet::MeltQuote>, Self::Err> {
        self.get_all_with_prefix("melt_quote:")
            .map_err(database::Error::from)
    }

    async fn remove_melt_quote(&self, quote_id: &str) -> Result<(), Self::Err> {
        let key = format!("melt_quote:{}", quote_id);
        self.remove_key(&key);
        Ok(())
    }

    async fn add_keys(&self, keyset: KeySet) -> Result<(), Self::Err> {
        let key = format!("keys:{}", keyset.id);
        self.set_json(&key, &keyset).map_err(database::Error::from)
    }

    async fn get_keys(&self, id: &Id) -> Result<Option<Keys>, Self::Err> {
        let key = format!("keys:{}", id);
        if let Some(keyset) = self
            .get_json::<KeySet>(&key)
            .map_err(database::Error::from)?
        {
            Ok(Some(keyset.keys))
        } else {
            Ok(None)
        }
    }

    async fn remove_keys(&self, id: &Id) -> Result<(), Self::Err> {
        let key = format!("keys:{}", id);
        self.remove_key(&key);
        Ok(())
    }

    async fn update_proofs(
        &self,
        added: Vec<ProofInfo>,
        removed_ys: Vec<PublicKey>,
    ) -> Result<(), Self::Err> {
        let mut storage = self.storage.lock().unwrap();

        // Add new proofs
        for proof_info in added {
            let key = format!("proof:{}", proof_info.y);
            let json_str = serde_json::to_string(&proof_info)
                .map_err(|e| database::Error::from(WasmDbError::from(e)))?;
            storage.insert(key, json_str);
        }

        // Remove proofs by Y value
        for y in removed_ys {
            let key = format!("proof:{}", y);
            storage.remove(&key);
        }

        Ok(())
    }

    async fn get_proofs(
        &self,
        mint_url: Option<MintUrl>,
        unit: Option<CurrencyUnit>,
        state: Option<Vec<State>>,
        spending_conditions: Option<Vec<SpendingConditions>>,
    ) -> Result<Vec<ProofInfo>, Self::Err> {
        let storage = self.storage.lock().unwrap();
        let mut results = Vec::new();

        for (key, value) in storage.iter() {
            if key.starts_with("proof:") {
                let proof_info: ProofInfo = serde_json::from_str(value)
                    .map_err(|e| database::Error::from(WasmDbError::from(e)))?;

                // Apply filters
                if let Some(ref filter_mint_url) = mint_url {
                    if &proof_info.mint_url != filter_mint_url {
                        continue;
                    }
                }

                if let Some(ref filter_unit) = unit {
                    if &proof_info.unit != filter_unit {
                        continue;
                    }
                }

                if let Some(ref filter_states) = state {
                    if !filter_states.contains(&proof_info.state) {
                        continue;
                    }
                }

                if let Some(ref filter_conditions) = spending_conditions {
                    if let Some(ref proof_conditions) = proof_info.spending_condition {
                        if !filter_conditions.contains(proof_conditions) {
                            continue;
                        }
                    } else if !filter_conditions.is_empty() {
                        continue;
                    }
                }

                results.push(proof_info);
            }
        }

        Ok(results)
    }

    async fn update_proofs_state(&self, ys: Vec<PublicKey>, state: State) -> Result<(), Self::Err> {
        let mut storage = self.storage.lock().unwrap();

        for y in ys {
            let key = format!("proof:{}", y);
            if let Some(value) = storage.get(&key).cloned() {
                let mut proof_info: ProofInfo = serde_json::from_str(&value)
                    .map_err(|e| database::Error::from(WasmDbError::from(e)))?;
                proof_info.state = state.clone();
                let json_str = serde_json::to_string(&proof_info)
                    .map_err(|e| database::Error::from(WasmDbError::from(e)))?;
                storage.insert(key, json_str);
            }
        }

        Ok(())
    }

    async fn increment_keyset_counter(&self, keyset_id: &Id, count: u32) -> Result<u32, Self::Err> {
        let key = format!("counter:{}", keyset_id);
        let mut storage = self.storage.lock().unwrap();

        let current_count: u32 = storage.get(&key).and_then(|v| v.parse().ok()).unwrap_or(0);

        let new_count = current_count + count;
        storage.insert(key, new_count.to_string());

        Ok(new_count)
    }

    async fn add_transaction(&self, transaction: Transaction) -> Result<(), Self::Err> {
        let key = format!("transaction:{}", transaction.id());
        self.set_json(&key, &transaction)
            .map_err(database::Error::from)
    }

    async fn get_transaction(
        &self,
        transaction_id: TransactionId,
    ) -> Result<Option<Transaction>, Self::Err> {
        let key = format!("transaction:{}", transaction_id);
        self.get_json(&key).map_err(database::Error::from)
    }

    async fn list_transactions(
        &self,
        mint_url: Option<MintUrl>,
        direction: Option<TransactionDirection>,
        unit: Option<CurrencyUnit>,
    ) -> Result<Vec<Transaction>, Self::Err> {
        let storage = self.storage.lock().unwrap();
        let mut results = Vec::new();

        for (key, value) in storage.iter() {
            if key.starts_with("transaction:") {
                let transaction: Transaction = serde_json::from_str(value)
                    .map_err(|e| database::Error::from(WasmDbError::from(e)))?;

                // Apply filters
                if let Some(ref filter_mint_url) = mint_url {
                    if transaction.mint_url != *filter_mint_url {
                        continue;
                    }
                }

                if let Some(ref filter_direction) = direction {
                    if transaction.direction != *filter_direction {
                        continue;
                    }
                }

                if let Some(ref filter_unit) = unit {
                    if transaction.unit != *filter_unit {
                        continue;
                    }
                }

                results.push(transaction);
            }
        }

        // Sort by timestamp (newest first)
        results.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));

        Ok(results)
    }

    async fn remove_transaction(&self, transaction_id: TransactionId) -> Result<(), Self::Err> {
        let key = format!("transaction:{}", transaction_id);
        self.remove_key(&key);
        Ok(())
    }
}
