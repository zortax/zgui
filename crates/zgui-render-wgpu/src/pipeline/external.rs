//! What one rectangle showing a texture the renderer did not draw is told.

use bytemuck::{Pod, Zeroable};
use zgui_render::ExternalTexture;
use zgui_scene::ExternalQuad;

/// The block one external quad reads.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Pod, Zeroable)]
pub struct ExternalParams {
    /// The quad, in device pixels: origin then extent.
    pub bounds: [f32; 4],
    /// The clip chain, the transform, the opacity, and whether the texture is already
    /// premultiplied.
    pub control: [f32; 4],
}

impl ExternalParams {
    /// The block for `quad` showing `texture`.
    pub fn of(quad: &ExternalQuad, texture: &ExternalTexture) -> Self {
        Self {
            bounds: [
                quad.bounds.origin.x.0,
                quad.bounds.origin.y.0,
                quad.bounds.size.width.0,
                quad.bounds.size.height.0,
            ],
            control: [
                quad.clip.0 as f32,
                quad.transform.index() as f32,
                quad.opacity.clamp(0.0, 1.0),
                f32::from(texture.premultiplied),
            ],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::ExternalParams;
    use zgui_geom::{Device, DevicePx, Point, Rect, Size};
    use zgui_render::{ExternalTexture, TextureHandle};
    use zgui_scene::{ExternalQuad, ExternalTextureId};

    fn texture(premultiplied: bool) -> ExternalTexture {
        ExternalTexture {
            id: ExternalTextureId(1),
            handle: TextureHandle(1),
            size: Size::new(8, 8),
            premultiplied,
        }
    }

    #[test]
    fn a_texture_that_is_not_premultiplied_says_so_where_the_shader_can_see_it() {
        let bounds: Rect<DevicePx, Device> = Rect::new(
            Point::new(DevicePx(0.0), DevicePx(0.0)),
            Size::new(DevicePx(8.0), DevicePx(8.0)),
        );
        let quad = ExternalQuad::new(bounds, ExternalTextureId(1));
        assert_eq!(ExternalParams::of(&quad, &texture(false)).control[3], 0.0);
        assert_eq!(ExternalParams::of(&quad, &texture(true)).control[3], 1.0);
    }
}
