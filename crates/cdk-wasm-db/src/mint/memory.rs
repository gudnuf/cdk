//! In-memory WASM SQLite mint database

use cdk_common::database::Error;
use cdk_sql_common::pool::Pool;

use crate::common::{Config, WasmSqliteConnectionManager};

use super::MintWasmDatabase;

/// Create an empty in-memory mint database
pub async fn empty() -> Result<MintWasmDatabase, Error> {
    // Initialize WASM SQLite first
    crate::init().await;
    
    let config: Config = ":memory:".into();
    let pool = Pool::<WasmSqliteConnectionManager>::new(config);
    use std::sync::Arc;
    Ok(super::SQLMintDatabase {
        pool: Arc::new(pool),
    })
}
