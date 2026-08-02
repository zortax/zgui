//! A real answer to "which state bits can matter for this element", read off a rule set.
//!
//! The shipped answer comes from the crate that owns the compiled rule set. Here it comes from a
//! second rule set built from the same stylesheet text, which is the same computation with the
//! ownership rearranged so that a test can hold one.
//!
//! The lookup is bucketed by the element's root-ness, its identifier, each of its classes and its
//! local name — which is precisely why the answer stops being an answer when any of those change.

use selectors::matching::QuirksMode;
use style::shared_lock::{SharedRwLock, StylesheetGuards};
use style::stylesheets::Origin;
use style::stylist::Stylist;
use stylo_dom::ElementState;
use zgui_dom::{Node, StyleFilter};

use crate::support::sheets::{self, Errors};
use crate::support::{device, prefs};

/// A filter answering from a rule set built out of stylesheet text.
pub(crate) struct SheetFilter {
    /// The rule set the answers come from.
    stylist: Stylist,
}

impl SheetFilter {
    /// A filter over the author sheets in `css`.
    pub(crate) fn new(css: &[&str]) -> Self {
        prefs::enable_css_features();
        let lock = SharedRwLock::new();
        let url = sheets::base_url();
        let errors = Errors::new();
        let mut stylist = Stylist::new(device::device(1280.0, 800.0, 1.0), QuirksMode::NoQuirks);
        for text in css {
            let sheet = sheets::parse(text, Origin::Author, &lock, &url, &errors);
            let guard = lock.read();
            stylist.append_stylesheet(sheet, &guard);
        }
        {
            let read = lock.read();
            stylist.flush(&StylesheetGuards {
                author: &read,
                ua_or_user: &read,
            });
        }
        Self { stylist }
    }
}

impl StyleFilter for SheetFilter {
    fn states_for(&self, element: Node<'_>) -> ElementState {
        let mut states = ElementState::empty();
        for (data, _origin) in self.stylist.iter_origins() {
            data.invalidation_map().state_affecting_selectors.lookup(
                element,
                QuirksMode::NoQuirks,
                None,
                |dependency| {
                    states |= dependency.state;
                    true
                },
            );
        }
        states
    }

    fn is_disabled(&self) -> bool {
        false
    }
}
