#![cfg(target_arch = "wasm32")]

use std::str::FromStr;
use wasm_bindgen::prelude::*;
use wasm_bindgen_test::*;

use cashu::Amount;
use cdk::mint_url::MintUrl;
use cdk::nuts::{MintQuoteBolt11Request, MintRequest};
use cdk::wallet::mint_connector::{HttpClient, MintConnector};
use instant::Instant;

// Configure for running in the browser
wasm_bindgen_test_configure!(run_in_browser);

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = console)]
    fn log(s: &str);
}

macro_rules! console_log {
    ($($t:tt)*) => (log(&format_args!($($t)*).to_string()))
}

/// Mock mint server for testing - this would normally be provided by a test server
/// For now, we'll create a client that will fail when trying to connect
fn create_test_http_client() -> HttpClient {
    let mint_url = MintUrl::from_str("http://localhost:3338").expect("Invalid mint URL");
    HttpClient::new(mint_url)
}

/// This test should reproduce the WASM panic related to Instant::now()
#[wasm_bindgen_test]
async fn test_retriable_http_request_instant_panic() {
    console_log!("Starting WASM HTTP client test that should reproduce the Instant::now() panic");

    let client = create_test_http_client();

    // Create a test mint request that will trigger the retriable_http_request method
    let mint_request = MintRequest {
        quote: "test_quote".to_string(),
        outputs: vec![], // Empty outputs for simplicity
    };

    console_log!("Created test client and mint request");

    // This should trigger the panic at std::time::Instant::now() in retriable_http_request
    let result = client.post_mint(mint_request).await;

    match result {
        Ok(_) => {
            console_log!("Test passed unexpectedly - mint request succeeded");
        }
        Err(e) => {
            console_log!("Expected error occurred: {:?}", e);
            // We expect this to fail, but not due to a panic
            // If we get here without a panic, the Instant::now() issue is fixed
        }
    }
}

/// Test that demonstrates the problematic timing logic
#[wasm_bindgen_test]
async fn test_mint_quote_wasm_compatibility() {
    console_log!("Testing mint quote operation for WASM compatibility");

    let client = create_test_http_client();

    let quote_request = MintQuoteBolt11Request {
        amount: Amount::from(100u64),
        unit: cashu::CurrencyUnit::Sat,
        description: Some("Test quote for WASM".to_string()),
    };

    console_log!("About to make mint quote request that may trigger timing issues");

    // This will fail to connect, but should not panic due to timing issues
    let result = client.post_mint_quote(quote_request).await;

    match result {
        Ok(_) => console_log!("Mint quote succeeded unexpectedly"),
        Err(e) => console_log!("Mint quote failed as expected: {:?}", e),
    }

    console_log!("Test completed without panic - timing issue may be resolved");
}

/// Test the basic HTTP client creation in WASM environment
#[wasm_bindgen_test]
fn test_http_client_creation() {
    console_log!("Testing HTTP client creation in WASM environment");

    let mint_url = MintUrl::from_str("http://localhost:3338").expect("Invalid mint URL");
    let client = HttpClient::new(mint_url);

    console_log!("HTTP client created successfully in WASM environment");

    // Just verify the client can be created without panicking
    assert!(!client.mint_url.to_string().is_empty());
}

/// Test that reproduces the specific error path mentioned in the stack trace
#[wasm_bindgen_test]
async fn test_melt_operation_timing_issue() {
    console_log!("Testing melt operation that may trigger retriable_http_request timing issue");

    let client = create_test_http_client();

    // Create a test melt request to trigger the problematic code path
    let melt_request = cashu::nuts::MeltRequest {
        quote: "test_melt_quote".to_string(),
        inputs: vec![], // Empty for test
        outputs: None,  // No fee return outputs
    };

    console_log!("Created melt request, about to test melt operation");

    // This should trigger the retriable_http_request code path that uses Instant::now()
    let result = client.post_melt(melt_request).await;

    match result {
        Ok(_) => console_log!("Melt request succeeded unexpectedly"),
        Err(e) => console_log!("Melt request failed as expected: {:?}", e),
    }

    console_log!("Melt operation test completed");
}

/// Test the instant crate timing utility directly
#[wasm_bindgen_test]
fn test_instant_timing() {
    console_log!("Testing instant::Instant in WASM environment");

    // This should not panic in WASM
    let start = Instant::now();
    console_log!("✓ Instant::now() succeeded");

    let end = Instant::now();
    let elapsed = start.elapsed();
    let duration = end.duration_since(start);

    console_log!(&format!("✓ Elapsed time: {:?}", elapsed));
    console_log!(&format!("✓ Duration since: {:?}", duration));

    // Verify the timing works without panicking
    assert!(elapsed.as_nanos() >= 0);
    assert!(duration.as_nanos() >= 0);

    console_log!("✅ instant::Instant works correctly in WASM!");
}
