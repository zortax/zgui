//! What one element is: what it is called, what box it was given, and what the cascade computed.
//!
//! The box model is read every frame, because it is what moves. The computed style is read only
//! when the picked element changes, because reading it means serialising a few hundred longhands
//! and doing that per frame would make the inspector the most expensive thing in the window.

use zgui::geom::{Device, DevicePx, Rect};
use zgui::runtime::Window;
use zgui::view::NodeId;

/// The properties always listed, whatever they compute to.
///
/// The ones a layout question is answered with. Somebody asking why a box is the wrong size is
/// asking about these, and hunting for them among four hundred alphabetical rows is the difference
/// between a tool and a dump.
const ALWAYS: [&str; 24] = [
    "display",
    "position",
    "width",
    "height",
    "min-width",
    "min-height",
    "max-width",
    "max-height",
    "flex-grow",
    "flex-shrink",
    "flex-basis",
    "flex-direction",
    "align-items",
    "justify-content",
    "gap",
    "overflow-x",
    "overflow-y",
    "box-sizing",
    "font-size",
    "font-family",
    "line-height",
    "color",
    "background-color",
    "opacity",
];

/// One computed declaration, as the panel lists it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Declaration {
    /// The property, spelled as a style sheet spells it.
    pub(crate) property: String,
    /// What it computed to.
    pub(crate) value: String,
    /// Whether it computed to something other than its initial value.
    pub(crate) authored: bool,
    /// The colour to paint beside it, when the whole value names one.
    ///
    /// Worked out here rather than while drawing, because it is a question about the value and the
    /// answer does not change until the value does.
    pub(crate) swatch: Option<String>,
}

/// The colour `value` names, when the whole of it names one.
///
/// Deliberately conservative, and the guard is what makes it so: the value has to be *one* colour
/// and nothing else. `background-image: linear-gradient(rgb(0,0,0), rgb(255,255,255))` contains two
/// colours and is not one, and a swatch painted from a value the engine will read differently is
/// worse than no swatch — it is a second answer, quietly disagreeing with the first.
///
/// The text is handed back rather than a parsed colour: the panel paints it by declaring it, so
/// whatever the engine makes of the string is exactly what is shown.
fn swatch(value: &str) -> Option<String> {
    /// The functional notations a computed colour is serialised in.
    const FUNCTIONS: [&str; 9] = [
        "rgb(", "rgba(", "hsl(", "hsla(", "color(", "oklab(", "oklch(", "lab(", "lch(",
    ];

    let value = value.trim();
    if value.eq_ignore_ascii_case("transparent") {
        return Some(value.to_owned());
    }
    if let Some(hex) = value.strip_prefix('#')
        && matches!(hex.len(), 3 | 4 | 6 | 8)
        && hex.chars().all(|digit| digit.is_ascii_hexdigit())
    {
        return Some(value.to_owned());
    }
    let lowered = value.to_ascii_lowercase();
    let named = FUNCTIONS
        .iter()
        .any(|function| lowered.starts_with(function));
    // One call and one only: a value holding two is a gradient, a shadow, or a shorthand, and none
    // of those is a colour this can stand beside.
    let single = value.ends_with(')') && value.matches('(').count() == 1;
    (named && single).then(|| value.to_owned())
}

/// The boxes of the box model, outermost first.
///
/// Three and not four: the margin box is not among them, because a fragment does not carry one. A
/// margin is consumed by the layout that positioned the box and leaves no rectangle behind, so a
/// margin band here would be a number recomputed from the computed style rather than the geometry
/// that was actually used — which is exactly the kind of nearly-right that a box model diagram must
/// not contain. The computed `margin-*` longhands are in the style listing below it.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct BoxModel {
    /// The border box: what a hit test and a background use.
    pub(crate) border: Rect<DevicePx, Device>,
    /// The padding box: the border box shrunk by the borders.
    pub(crate) padding: Rect<DevicePx, Device>,
    /// The content box: the padding box shrunk by the padding.
    pub(crate) content: Rect<DevicePx, Device>,
}

/// Everything the element panel shows about one element.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Element {
    /// Which node this is.
    pub(crate) node: NodeId,
    /// The element's name, as a selector writes it.
    pub(crate) name: String,
    /// Its classes, in the order it carries them.
    pub(crate) classes: Vec<String>,
    /// Its `id`, if it has one.
    pub(crate) id: Option<String>,
    /// The four boxes.
    pub(crate) boxes: BoxModel,
    /// How many boxes the element generated, which is more than one for a wrapped run of text.
    pub(crate) fragments: usize,
    /// What the cascade computed, listed.
    pub(crate) style: Vec<Declaration>,
    /// How this element's own and wrapped vector content is rasterised, when it has any.
    pub(crate) vector: Option<VectorRendering>,
}

/// The actual raster paths used by a picked element's vector content.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct VectorRendering {
    /// Routes selected by the element's own fragments.
    pub(crate) direct: zgui_paint::VectorRoutes,
    /// Routes selected by the element and all descendants, for wrappers such as icon spans.
    pub(crate) subtree: zgui_paint::VectorRoutes,
    /// The configured backend for any general-raster routes.
    pub(crate) backend: Option<zgui::render::VectorBackend>,
    /// Whether the element was present when Vello paid its lazy initialization cost.
    pub(crate) initialized_vello: bool,
}

/// Reads `node` out of `window`, recomputing the style listing only when `restyle` says to.
pub(crate) fn sample_element(window: &Window, node: NodeId, restyle: bool) -> Option<Element> {
    let key = zgui_view_dom::id::to_document(node)?;
    let document = window.document().borrow();
    if !zgui_view_dom::id::is_live(&document, node) {
        return None;
    }
    let index = zgui_view_dom::id::resolve(&document, node);
    let record = document.node(index);
    let name = record.record().local_name().as_str().to_owned();
    let classes: Vec<String> = document
        .store()
        .classes_of(index)
        .iter()
        .map(|class| class.0.as_ref().to_owned())
        .collect();
    let id = record.record().id_attr().map(|id| id.to_string());

    let layout = window.layout().borrow();
    let fragments: Vec<_> = layout
        .boxes_of(key)
        .iter()
        .flat_map(|box_key| layout.fragments_of_box(*box_key))
        .copied()
        .collect();
    // The first fragment, and the count of all of them. A run of text that wrapped is several
    // fragments of one box, and showing the first while saying how many there are is the honest
    // rendering: the panel is about one element, and the element genuinely has more than one box.
    let first = fragments.first().and_then(|key| layout.fragment(*key))?;
    let boxes = BoxModel {
        border: first.border_box,
        padding: first.padding_box,
        content: first.content_box,
    };

    let style = if restyle {
        record.primary_style().map(declarations).unwrap_or_default()
    } else {
        Vec::new()
    };
    let direct = window.vector_routes(key);
    let subtree = window.vector_routes_in_subtree(key);
    let vector = (!subtree.is_empty()).then(|| VectorRendering {
        direct,
        subtree,
        backend: window.renderer().vector_status().backend,
        initialized_vello: window.vello_initializers().contains(&key),
    });
    Some(Element {
        node,
        name,
        classes,
        id,
        boxes,
        fragments: fragments.len(),
        style,
        vector,
    })
}

/// Serialises the properties the panel lists.
///
/// Two groups, and the second is the interesting one: every longhand whose computed value is not
/// the initial one is something *somebody wrote*, directly or through a rule that matched, and that
/// is the list an author is looking for when a box does not look the way they asked for.
fn declarations(style: zgui_css::ComputedStyle) -> Vec<Declaration> {
    let initial = zgui_css::StyleDraft::initial().build();
    let mut out: Vec<Declaration> = Vec::new();
    for property in ALWAYS {
        if let Some(value) = zgui_css::parity::observe::computed_value(&style, property) {
            let authored =
                zgui_css::parity::observe::differs_from_initial(&style, &initial, property);
            out.push(Declaration {
                property: property.to_owned(),
                swatch: swatch(&value),
                value,
                authored,
            });
        }
    }
    let mut authored: Vec<Declaration> = zgui_css::parity::catalog::longhands()
        .iter()
        .filter(|longhand| !longhand.is_alias() && !ALWAYS.contains(&longhand.css_name))
        .filter(|longhand| {
            zgui_css::parity::observe::differs_from_initial(&style, &initial, longhand.css_name)
        })
        .filter_map(|longhand| {
            let value = zgui_css::parity::observe::computed_value(&style, longhand.css_name)?;
            Some(Declaration {
                property: longhand.css_name.to_owned(),
                swatch: swatch(&value),
                value,
                authored: true,
            })
        })
        .collect();
    authored.sort_by(|left, right| left.property.cmp(&right.property));
    out.append(&mut authored);
    out
}

#[cfg(test)]
mod tests {
    use super::swatch;

    #[test]
    fn a_value_that_is_one_colour_is_painted() {
        assert_eq!(
            swatch("rgb(122, 162, 247)").as_deref(),
            Some("rgb(122, 162, 247)")
        );
        assert!(swatch("rgba(0, 0, 0, 0.5)").is_some());
        assert!(swatch("#7aa2f7").is_some());
        assert!(swatch("#7aa2f780").is_some());
        assert!(swatch("transparent").is_some());
        assert!(swatch("oklch(0.7 0.1 250)").is_some());
    }

    #[test]
    fn a_value_that_is_not_a_colour_is_left_alone() {
        assert_eq!(swatch("16px"), None);
        assert_eq!(swatch("block"), None);
        assert_eq!(swatch("#zzz"), None, "not hexadecimal");
        assert_eq!(swatch("#7aa2f"), None, "not a length a hex colour comes in");
    }

    #[test]
    fn a_value_holding_more_than_one_colour_is_not_one() {
        // The case the guard exists for. A swatch painted from the first colour in a gradient is a
        // second answer quietly disagreeing with the value printed beside it.
        assert_eq!(
            swatch("linear-gradient(rgb(0, 0, 0), rgb(255, 255, 255))"),
            None
        );
        assert_eq!(swatch("rgb(0, 0, 0) 0px 1px 2px"), None, "a shadow");
    }
}
