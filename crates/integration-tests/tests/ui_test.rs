//! Trybuild tests for compile-time diagnostics.
//!
//! This is a separate binary to make it easy for only the trybuild CI job to
//! run it.

#[test]
fn ui() {
    let t = trybuild::TestCases::new();
    t.compile_fail("tests/ui/*.rs");
}
