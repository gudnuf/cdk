//! WASM SQLite Mint

use cdk_common::database::Error;
use cdk_sql_common::mint::SQLMintAuthDatabase;
use cdk_sql_common::pool::Pool;
use cdk_sql_common::SQLMintDatabase;

use crate::common::{Config, WasmSqliteConnectionManager};

pub mod memory;

/// Mint WASM SQLite implementation
pub type MintWasmDatabase = SQLMintDatabase<WasmSqliteConnectionManager>;

/// Mint Auth database with WASM SQLite
#[cfg(feature = "auth")]
pub type MintWasmAuthDatabase = SQLMintAuthDatabase<WasmSqliteConnectionManager>;

/// Creates a new MintWasmDatabase instance
pub async fn new_mint_wasm_database<X>(db: X) -> Result<MintWasmDatabase, Error>
where
    X: Into<String>,
{
    // Initialize WASM SQLite first
    crate::init().await;
    
    let config: Config = db.into().into();
    let pool = Pool::new(config);
    
    // Create database using SQL common's new method
    use std::sync::Arc;
    Ok(SQLMintDatabase {
        pool: Arc::new(pool),
    })
}

#[cfg(feature = "auth")]
/// Creates a new MintWasmAuthDatabase instance
pub async fn new_mint_wasm_auth_database<X>(db: X) -> Result<MintWasmAuthDatabase, Error>
where
    X: Into<String>,
{
    // Initialize WASM SQLite first
    crate::init().await;
    
    let config: Config = db.into().into();
    let pool = Pool::new(config);
    
    // Create database using SQL common's new method
    use std::sync::Arc;
    Ok(SQLMintAuthDatabase {
        pool: Arc::new(pool),
    })
}

#[cfg(test)]
mod test {
    use cdk_common::mint_db_test;

    use super::*;

    async fn provide_db() -> MintWasmDatabase {
        // Initialize WASM SQLite first
        crate::init().await;
        memory::empty().await.unwrap()
    }

    mint_db_test!(provide_db);
}
