//! A strip of slides showing one at a time.

mod context;
mod parts;
mod style;

pub use crate::carousel::context::{CarouselContext, CarouselSlot};
pub use crate::carousel::parts::{
    CarouselContent, CarouselContentProps, CarouselItem, CarouselItemProps, CarouselNext,
    CarouselNextProps, CarouselPrevious, CarouselPreviousProps,
};
pub use crate::carousel::style::CarouselStyle;

use zgui::prelude::*;
use zgui::reactive::UnsyncCallback;
use zgui::vocab::{Key, NamedKey};
use zgui::{component, view};
use zgui_ui_primitives::{Binding, Controllable, Orientation};

/// What the carousel's rules are installed under.
pub(crate) const SHEET: &str = "zui-carousel";

/// A strip of slides with a way to step through them.
///
/// ```
/// use zgui::prelude::*;
/// use zgui::{component, view};
/// use zgui_ui::prelude::*;
///
/// /// Three pictures, one at a time.
/// #[component]
/// fn Gallery() -> impl IntoView {
///     view! {
///         Carousel(label = "Photographs") {
///             CarouselContent {
///                 CarouselItem {text {"One"}}
///                 CarouselItem {text {"Two"}}
///                 CarouselItem {text {"Three"}}
///             }
///             CarouselPrevious()
///             CarouselNext()
///         }
///     }
/// }
/// ```
///
/// # How it moves
///
/// One step brings exactly one slide to the front, whatever the slides are worth. The distance is
/// measured off the showing slide's own box and reaches the sheet as one custom property, a
/// length; the sheet offsets the track by it. How long the step takes and how it is paced are the
/// sheet's business.
///
/// Slides are as wide as the viewport until a caller gives them a width of their own. Several
/// narrow slides show side by side, and slides of different widths sit together; a step is one
/// slide in both cases, because the distance is measured off the slides rather than divided out of
/// the track.
///
/// # Keyboard
///
/// The arrow keys for the carousel's own axis step back and forward while the focus is anywhere
/// inside it. The other axis is left alone, so a horizontal carousel inside a scrolling page does
/// not swallow the keys that scroll the page.
///
/// # What a reader is told
///
/// That it is a carousel, and that each slide is slide *n* of *m* — which is the only way somebody
/// who cannot see the strip knows there is more of it and how much.
#[component]
pub fn Carousel(
    /// Which slide is showing, when the caller holds it.
    ///
    /// A writable signal is written back to; a value the caller computes needs
    /// [`Binding::controlled`].
    #[prop(into, optional)]
    index: Binding<usize>,
    /// Which one starts showing, when the carousel owns that itself.
    #[prop(default = 0)]
    default_index: usize,
    /// Told whenever the showing slide changes, whoever owns it.
    #[prop(optional)]
    on_change: Option<UnsyncCallback<usize>>,
    /// Which way the slides run.
    #[prop(default = Orientation::Horizontal)]
    orientation: Orientation,
    /// Whether stepping past the last slide goes back to the first.
    #[prop(default = false)]
    wrap: bool,
    /// What the carousel is called, for a reader.
    #[prop(into, optional)]
    label: Option<String>,
    /// Where to record this component's own element.
    #[prop(optional)]
    node_ref: Option<NodeRef>,
    /// Classes merged after the carousel's own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
    /// The track and the arrows.
    children: Children,
) -> impl IntoView {
    install_stylesheet(SHEET, CarouselStyle::CSS);
    let element = node_ref.unwrap_or_default();
    let context = CarouselContext::new(
        Controllable::new(index, default_index, on_change),
        orientation,
        wrap,
    )
    .provide();

    let vertical = matches!(orientation, Orientation::Vertical);
    let mut semantics = A11yBinding::new(Role::Group).role_description("carousel");
    if let Some(text) = label {
        semantics = semantics.label(text);
    }

    let own = Attrs::new()
        .class_toggle(zgui::view::ClassName::new("zui-carousel"), true)
        .attribute(
            zgui::view::AttrName::new("data-orientation"),
            orientation.name(),
        )
        .a11y_from(semantics);

    view! {
        box(
            node_ref = element,
            class = CarouselStyle::CLASS,
            on:key_down = move |ev| {
                let (back, on) = if vertical {
                    (NamedKey::ArrowUp, NamedKey::ArrowDown)
                } else {
                    (NamedKey::ArrowLeft, NamedKey::ArrowRight)
                };
                match &ev.key {
                    Key::Named(key) if *key == back => context.previous(),
                    Key::Named(key) if *key == on => context.next(),
                    _ => return,
                }
                ev.prevent_default();
                ev.stop_propagation();
            },
            {..own},
            {..attrs},
            class = class
        ) {
            {children.into_view_once()}
        }
    }
}
