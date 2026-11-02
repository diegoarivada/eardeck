//! Feeds hostile Bluetooth packets to the parser. Anyone nearby can broadcast these, so the
//! parser is the app's main attack surface. Run with: cargo +nightly fuzz run parser
#![no_main]

use libfuzzer_sys::fuzz_target;

#[path = "../../src/parser.rs"]
#[allow(dead_code)]
mod parser;

fuzz_target!(|data: &[u8]| {
    if let Some(r) = parser::parse(data) {
        for level in [r.left, r.right, r.case, r.main].into_iter().flatten() {
            assert!(level <= 100);
        }
    }
});
