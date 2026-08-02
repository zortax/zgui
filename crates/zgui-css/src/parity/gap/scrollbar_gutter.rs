//! The gutter a locked scroll container is supposed to keep, which this build cannot ask for.
//!
//! Opening a modal usually means stopping the window scrolling behind it, and the obvious way to
//! do that — `overflow: hidden` on the root — takes the scrollbar away with it. The gutter it
//! occupied is given back to the content, every line re-wraps a few pixels wider, and the whole
//! page jumps sideways behind the modal on the frame it opens.
//!
//! CSS has an answer and this build does not generate it: `scrollbar-gutter` is defined in the
//! engine's sources for another target only, so the parser does not know the name and a
//! declaration using it is dropped without a word.
//!
//! Layout has a mechanism that would serve — a container can be *locked*, and keeps whatever
//! gutter it was reserving — but nothing above layout can reach it: no view-layer seam exposes
//! it, so a component that locks the window does so by installing `overflow: hidden` and wears
//! the reflow.

use crate::parity::support::{AbsentReason, Support};

crate::register_properties! {
    scrollbar_gutter => Support::Absent(AbsentReason::GeckoOnly),
}

#[cfg(test)]
mod tests {
    use super::REGISTERED;

    #[test]
    fn the_row_is_the_one_property_that_would_reserve_the_gutter() {
        let names: Vec<String> = REGISTERED.iter().map(|row| row.css_name()).collect();
        assert_eq!(names, ["scrollbar-gutter"]);
    }
}
