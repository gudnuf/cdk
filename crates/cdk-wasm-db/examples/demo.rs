// Demo showing cdk-wasm-db usage across different targets

use cdk_wasm_db::{MintWasmDatabase, WalletWasmDatabase};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("CDK WASM DB Demo");

    // Initialize (safe on all targets)
    cdk_wasm_db::init().await;
    println!("✓ Initialized WASM DB");

    // Create databases
    #[cfg(target_arch = "wasm32")]
    {
        let wallet_db = WalletWasmDatabase::new_internal(":memory:").await?;
        let mint_db = MintWasmDatabase::new_internal(":memory:").await?;
        println!("✓ Created WASM databases");

        // WASM-specific operations using internal methods
        wallet_db
            .set_internal("test_key".to_string(), "test_value".to_string())
            .await?;
        let value = wallet_db.get_internal("test_key").await?;
        println!("✓ Stored and retrieved: {:?}", value);

        let keys = wallet_db.keys_internal().await?;
        println!("✓ Keys in database: {:?}", keys);
    }

    #[cfg(not(target_arch = "wasm32"))]
    {
        let _wallet_db = WalletWasmDatabase::new(":memory:").await?;
        let _mint_db = MintWasmDatabase::new(":memory:").await?;
        println!("✓ Created native databases");
    }

    // Show target-specific behavior
    #[cfg(target_arch = "wasm32")]
    {
        println!("Running on WASM target with in-memory key-value storage");
        println!("Note: JavaScript API uses Promises for async operations");
    }

    #[cfg(not(target_arch = "wasm32"))]
    {
        println!("Running on native target with full SQLite functionality");
        println!("✓ Full CDK database interface available");

        // Native targets have full cdk-sqlite functionality
        // This demonstrates the crate successfully compiles and links
        println!("✓ Native database operations ready");
    }

    println!("✓ Demo completed successfully!");
    Ok(())
}
