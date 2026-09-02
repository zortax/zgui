//! What a style sheet says about drawing a box with an application's own shader.
//!
//! There is no `@property` registration in this build, so an effect is named through a custom
//! property — the same route `zgui-fill` and `zgui-text-fill` take, and for the same reason: a
//! custom property's computed value is a token stream rather than a typed value, which is what
//! makes it the non-forking way to feed this engine something it has no property for.
//!
//! Two properties, because an effect does one of two things and the two land in different places:
//!
//! * `--zgui-shader` names a paint effect, which fills the box in place of its background;
//! * `--zgui-shape` names a coverage effect, which reshapes the box the background and the border
//!   are drawn into. A *smoothed corner* is not this: `--zgui-corner-shape` is the engine's own,
//!   and it reaches the shadow, the outline and the clip a box gives its children as well;
//! * `--zgui-filter` and `--zgui-backdrop-filter` name a filter effect, which reads the box's own
//!   content or what is drawn beneath it and writes what replaces it.
//!
//! A filter effect runs after the CSS `filter` chain rather than among it. The two are separate
//! properties, so there is no order between them to honour, and putting the one nothing can look
//! inside last is the arrangement whose result does not depend on where it was written.
//!
//! Parameters come from the cascade beside them, one custom property per field of the effect's own
//! structure, named `--<effect>-<field>`. Resolving them here means resolving them once per
//! distinct style rather than once per element, which is what keeps a page of smoothed corners
//! costing what a page of rounded ones does.
//!
//! # These properties inherit
//!
//! An unregistered custom property inherits, and there is no `@property` registration in this build
//! to say otherwise, so `--zgui-corner-shape` on a card is also on everything inside the card. A
//! descendant that paints no background of its own draws nothing either way, which is the ordinary
//! case and why this is survivable; a descendant that *does* paint one would otherwise take its
//! ancestor's shape without asking. A subtree opts out by writing `none`, which every one of these
//! properties reads as naming no effect at all.

use smallvec::SmallVec;
use zgui_css::ComputedStyle;
use zgui_css::values::custom;
use zgui_scene::ShaderMode;

use zgui_scene::property::{BACKDROP_FILTER, FILTER, SHADER, SHAPE};

/// How many parameters are kept beside a name before the rest are dropped.
///
/// Four floats is the width the shading block holds, and an effect declaring more than that is
/// refused when it is declared rather than here.
const MAX_PARAMETERS: usize = 16;

/// What a style says about drawing its box with an effect.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ShaderStyle {
    /// The effect that draws the box itself, when a style sheet named one.
    pub named: Option<NamedShader>,
    /// The effect that filters the box's own content.
    pub filter: Option<NamedShader>,
    /// The effect that filters whatever is drawn beneath the box.
    pub backdrop: Option<NamedShader>,
}

/// One effect a style sheet named, before anything has resolved it to a handle.
#[derive(Clone, Debug, PartialEq)]
pub struct NamedShader {
    /// The effect's name, as the style sheet wrote it.
    pub name: String,
    /// Which of the two properties named it, and therefore what the effect is expected to do.
    pub mode: ShaderMode,
    /// The parameters read from the cascade, in the order the properties were found.
    ///
    /// Held as a name and a number rather than as bytes, because which byte a field occupies is
    /// the effect's own layout and is not known until something resolves the name.
    pub parameters: SmallVec<[(String, f32); 4]>,
}

impl ShaderStyle {
    /// A style that names no effect.
    pub const NONE: Self = Self {
        named: None,
        filter: None,
        backdrop: None,
    };

    /// Whether nothing at all is named.
    pub fn is_none(&self) -> bool {
        self.named.is_none() && self.filter.is_none() && self.backdrop.is_none()
    }

    /// Whether anything named here needs the box composited into a target of its own.
    ///
    /// A filter does, for the reason every filter does: it reads the content, so the content has
    /// to exist somewhere before it is read. The effect that draws the box needs nothing — it *is*
    /// the box's painting.
    pub fn needs_isolation(&self) -> bool {
        self.filter.is_some() || self.backdrop.is_some()
    }
}

/// What `style` says about drawing its box with an effect.
///
/// `--zgui-corner-shape` wins where both are written: a coverage effect decides the shape the
/// background is drawn into, and a paint effect replaces that background — so honouring both would
/// mean shaping something that is not drawn.
pub fn of(style: &ComputedStyle) -> Option<Box<ShaderStyle>> {
    let found = ShaderStyle {
        named: named(style, SHAPE, ShaderMode::Coverage)
            .or_else(|| named(style, SHADER, ShaderMode::Paint)),
        filter: named(style, FILTER, ShaderMode::Filter),
        backdrop: named(style, BACKDROP_FILTER, ShaderMode::Filter),
    };
    // Boxed, and absent rather than empty when nothing is named. A lowering is cloned for every
    // fragment that carries it — see `faded` — and this structure is five hundred bytes, so a
    // document with no effect in it would otherwise pay half as much again per fragment per frame
    // for three `None`s.
    (!found.is_none()).then(|| Box::new(found))
}

/// The effect `property` names on `style`, with its parameters.
fn named(style: &ComputedStyle, property: &str, mode: ShaderMode) -> Option<NamedShader> {
    let name = custom::text(style, property)?.trim();
    // `none` is how a subtree opts out of a value it inherited, so it has to mean "no effect"
    // rather than "an effect called none".
    if name.is_empty() || name.eq_ignore_ascii_case("none") {
        return None;
    }
    let name = name.to_owned();
    let parameters = parameters(style, &name);
    Some(NamedShader {
        name,
        mode,
        parameters,
    })
}

/// The `--<effect>-<field>` properties on `style`, as far as the block can hold them.
///
/// Which fields exist is the effect's own knowledge and is not available here, so every candidate
/// is read and the ones an effect does not declare are dropped where the name is resolved. That
/// costs a lookup per property written rather than a lookup per field declared, and a style sheet
/// that writes none costs nothing at all.
fn parameters(style: &ComputedStyle, name: &str) -> SmallVec<[(String, f32); 4]> {
    let prefix = format!("{name}-");
    let mut found: SmallVec<[(String, f32); 4]> = SmallVec::new();
    for property in custom::names(style) {
        if found.len() == MAX_PARAMETERS {
            break;
        }
        let Some(field) = property.strip_prefix(prefix.as_str()) else {
            continue;
        };
        // A number first, because a parameter to something outside this engine usually is one; a
        // length after, so `--glow-reach: 12px` says what it looks like it says.
        if let Some(value) =
            custom::number(style, &property).or_else(|| custom::length(style, &property))
        {
            found.push((field.to_owned(), value));
        }
    }
    found
}
