//! Native implementation using cdk-sqlite

// Re-export the cdk-sqlite types for non-WASM targets
pub use cdk_sqlite::{
    MintSqliteDatabase as MintWasmDatabase, WalletSqliteDatabase as WalletWasmDatabase,
};

#[cfg(feature = "auth")]
pub use cdk_sqlite::mint::MintSqliteAuthDatabase as MintWasmAuthDatabase;
