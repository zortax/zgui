//! Turning the variant properties into the OpenType features a shaper is handed.
//!
//! Every property in this module's parent says the same kind of thing in a different vocabulary:
//! turn one of the face's optional behaviours on or off. OpenType already has a vocabulary for
//! that — a four-character tag and a value — and a shaper already takes a list of them. So the
//! properties are *resolved into* that list rather than being carried to the shaper one by one,
//! which is why a shaper needs no code at all per property and why a property cannot be lowered and
//! then forgotten on the way.

use crate::style::face::{FontFeature, tag};
use crate::style::variant::FontVariant;
use crate::style::variant::caps::FontVariantCaps;
use crate::style::variant::east_asian::{EastAsianForms, EastAsianWidth};
use crate::style::variant::kerning::FontKerning;
use crate::style::variant::ligatures::{FontVariantLigatures, LigatureSetting};
use crate::style::variant::numeric::{NumericFigures, NumericFractions, NumericSpacing};
use crate::style::variant::position::FontVariantPosition;

/// The list a resolution is written into.
pub type Features = smallvec::SmallVec<[FontFeature; 4]>;

/// Appends the features `variant` asks for.
///
/// Nothing is appended for a setting that was not asked for, and that is the point rather than an
/// optimisation: a face's own defaults differ per script and per feature, so writing `kern=1` for
/// `font-kerning: auto` would replace the shaper's judgement with a guess, and an author who wrote
/// nothing would get different text from one who wrote nothing on a different platform.
pub fn append(variant: &FontVariant, out: &mut Features) {
    match variant.kerning {
        FontKerning::Auto => {}
        FontKerning::Normal => push(out, b"kern", 1),
        FontKerning::None => push(out, b"kern", 0),
    }
    ligatures(&variant.ligatures, out);
    if variant.caps == FontVariantCaps::SmallCaps {
        push(out, b"smcp", 1);
    }
    match variant.position {
        FontVariantPosition::Normal => {}
        FontVariantPosition::Sub => push(out, b"subs", 1),
        FontVariantPosition::Super => push(out, b"sups", 1),
    }
    numeric(variant, out);
    east_asian(variant, out);
}

/// The four ligature groups.
///
/// Common ligatures are two tags rather than one: `liga` is the standard set and `clig` the
/// contextual part of it, and CSS names both with the single keyword `common-ligatures`, so turning
/// the group off has to turn off both or the contextual half survives.
fn ligatures(ligatures: &FontVariantLigatures, out: &mut Features) {
    for (setting, tags) in [
        (ligatures.common, &[b"liga", b"clig"][..]),
        (ligatures.discretionary, &[b"dlig"][..]),
        (ligatures.historical, &[b"hlig"][..]),
        (ligatures.contextual, &[b"calt"][..]),
    ] {
        let value = match setting {
            LigatureSetting::Auto => continue,
            LigatureSetting::On => 1,
            LigatureSetting::Off => 0,
        };
        for name in tags {
            push(out, name, value);
        }
    }
}

/// The figure, spacing, fraction, ordinal and slashed-zero features.
fn numeric(variant: &FontVariant, out: &mut Features) {
    match variant.numeric.figures {
        NumericFigures::Auto => {}
        NumericFigures::Lining => push(out, b"lnum", 1),
        NumericFigures::Oldstyle => push(out, b"onum", 1),
    }
    match variant.numeric.spacing {
        NumericSpacing::Auto => {}
        NumericSpacing::Proportional => push(out, b"pnum", 1),
        NumericSpacing::Tabular => push(out, b"tnum", 1),
    }
    match variant.numeric.fractions {
        NumericFractions::Auto => {}
        NumericFractions::Diagonal => push(out, b"frac", 1),
        NumericFractions::Stacked => push(out, b"afrc", 1),
    }
    if variant.numeric.ordinal {
        push(out, b"ordn", 1);
    }
    if variant.numeric.slashed_zero {
        push(out, b"zero", 1);
    }
}

/// The national-form, width and ruby features.
fn east_asian(variant: &FontVariant, out: &mut Features) {
    match variant.east_asian.forms {
        EastAsianForms::Auto => {}
        EastAsianForms::Jis78 => push(out, b"jp78", 1),
        EastAsianForms::Jis83 => push(out, b"jp83", 1),
        EastAsianForms::Jis90 => push(out, b"jp90", 1),
        EastAsianForms::Jis04 => push(out, b"jp04", 1),
        EastAsianForms::Simplified => push(out, b"smpl", 1),
        EastAsianForms::Traditional => push(out, b"trad", 1),
    }
    match variant.east_asian.width {
        EastAsianWidth::Auto => {}
        EastAsianWidth::FullWidth => push(out, b"fwid", 1),
        EastAsianWidth::ProportionalWidth => push(out, b"pwid", 1),
    }
    if variant.east_asian.ruby {
        push(out, b"ruby", 1);
    }
}

/// Appends one feature.
fn push(out: &mut Features, name: &[u8; 4], value: u32) {
    out.push(FontFeature {
        tag: tag(name),
        value,
    });
}
