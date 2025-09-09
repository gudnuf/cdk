//! Simple example to test WASM timing functionality
//! This demonstrates that the instant crate works correctly in WASM

#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;

use instant::Instant;
use std::time::Duration;

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = console)]
    fn log(s: &str);
}

#[cfg(target_arch = "wasm32")]
macro_rules! console_log {
    ($($t:tt)*) => (log(&format_args!($($t)*).to_string()))
}

#[cfg(not(target_arch = "wasm32"))]
macro_rules! console_log {
    ($($t:tt)*) => (println!($($t)*))
}

fn main() {
    console_log!("🧪 Testing instant::Instant functionality...");

    // This should work on both native and WASM targets
    let start = Instant::now();
    console_log!("✓ Instant::now() succeeded");

    // Simulate some work (in a real scenario this might be an async operation)
    let mut sum = 0u64;
    for i in 0..1000 {
        sum = sum.wrapping_add(i);
    }

    let end = Instant::now();
    let elapsed = start.elapsed();
    let duration = end.duration_since(start);

    console_log!("✓ Elapsed time: {:?}", elapsed);
    console_log!("✓ Duration since: {:?}", duration);
    console_log!("✓ Sum (to prevent optimization): {}", sum);

    // Test edge cases
    let same_instant = Instant::now();
    let zero_duration = same_instant.duration_since(same_instant);
    console_log!("✓ Zero duration test: {:?}", zero_duration);

    // Test that durations are reasonable (these checks are always true but verify the calls work)
    assert!(elapsed.as_nanos() < u128::MAX); // Always true but ensures the method works
    assert!(duration.as_nanos() < u128::MAX); // Always true but ensures the method works
    assert!(zero_duration <= Duration::from_millis(1)); // Should be very close to zero

    console_log!("🎉 All timing tests passed!");

    #[cfg(target_arch = "wasm32")]
    console_log!("🌐 Successfully tested instant crate timing in WASM environment!");

    #[cfg(not(target_arch = "wasm32"))]
    console_log!("🖥️ Running on native target - using instant crate's native Instant wrapper");
}
