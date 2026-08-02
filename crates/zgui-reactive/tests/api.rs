//! The APIs this crate deliberately does not publish.
//!
//! Each case here is an API of the underlying reactive engine that compiles, runs and then
//! behaves wrongly in a way that is hard to attribute: it over-invalidates, it panics on a
//! shrinking collection, it runs an effect off the UI thread, or it demands a closure that
//! cannot capture a view. Leaving them unreachable is the whole reason this crate wraps the
//! engine rather than re-exporting it, so the absence is asserted rather than assumed.

/// Each of these programs must fail to compile.
#[test]
fn the_unpublished_apis_are_unreachable() {
    let cases = trybuild::TestCases::new();
    cases.compile_fail("tests/ui/*.rs");
}
