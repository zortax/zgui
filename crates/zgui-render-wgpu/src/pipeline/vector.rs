//! What one composite of a rasterised vector batch is told.

use bytemuck::{Pod, Zeroable};
use zgui_geom::{Device, Rect};

/// One quad of a vector composite.
///
/// A pass is composited either as one of these covering its whole region, or as one per item —
/// which is what lets each item carry its own clip. Which of the two happens is decided in the
/// display list and copied here; a backend that chose for itself could double-blend the overlap
/// between two items reading one scratch.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Pod, Zeroable)]
pub struct VectorInstance {
    /// The quad, in device pixels: origin then extent.
    pub bounds: [f32; 4],
    /// The scratch texel the quad's origin reads, then two unused lanes.
    pub source: [f32; 4],
    /// The clip chain, then three unused lanes.
    pub control: [f32; 4],
}

impl VectorInstance {
    /// A composite of `bounds`, reading the scratch from `source`, through chain `clip`.
    ///
    /// `bounds` is in device pixels of the target being drawn into; `source` is in texels of the
    /// scratch, which one device pixel of the composed target maps onto one for one.
    pub fn new(bounds: Rect<i32, Device>, source: (i32, i32), clip: u32) -> Self {
        Self {
            bounds: [
                bounds.origin.x as f32,
                bounds.origin.y as f32,
                bounds.size.width as f32,
                bounds.size.height as f32,
            ],
            source: [source.0 as f32, source.1 as f32, 0.0, 0.0],
            control: [clip as f32, 0.0, 0.0, 0.0],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::VectorInstance;
    use zgui_geom::{Point, Rect, Size};

    #[test]
    fn a_composite_carries_its_own_clip_and_its_own_corner_of_the_scratch() {
        let instance =
            VectorInstance::new(Rect::new(Point::new(32, 48), Size::new(16, 8)), (16, 32), 7);
        assert_eq!(instance.bounds, [32.0, 48.0, 16.0, 8.0]);
        assert_eq!(instance.source[0], 16.0);
        assert_eq!(instance.source[1], 32.0);
        assert_eq!(
            instance.control[0], 7.0,
            "the clip chain has to reach the shader, or every instance is clipped by a stranger's"
        );
    }
}
