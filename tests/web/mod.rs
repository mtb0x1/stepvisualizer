//! Main integration test runner for stepvisualizer in WebAssembly.
use wasm_bindgen_test::*;

wasm_bindgen_test_configure!(run_in_browser);

mod integrations;
mod units;
