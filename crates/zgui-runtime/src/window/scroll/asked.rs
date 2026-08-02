//! How far a device's scroll asked to go, once this desktop and this document have both answered.
//!
//! A scroll event carries a number and a unit, and neither is a distance. Turning the pair into one
//! takes three answers from three different owners, and the whole reason this is a module rather
//! than a multiplication is that putting any of the three in the wrong place produces a wheel that
//! is wrong somewhere and never obviously broken anywhere.
//!
//! | Question | Whose answer |
//! |---|---|
//! | how many lines does one detent of a wheel mean | the desktop's, through [`ScrollSettings`] |
//! | has the person's direction preference been applied yet | the backend's, through [`ScrollSettings`] |
//! | how tall is a line, and how far is a page | the scrolled container's own computed style |
//!
//! The first two arrive together from the platform seam. The third cannot: it is a property of
//! whichever element the wheel turned out to be over, which is not known until the event has been
//! routed — so it is read here, after that, rather than guessed at anywhere earlier.

use zgui_geom::CssPx;
use zgui_layout::scroll_region::region_of_element;
use zgui_platform::ScrollSettings;
use zgui_vocab::ScrollDelta;

use crate::window::Window;

impl Window {
    /// The delta the person asked for, in this framework's units and this desktop's convention.
    ///
    /// Two things happen to it. The count of lines a detent means is applied — the desktop's
    /// answer, not a constant here — and the direction preference is applied if this backend says
    /// it has not been applied already. On every ordinary desktop it has been, by the input stack,
    /// long before the event reached any window; a framework that flipped it again would override
    /// that setting for every program built with it.
    pub(super) fn asked_for(&self, delta: ScrollDelta) -> ScrollDelta {
        let settings = self.scroll_settings();
        match delta {
            ScrollDelta::Lines { x, y } => {
                let (x, y) = settings.direction.apply_lines(x, y);
                ScrollDelta::Lines {
                    x: settings.lines_for(x),
                    y: settings.lines_for(y),
                }
            }
            ScrollDelta::Pixels(pixels) => ScrollDelta::Pixels(settings.direction.apply(pixels)),
            // A unit this build has never heard of is passed on rather than guessed at, which
            // moves the document by whatever the device meant and never by a made-up multiple.
            other => other,
        }
    }

    /// Declares what a scroll from this desktop's devices means.
    ///
    /// Set when the window is opened, from the backend's own answer. Nothing above the platform
    /// seam names a desktop; this is where that answer arrives and the only place it is kept.
    pub(crate) fn set_scroll_settings(&mut self, settings: ScrollSettings) {
        self.scroll_settings = settings;
    }

    /// What this desktop means by a scroll.
    pub fn scroll_settings(&self) -> ScrollSettings {
        self.scroll_settings
    }

    /// How far a line and a page are on one container.
    ///
    /// Both are read from that container's own computed style rather than from a constant: a notch
    /// over a list of 14-pixel rows and a notch over a heading must not move the same distance, and
    /// a constant here is a wheel that feels wrong on every document and is never obviously broken
    /// on any of them.
    pub(super) fn scroll_units(&mut self, container: zgui_dom::NodeKey) -> zgui_input::ScrollUnits {
        let style = self
            .layout
            .borrow()
            .boxes_of(container)
            .first()
            .copied()
            .and_then(|key| self.layout.borrow().get(key).cloned())
            .map(|node| zgui_text_style::lower::set::text_style(&node.style));
        let line = match style {
            Some(style) => self.text.strut(&style).line_height,
            None => {
                self.text
                    .strut(&zgui_text_style::TextStyle::initial())
                    .line_height
            }
        };
        let scrollport = region_of_element(&self.layout.borrow(), container)
            .map_or(CssPx(0.0), |region| {
                CssPx(region.scrollport.size.height.0 / self.scale)
            });
        zgui_input::ScrollUnits::for_scrollport(CssPx(line.0), scrollport)
    }
}
