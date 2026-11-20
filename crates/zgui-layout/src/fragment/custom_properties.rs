//! Which of the framework's own custom properties a style declares.
//!
//! A custom property is read by name, and each read walks the style's property maps. The fragment
//! pass and the paint walk ask the same few questions of every box on every frame, and nearly every
//! box declares none of these properties. The store records the answer once per distinct style
//! (see [`LayoutStore::custom_properties`](crate::LayoutStore::custom_properties)), and a reader
//! skips each read whose property is absent.

use zgui_css::ComputedStyle;
use zgui_css::values::custom;

/// The set of the framework's own custom properties that one style declares.
///
/// A property is in the set when a read of it on the style finds a value, inherited or its own.
/// A reader that has no recorded set uses [`CustomProperties::ALL`], which makes every read look.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CustomProperties(u8);

impl CustomProperties {
    /// No property declared.
    pub const NONE: Self = Self(0);
    /// Every property possibly declared, so that each read looks.
    pub const ALL: Self = Self(u8::MAX);

    const SHADER: u8 = 1 << 0;
    const SHAPE: u8 = 1 << 1;
    const FILTER: u8 = 1 << 2;
    const BACKDROP_FILTER: u8 = 1 << 3;
    const CORNER_SHAPE: u8 = 1 << 4;
    const EFFECTS: u8 = Self::SHADER | Self::SHAPE | Self::FILTER | Self::BACKDROP_FILTER;

    /// The properties `style` declares.
    pub fn of(style: &ComputedStyle) -> Self {
        let mut bits = 0;
        for (property, bit) in [
            (&custom::SHADER, Self::SHADER),
            (&custom::SHAPE, Self::SHAPE),
            (&custom::FILTER, Self::FILTER),
            (&custom::BACKDROP_FILTER, Self::BACKDROP_FILTER),
            (
                &crate::fragment::corner::CORNER_SHAPE_PROPERTY,
                Self::CORNER_SHAPE,
            ),
        ] {
            if property.is_declared(style) {
                bits |= bit;
            }
        }
        Self(bits)
    }

    /// Whether `--zgui-filter` is possibly declared.
    pub const fn filter(self) -> bool {
        self.0 & Self::FILTER != 0
    }

    /// Whether `--zgui-backdrop-filter` is possibly declared.
    pub const fn backdrop_filter(self) -> bool {
        self.0 & Self::BACKDROP_FILTER != 0
    }

    /// Whether `--zgui-corner-shape` is possibly declared.
    pub const fn corner_shape(self) -> bool {
        self.0 & Self::CORNER_SHAPE != 0
    }

    /// Whether any of the
    /// [`WILDCARD_DECLARERS`](zgui_css::values::custom::WILDCARD_DECLARERS) is possibly declared.
    pub const fn any_effect(self) -> bool {
        self.0 & Self::EFFECTS != 0
    }
}

#[cfg(test)]
mod tests {
    use zgui_css::StyleDraft;
    use zgui_css::values::custom;

    use super::CustomProperties;

    #[test]
    fn a_style_without_framework_properties_declares_none() {
        let style = StyleDraft::initial()
            .with_custom_property("accent", "red")
            .build();
        assert_eq!(CustomProperties::of(&style), CustomProperties::NONE);
        assert!(!CustomProperties::NONE.any_effect());
        assert!(CustomProperties::ALL.filter() && CustomProperties::ALL.corner_shape());
    }

    #[test]
    fn each_bit_answers_what_a_read_of_its_property_answers() {
        for name in custom::WILDCARD_DECLARERS
            .into_iter()
            .chain([crate::fragment::corner::CORNER_SHAPE])
        {
            let style = StyleDraft::initial()
                .with_custom_property(name, "none")
                .build();
            let declared = CustomProperties::of(&style);
            assert_eq!(
                declared.filter(),
                custom::text(&style, custom::FILTER.as_str()).is_some()
            );
            assert_eq!(
                declared.backdrop_filter(),
                custom::text(&style, custom::BACKDROP_FILTER.as_str()).is_some()
            );
            assert_eq!(
                declared.corner_shape(),
                custom::text(&style, crate::fragment::corner::CORNER_SHAPE).is_some()
            );
            assert_eq!(
                declared.any_effect(),
                custom::WILDCARD_DECLARERS
                    .iter()
                    .any(|name| custom::text(&style, name).is_some())
            );
            assert_ne!(declared, CustomProperties::NONE, "{name}");
        }
    }

    /// A box under a filtered or shaped ancestor inherits the property, and its set says so.
    #[test]
    fn an_inherited_property_is_declared() {
        use zgui_geom::{DevicePx, Point, Rect, Size};

        use crate::fragment::{corner, filter, ink};

        let parent = StyleDraft::initial()
            .with_custom_property(custom::FILTER.as_str(), "none")
            .with_custom_property(corner::CORNER_SHAPE, "squircle")
            .build();
        let child = zgui_css::inherited_style(&parent);
        let declared = CustomProperties::of(&child);
        assert!(declared.filter() && declared.corner_shape() && declared.any_effect());
        assert!(!declared.backdrop_filter());
        assert_eq!(
            corner::shape_with(&child, declared),
            corner::shape_with(&child, CustomProperties::ALL)
        );
        assert_eq!(
            corner::shape_with(&child, declared),
            zgui_scene::CornerShape::SQUIRCLE
        );
        let border_box = Rect::new(
            Point::new(DevicePx(0.0), DevicePx(0.0)),
            Size::new(DevicePx(10.0), DevicePx(10.0)),
        );
        assert_eq!(
            ink::of(&child, declared, border_box, 1.0),
            ink::of(&child, CustomProperties::ALL, border_box, 1.0)
        );
        assert_eq!(
            filter::reads_outside(&child, declared, 1.0),
            filter::reads_outside(&child, CustomProperties::ALL, 1.0)
        );
    }

    #[test]
    fn the_interned_effect_names_are_the_scene_effect_names() {
        use zgui_scene::property;
        assert_eq!(custom::SHADER.as_str(), property::SHADER);
        assert_eq!(custom::SHAPE.as_str(), property::SHAPE);
        assert_eq!(custom::FILTER.as_str(), property::FILTER);
        assert_eq!(custom::BACKDROP_FILTER.as_str(), property::BACKDROP_FILTER);
    }

    /// The store answers for the style a box holds now, through every way a box gets a style.
    #[test]
    fn the_store_answers_for_the_style_a_box_holds() {
        use zgui_arena::DocumentId;

        use crate::node::box_node::BoxNode;
        use crate::node::kind::{BoxKind, FormattingContext};
        use crate::tree::store::LayoutStore;

        let plain = StyleDraft::initial()
            .with_custom_property("accent", "red")
            .build();
        let filtered = StyleDraft::initial()
            .with_custom_property("zgui-filter", "glow")
            .build();
        let held = |store: &LayoutStore, key| store.custom_properties(key, &store.node(key).style);
        let mut store = LayoutStore::new(DocumentId::FIRST);
        let key = store.insert(BoxNode::new(
            plain.clone(),
            BoxKind::Element,
            FormattingContext::Block,
        ));
        assert_eq!(held(&store, key), CustomProperties::NONE);

        assert!(store.set_style(key, &filtered));
        assert_eq!(held(&store, key), CustomProperties::of(&filtered));
        assert!(held(&store, key).filter());

        assert!(store.set_style(key, &plain));
        assert_eq!(held(&store, key), CustomProperties::NONE);

        // A style other than the one the slot names has no recorded set, so every read looks.
        assert_eq!(
            store.custom_properties(key, &filtered),
            CustomProperties::ALL
        );
        store.get_mut(key).expect("live").style = filtered;
        assert_eq!(held(&store, key), CustomProperties::ALL);
    }
}
