//! Native stub implementation for cdk-wasm-db
//!
//! This module provides stub implementations that compile on native targets
//! but are not functional. The actual WASM database should only be used
//! in WebAssembly environments.

use std::collections::HashMap;
use std::fmt;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use cdk_common::{
    common::ProofInfo,
    database::{self, WalletDatabase},
    mint_url::MintUrl,
    nuts::{
        CurrencyUnit, Id, KeySet, KeySetInfo, Keys, MintInfo, PublicKey, SpendingConditions, State,
    },
    wallet::{
        self, MintQuote as WalletMintQuote, Transaction, TransactionDirection, TransactionId,
    },
};

/// Native stub database error type
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WasmDbError {
    message: String,
}

impl WasmDbError {
    /// Create a new WASM database error with the given message
    pub fn new(message: String) -> WasmDbError {
        WasmDbError { message }
    }

    /// Get the error message
    pub fn message(&self) -> String {
        self.message.clone()
    }
}

impl fmt::Display for WasmDbError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "WASM DB error (native stub): {}", self.message)
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

/// Native stub for WASM wallet database
///
/// This is a non-functional stub that allows compilation on native targets.
/// All methods return errors indicating that this should only be used in WASM.
#[derive(Debug, Clone)]
pub struct WalletWasmDatabase;

impl WalletWasmDatabase {
    /// Create a new stub wallet database for native compilation
    ///
    /// Note: This will not actually work and is only provided for compilation.
    pub fn new() -> WalletWasmDatabase {
        WalletWasmDatabase
    }
}

impl Default for WalletWasmDatabase {
    fn default() -> Self {
        Self::new()
    }
}

// Implement the WalletDatabase trait with stub methods that return errors
#[async_trait]
impl WalletDatabase for WalletWasmDatabase {
    type Err = database::Error;

    async fn add_mint(
        &self,
        _mint_url: MintUrl,
        _mint_info: Option<MintInfo>,
    ) -> Result<(), Self::Err> {
        Err(database::Error::Internal(
            "WalletWasmDatabase is only supported in WebAssembly environments".to_string(),
        ))
    }

    async fn remove_mint(&self, _mint_url: MintUrl) -> Result<(), Self::Err> {
        Err(database::Error::Internal(
            "WalletWasmDatabase is only supported in WebAssembly environments".to_string(),
        ))
    }

    async fn get_mint(&self, _mint_url: MintUrl) -> Result<Option<MintInfo>, Self::Err> {
        Err(database::Error::Internal(
            "WalletWasmDatabase is only supported in WebAssembly environments".to_string(),
        ))
    }

    async fn get_mints(&self) -> Result<HashMap<MintUrl, Option<MintInfo>>, Self::Err> {
        Err(database::Error::Internal(
            "WalletWasmDatabase is only supported in WebAssembly environments".to_string(),
        ))
    }

    async fn update_mint_url(
        &self,
        _old_mint_url: MintUrl,
        _new_mint_url: MintUrl,
    ) -> Result<(), Self::Err> {
        Err(database::Error::Internal(
            "WalletWasmDatabase is only supported in WebAssembly environments".to_string(),
        ))
    }

    async fn add_mint_keysets(
        &self,
        _mint_url: MintUrl,
        _keysets: Vec<KeySetInfo>,
    ) -> Result<(), Self::Err> {
        Err(database::Error::Internal(
            "WalletWasmDatabase is only supported in WebAssembly environments".to_string(),
        ))
    }

    async fn get_mint_keysets(
        &self,
        _mint_url: MintUrl,
    ) -> Result<Option<Vec<KeySetInfo>>, Self::Err> {
        Err(database::Error::Internal(
            "WalletWasmDatabase is only supported in WebAssembly environments".to_string(),
        ))
    }

    async fn get_keyset_by_id(&self, _keyset_id: &Id) -> Result<Option<KeySetInfo>, Self::Err> {
        Err(database::Error::Internal(
            "WalletWasmDatabase is only supported in WebAssembly environments".to_string(),
        ))
    }

    async fn add_mint_quote(&self, _quote: WalletMintQuote) -> Result<(), Self::Err> {
        Err(database::Error::Internal(
            "WalletWasmDatabase is only supported in WebAssembly environments".to_string(),
        ))
    }

    async fn get_mint_quote(&self, _quote_id: &str) -> Result<Option<WalletMintQuote>, Self::Err> {
        Err(database::Error::Internal(
            "WalletWasmDatabase is only supported in WebAssembly environments".to_string(),
        ))
    }

    async fn get_mint_quotes(&self) -> Result<Vec<WalletMintQuote>, Self::Err> {
        Err(database::Error::Internal(
            "WalletWasmDatabase is only supported in WebAssembly environments".to_string(),
        ))
    }

    async fn remove_mint_quote(&self, _quote_id: &str) -> Result<(), Self::Err> {
        Err(database::Error::Internal(
            "WalletWasmDatabase is only supported in WebAssembly environments".to_string(),
        ))
    }

    async fn add_melt_quote(&self, _quote: wallet::MeltQuote) -> Result<(), Self::Err> {
        Err(database::Error::Internal(
            "WalletWasmDatabase is only supported in WebAssembly environments".to_string(),
        ))
    }

    async fn get_melt_quote(
        &self,
        _quote_id: &str,
    ) -> Result<Option<wallet::MeltQuote>, Self::Err> {
        Err(database::Error::Internal(
            "WalletWasmDatabase is only supported in WebAssembly environments".to_string(),
        ))
    }

    async fn get_melt_quotes(&self) -> Result<Vec<wallet::MeltQuote>, Self::Err> {
        Err(database::Error::Internal(
            "WalletWasmDatabase is only supported in WebAssembly environments".to_string(),
        ))
    }

    async fn remove_melt_quote(&self, _quote_id: &str) -> Result<(), Self::Err> {
        Err(database::Error::Internal(
            "WalletWasmDatabase is only supported in WebAssembly environments".to_string(),
        ))
    }

    async fn add_keys(&self, _keyset: KeySet) -> Result<(), Self::Err> {
        Err(database::Error::Internal(
            "WalletWasmDatabase is only supported in WebAssembly environments".to_string(),
        ))
    }

    async fn get_keys(&self, _id: &Id) -> Result<Option<Keys>, Self::Err> {
        Err(database::Error::Internal(
            "WalletWasmDatabase is only supported in WebAssembly environments".to_string(),
        ))
    }

    async fn remove_keys(&self, _id: &Id) -> Result<(), Self::Err> {
        Err(database::Error::Internal(
            "WalletWasmDatabase is only supported in WebAssembly environments".to_string(),
        ))
    }

    async fn update_proofs(
        &self,
        _added: Vec<ProofInfo>,
        _removed_ys: Vec<PublicKey>,
    ) -> Result<(), Self::Err> {
        Err(database::Error::Internal(
            "WalletWasmDatabase is only supported in WebAssembly environments".to_string(),
        ))
    }

    async fn get_proofs(
        &self,
        _mint_url: Option<MintUrl>,
        _unit: Option<CurrencyUnit>,
        _state: Option<Vec<State>>,
        _spending_conditions: Option<Vec<SpendingConditions>>,
    ) -> Result<Vec<ProofInfo>, Self::Err> {
        Err(database::Error::Internal(
            "WalletWasmDatabase is only supported in WebAssembly environments".to_string(),
        ))
    }

    async fn update_proofs_state(
        &self,
        _ys: Vec<PublicKey>,
        _state: State,
    ) -> Result<(), Self::Err> {
        Err(database::Error::Internal(
            "WalletWasmDatabase is only supported in WebAssembly environments".to_string(),
        ))
    }

    async fn increment_keyset_counter(
        &self,
        _keyset_id: &Id,
        _count: u32,
    ) -> Result<u32, Self::Err> {
        Err(database::Error::Internal(
            "WalletWasmDatabase is only supported in WebAssembly environments".to_string(),
        ))
    }

    async fn add_transaction(&self, _transaction: Transaction) -> Result<(), Self::Err> {
        Err(database::Error::Internal(
            "WalletWasmDatabase is only supported in WebAssembly environments".to_string(),
        ))
    }

    async fn get_transaction(
        &self,
        _transaction_id: TransactionId,
    ) -> Result<Option<Transaction>, Self::Err> {
        Err(database::Error::Internal(
            "WalletWasmDatabase is only supported in WebAssembly environments".to_string(),
        ))
    }

    async fn list_transactions(
        &self,
        _mint_url: Option<MintUrl>,
        _direction: Option<TransactionDirection>,
        _unit: Option<CurrencyUnit>,
    ) -> Result<Vec<Transaction>, Self::Err> {
        Err(database::Error::Internal(
            "WalletWasmDatabase is only supported in WebAssembly environments".to_string(),
        ))
    }

    async fn remove_transaction(&self, _transaction_id: TransactionId) -> Result<(), Self::Err> {
        Err(database::Error::Internal(
            "WalletWasmDatabase is only supported in WebAssembly environments".to_string(),
        ))
    }
}
