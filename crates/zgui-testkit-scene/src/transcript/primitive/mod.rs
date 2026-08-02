//! One primitive, on one line.
//!
//! One line per primitive rather than a nested block, because the unit a reviewer compares is the
//! primitive: a line that moved is a primitive that moved. Fields at their default value are
//! omitted, so a diff shows what changed instead of a wall of zeroes — and every field that can
//! change what is drawn is present, because a field left out is a field a golden cannot see
//! regress.

pub mod boxes;
pub mod group;
pub mod sprite;
pub mod style;
pub mod vector;

use zgui_geom::Matrix4;
use zgui_scene::{ClipId, Scene, SpatialId};

use crate::text::number::list;
use crate::transcript::clip;

pub use crate::transcript::primitive::boxes::{decoration, quad, shadow};
pub use crate::transcript::primitive::group::{backdrop, filter, filters, group};
pub use crate::transcript::primitive::sprite::{color_sprite, mono_sprite, subpixel_sprite};
pub use crate::transcript::primitive::vector::{external, vector};

/// The clip and the coordinate system every primitive ends with, omitted where they are the
/// defaults.
///
/// Omitting them is what keeps a line readable, and it is safe in exactly one direction: the
/// default clip admits everything and a coordinate system that resolves to the identity moves
/// nothing, so an absent field is unambiguous rather than unknown.
///
/// The *resolved* matrix decides that, not the name: a coordinate system exists for every box that
/// is anchored differently from the one above it, and a sticky box inside nothing that moves is
/// drawn exactly where an unanchored one would be.
pub fn suffix(scene: &Scene, clip: ClipId, transform: Option<SpatialId>) -> String {
    let mut text = String::new();
    if !clip.is_root() {
        text.push_str(&format!(" clip={}", clip::chain(&scene.clips, clip)));
    }
    let Some(id) = transform else {
        return text;
    };
    let resolved = scene.spatial.resolve(id);
    if resolved == Some(Matrix4::IDENTITY) {
        return text;
    }
    let matrix = match resolved {
        Some(matrix) => {
            let columns: Vec<String> = matrix.columns.iter().map(|column| list(column)).collect();
            format!("[{}]", columns.join(", "))
        }
        None => "<missing>".to_owned(),
    };
    text.push_str(&format!(" transform=#{} {matrix}", id.index()));
    text
}
