//! Addressing a store's collection by position re-runs every later sibling's observers and reads
//! past the end when the collection shrinks. Keyed addressing is the published alternative.

use zgui_reactive::StoreFieldIterator;

fn main() {}
