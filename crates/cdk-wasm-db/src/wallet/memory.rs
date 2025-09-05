//! In-memory WASM SQLite wallet database

use cdk_common::database::Error;
use cdk_sql_common::pool::Pool;

use crate::common::{Config, WasmSqliteConnectionManager};

use super::WalletWasmDatabase;

/// Create an empty in-memory wallet database
pub async fn empty() -> Result<WalletWasmDatabase, Error> {
    // Initialize WASM SQLite first
    crate::init().await;
    
    let config: Config = ":memory:".into();
    let pool = Pool::<WasmSqliteConnectionManager>::new(config);
    use std::sync::Arc;
    Ok(super::SQLWalletDatabase {
        pool: Arc::new(pool),
    })
}
