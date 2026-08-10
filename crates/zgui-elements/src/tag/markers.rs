//! One marker type and one builder function per name.

use zgui_interned::ElementName;

use crate::element::Element;
use crate::tag::Tag;

/// Declares a marker type, its name, and the function that starts a builder over it.
macro_rules! elements {
    ($(
        $(#[$meta:meta])*
        $marker:ident = $function:ident, $name:literal;
    )*) => {
        $(
            $(#[$meta])*
            #[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
            pub struct $marker;

            impl Tag for $marker {
                fn name() -> ElementName {
                    ElementName::new($name)
                }
            }

            $(#[$meta])*
            ///
            /// Starts a builder. Nothing is created until the view is built.
            pub fn $function() -> Element<$marker> {
                Element::new()
            }
        )*
    };
}

elements! {
    /// A container that means nothing in particular, laid out as a block.
    ///
    /// The one to reach for when the grouping is structural: something has to hold these three
    /// things together and there is nothing more to say about it.
    Box_ = r#box, "box";

    /// Children in a line, left to right.
    Row = row, "row";

    /// Children in a line, top to bottom.
    Column = column, "column";

    /// Children over one another, in the order they were written.
    ///
    /// Every child is positioned against this element, so a badge over an avatar and an overlay
    /// over an image are one element each rather than a positioning context built by hand.
    Stack = stack, "stack";

    /// A run of text.
    Text = text, "text";

    /// Text that names something else: a field's caption, a control's title.
    ///
    /// Distinct from [`text`](crate::text) so that it can be styled and read out as a name rather
    /// than as prose.
    Label = label, "label";

    /// A picture, sized by what it is a picture of.
    ///
    /// Replaced content: the picture arrives from outside the document, and anything written
    /// inside the element is never laid out.
    Image = image, "image";

    /// Shapes, drawn from paths.
    ///
    /// See [`Element::paths`](crate::Element::paths) for where a shape's colour comes from — which
    /// is not `fill`.
    Vector = vector, "vector";

    /// Content larger than the space there is for it, which the user can move through.
    Scroll = scroll, "scroll";

    /// Shapes the application draws and mutates, kept as a retained scene of its own.
    ///
    /// Unstyled it is 300×150, because a canvas nobody sized and nobody can see reads as broken.
    Canvas = canvas, "canvas";

    /// Text the user changes.
    Editor = editor, "editor";

    /// One value the user enters.
    Field = field, "field";

    /// Something the user operates: a button, a switch, a slider's thumb.
    Control = control, "control";

    /// Pixels another renderer produces, on a texture of zgui's own graphics device.
    ///
    /// Replaced content, exactly as [`image`](crate::image) is: the box is styled, sized, clipped
    /// and composited like any other, and what fills it is named by the producer it is given.
    /// Anything written inside the element is never laid out. A card, a sheet or a menu's body is
    /// a [`box`](Box_) a style sheet raises.
    Surface = surface, "surface";

    /// The space between two things, which takes whatever is left over.
    Spacer = spacer, "spacer";

    /// Where portalled content goes.
    ///
    /// One per window, created by the framework. A view does not normally build one: reaching a
    /// window's own is what [`Portal`](zgui_view::Portal) does.
    OverlayRoot = overlay_root, "overlay_root";
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_name_is_spelled_the_way_a_selector_would_write_it() {
        assert_eq!(Box_::name().as_str(), "box");
        assert_eq!(OverlayRoot::name().as_str(), "overlay_root");
        assert_eq!(Vector::name().as_str(), "vector");
    }

    /// The vocabulary is closed, and its size is what the framework's own style sheet is written
    /// against. A seventeenth name added here and not there is an element with no layout at all.
    #[test]
    fn the_vocabulary_is_the_sixteen_names_the_style_sheet_gives_defaults_to() {
        let names = crate::names();
        assert_eq!(names.len(), 16);
        let mut sorted: Vec<&str> = names.iter().map(|name| name.as_str()).collect();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.len(), 16, "two names collided");
    }
}
