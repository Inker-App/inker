//! `cargo run -p inker-ffi --features bindgen --bin uniffi-bindgen-swift -- <lib> <out> --swift-sources --headers --modulemap ...`
//!
//! The Swift-specific front end: it can emit an xcframework-style modulemap
//! and lets us choose the C module name, which the generic CLI cannot.
fn main() {
    uniffi::uniffi_bindgen_swift()
}
